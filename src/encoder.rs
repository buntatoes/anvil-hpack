// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Buntato
//! Header block encoding (RFC 7541 section 6).

use crate::huffman::{huffman_encode, huffman_encoded_len};
use crate::integer::encode_integer;
use crate::table::{DynamicTable, entry_size};
use crate::{DEFAULT_TABLE_SIZE, Header};

/// Encodes the header blocks of one connection, in order.
///
/// Fields that match the static or dynamic table are sent as an index.
/// Other fields are sent as literals and added to the dynamic table, unless
/// they are marked never-indexed or are too large to be worth keeping.
/// Strings use the Huffman code when it is not longer than the plain bytes.
#[derive(Clone, Debug)]
pub struct Encoder {
    table: DynamicTable,
    pending_size_update: Option<usize>,
    /// Smallest size set since the last block. RFC 7541 section 4.2: when the
    /// size went below the final one, the next block signals this one first.
    pending_min_size: Option<usize>,
    use_huffman: bool,
}

impl Default for Encoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Encoder {
    /// An encoder with the default 4096 byte table.
    pub fn new() -> Self {
        Self {
            table: DynamicTable::new(DEFAULT_TABLE_SIZE),
            pending_size_update: None,
            pending_min_size: None,
            use_huffman: true,
        }
    }

    /// An encoder whose table starts at `size` bytes, for when both sides
    /// already agreed on that size and no size update is needed.
    pub fn with_table_size(size: usize) -> Self {
        Self {
            table: DynamicTable::new(size),
            ..Self::new()
        }
    }

    /// Turn Huffman coding of strings on or off.
    pub fn set_huffman(&mut self, on: bool) {
        self.use_huffman = on;
    }

    /// The dynamic table this encoder believes the peer holds.
    pub fn table(&self) -> &DynamicTable {
        &self.table
    }

    /// Use a table of `size` bytes from the next block on. Call this when the
    /// peer's `SETTINGS_HEADER_TABLE_SIZE` arrives. The next block starts with
    /// a size update so both sides agree.
    pub fn set_max_table_size(&mut self, size: usize) {
        self.table.set_max_size(size);
        self.pending_size_update = Some(size);
        self.pending_min_size = Some(self.pending_min_size.map_or(size, |min| min.min(size)));
    }

    /// Encode `headers` as one header block and append it to `out`.
    pub fn encode(&mut self, headers: &[Header], out: &mut Vec<u8>) {
        if let Some(size) = self.pending_size_update.take() {
            // The decoder evicts to the smallest size, then grows to the last.
            if let Some(min) = self.pending_min_size.take().filter(|&min| min < size) {
                encode_integer(min, 5, 0x20, out);
            }
            encode_integer(size, 5, 0x20, out);
        }
        for header in headers {
            self.encode_field(header, out);
        }
    }

    fn encode_field(&mut self, header: &Header, out: &mut Vec<u8>) {
        let found = self.table.find(&header.name, &header.value);
        if header.never_indexed {
            let name_index = found.map_or(0, |(index, _)| index);
            encode_integer(name_index, 4, 0x10, out);
            if name_index == 0 {
                self.write_string(&header.name, out);
            }
            self.write_string(&header.value, out);
            return;
        }
        if let Some((index, true)) = found {
            encode_integer(index, 7, 0x80, out);
            return;
        }
        let name_index = found.map_or(0, |(index, _)| index);
        // Large fields would push everything else out of the table.
        let worth_indexing = entry_size(&header.name, &header.value) <= self.table.max_size() / 2;
        if worth_indexing {
            encode_integer(name_index, 6, 0x40, out);
        } else {
            encode_integer(name_index, 4, 0x00, out);
        }
        if name_index == 0 {
            self.write_string(&header.name, out);
        }
        self.write_string(&header.value, out);
        if worth_indexing {
            self.table.insert(header.name.clone(), header.value.clone());
        }
    }

    fn write_string(&self, bytes: &[u8], out: &mut Vec<u8>) {
        let huffman_len = huffman_encoded_len(bytes);
        if self.use_huffman && huffman_len <= bytes.len() {
            encode_integer(huffman_len, 7, 0x80, out);
            huffman_encode(bytes, out);
        } else {
            encode_integer(bytes.len(), 7, 0x00, out);
            out.extend_from_slice(bytes);
        }
    }
}

// SPDX-License-Identifier: LicenseRef-PolyForm-Noncommercial-1.0.0-OR-Commercial
// Copyright (c) 2026 Buntato
//! Header block decoding (RFC 7541 section 6).

use crate::huffman::huffman_decode;
use crate::integer::decode_integer;
use crate::table::{DynamicTable, entry_size};
use crate::{DEFAULT_MAX_HEADER_LIST_SIZE, DEFAULT_TABLE_SIZE, Header, HpackError, MAX_STRING_LEN};

/// Decodes the header blocks of one connection, in order.
///
/// The dynamic table carries over from block to block, so every block the
/// peer sends must go through the same decoder. After an error the table
/// may be out of step with the peer, and the connection should be closed.
#[derive(Clone, Debug)]
pub struct Decoder {
    table: DynamicTable,
    max_allowed_table_size: usize,
    max_header_list_size: usize,
}

impl Default for Decoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Decoder {
    /// A decoder with the default table size and header list cap.
    pub fn new() -> Self {
        Self::with_limits(DEFAULT_TABLE_SIZE, DEFAULT_MAX_HEADER_LIST_SIZE)
    }

    /// A decoder that lets the peer use up to `max_table_size` bytes of
    /// dynamic table and accepts header lists up to `max_header_list_size`.
    pub fn with_limits(max_table_size: usize, max_header_list_size: usize) -> Self {
        Self {
            table: DynamicTable::new(max_table_size),
            max_allowed_table_size: max_table_size,
            max_header_list_size,
        }
    }

    /// The dynamic table as it stands after the last block.
    pub fn table(&self) -> &DynamicTable {
        &self.table
    }

    /// Change the largest table size the peer may ask for. Use this when the
    /// local `SETTINGS_HEADER_TABLE_SIZE` changes and the peer has acknowledged it.
    pub fn set_max_allowed_table_size(&mut self, size: usize) {
        self.max_allowed_table_size = size;
        if self.table.max_size() > size {
            self.table.set_max_size(size);
        }
    }

    /// Decode one complete header block.
    pub fn decode(&mut self, block: &[u8]) -> Result<Vec<Header>, HpackError> {
        let mut headers = Vec::new();
        let mut list_size = 0usize;
        let mut pos = 0usize;
        let mut seen_field = false;
        while pos < block.len() {
            let first = block[pos];
            let header = if first & 0x80 != 0 {
                // Indexed field.
                let index = decode_integer(block, &mut pos, 7)?;
                let (name, value) = self
                    .table
                    .get(index)
                    .ok_or(HpackError::InvalidIndex(index))?;
                Header {
                    name: name.to_vec(),
                    value: value.to_vec(),
                    never_indexed: false,
                }
            } else if first & 0xc0 == 0x40 {
                // Literal with incremental indexing.
                let header = self.read_literal(block, &mut pos, 6, false)?;
                self.table.insert(header.name.clone(), header.value.clone());
                header
            } else if first & 0xe0 == 0x20 {
                // Dynamic table size update. Only allowed before the first field.
                if seen_field {
                    return Err(HpackError::LateTableSizeUpdate);
                }
                let size = decode_integer(block, &mut pos, 5)?;
                if size > self.max_allowed_table_size {
                    return Err(HpackError::TableSizeTooLarge(size));
                }
                self.table.set_max_size(size);
                continue;
            } else if first & 0xf0 == 0x10 {
                // Literal never indexed.
                self.read_literal(block, &mut pos, 4, true)?
            } else {
                // Literal without indexing.
                self.read_literal(block, &mut pos, 4, false)?
            };
            seen_field = true;
            list_size = list_size.saturating_add(entry_size(&header.name, &header.value));
            if list_size > self.max_header_list_size {
                return Err(HpackError::HeaderListTooLarge(list_size));
            }
            headers.push(header);
        }
        Ok(headers)
    }

    /// A literal field: a name index (or 0 and a literal name), then a value.
    fn read_literal(
        &self,
        block: &[u8],
        pos: &mut usize,
        prefix_bits: u8,
        never_indexed: bool,
    ) -> Result<Header, HpackError> {
        let index = decode_integer(block, pos, prefix_bits)?;
        let name = if index == 0 {
            read_string(block, pos)?
        } else {
            let (name, _) = self
                .table
                .get(index)
                .ok_or(HpackError::InvalidIndex(index))?;
            name.to_vec()
        };
        let value = read_string(block, pos)?;
        Ok(Header {
            name,
            value,
            never_indexed,
        })
    }
}

/// A string literal: a Huffman flag, a 7-bit prefixed length, then the bytes.
fn read_string(block: &[u8], pos: &mut usize) -> Result<Vec<u8>, HpackError> {
    let huffman = block.get(*pos).ok_or(HpackError::Truncated)? & 0x80 != 0;
    let len = decode_integer(block, pos, 7)?;
    if len > MAX_STRING_LEN {
        return Err(HpackError::StringTooLong(len));
    }
    let end = pos.checked_add(len).ok_or(HpackError::Truncated)?;
    let raw = block.get(*pos..end).ok_or(HpackError::Truncated)?;
    *pos = end;
    if huffman {
        huffman_decode(raw, MAX_STRING_LEN)
    } else {
        Ok(raw.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_bad_indexes_and_late_size_updates() {
        let mut decoder = Decoder::new();
        assert_eq!(decoder.decode(&[0x80]), Err(HpackError::InvalidIndex(0)));
        assert_eq!(decoder.decode(&[0xbe]), Err(HpackError::InvalidIndex(62)));
        assert_eq!(
            decoder.decode(&[0x82, 0x20]),
            Err(HpackError::LateTableSizeUpdate)
        );
        // 0x3f 0xe1 0x1f = 4096 + 1 bytes, over the default limit.
        assert_eq!(
            decoder.decode(&[0x3f, 0xe2, 0x1f]),
            Err(HpackError::TableSizeTooLarge(4097))
        );
        assert_eq!(decoder.decode(&[0x20, 0x82]).unwrap().len(), 1);
        assert_eq!(decoder.table().max_size(), 0);
    }

    #[test]
    fn caps_the_header_list() {
        let mut decoder = Decoder::with_limits(4096, 64);
        // :method GET is 7 + 3 + 32 = 42 bytes; two of them are 84.
        assert!(decoder.decode(&[0x82]).is_ok());
        assert!(matches!(
            decoder.decode(&[0x82, 0x82]),
            Err(HpackError::HeaderListTooLarge(84))
        ));
    }

    #[test]
    fn rejects_strings_that_run_past_the_block() {
        let mut decoder = Decoder::new();
        assert_eq!(
            decoder.decode(&[0x40, 0x05, b'a']),
            Err(HpackError::Truncated)
        );
        assert_eq!(decoder.decode(&[0x04]), Err(HpackError::Truncated));
        assert!(matches!(
            decoder.decode(&[0x00, 0x7f, 0xff, 0xff, 0x03]),
            Err(HpackError::StringTooLong(_))
        ));
    }
}

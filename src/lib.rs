// SPDX-License-Identifier: LicenseRef-PolyForm-Noncommercial-1.0.0-OR-Commercial
// Copyright (c) 2026 Buntato
//! HPACK header compression for HTTP/2 (RFC 7541).
//!
//! A [`Decoder`] turns one header block into a list of [`Header`]s and keeps
//! the dynamic table between blocks of the same connection. An [`Encoder`]
//! does the reverse. Both follow the static table and Huffman code from the
//! RFC's appendices.
//!
//! Every input is treated as hostile. Integers that overflow, strings that run
//! past the block, bad Huffman padding, an end-of-string code inside a string,
//! a table index that does not exist, a table size above the agreed limit, and
//! header lists above the caller's size cap are all errors, never panics.
//!
//! The library has no dependencies.

mod decoder;
mod encoder;
mod huffman;
mod integer;
mod table;
mod tables;

pub use decoder::Decoder;
pub use encoder::Encoder;
pub use huffman::{huffman_decode, huffman_encode, huffman_encoded_len};
pub use integer::{decode_integer, encode_integer};
pub use table::{DynamicTable, STATIC_TABLE_LEN, entry_size, static_entry};

/// Dynamic table size both sides start with (`SETTINGS_HEADER_TABLE_SIZE`).
pub const DEFAULT_TABLE_SIZE: usize = 4096;

/// Default cap on the decoded header list, counted the way RFC 7540 counts
/// `SETTINGS_MAX_HEADER_LIST_SIZE`: name plus value plus 32 for each field.
pub const DEFAULT_MAX_HEADER_LIST_SIZE: usize = 256 * 1024;

/// Longest single name or value the decoder accepts, before or after Huffman decoding.
pub const MAX_STRING_LEN: usize = 64 * 1024;

/// Largest integer the decoder accepts. Anything bigger is an error.
pub const MAX_INTEGER: usize = u32::MAX as usize;

/// One header field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    /// Field name as sent. HTTP/2 names are lowercase.
    pub name: Vec<u8>,
    /// Field value as sent.
    pub value: Vec<u8>,
    /// The sender marked this field "never indexed". An intermediary must
    /// forward it the same way, and an encoder must not add it to a table.
    pub never_indexed: bool,
}

impl Header {
    /// A field from text, not marked never-indexed.
    pub fn new(name: &str, value: &str) -> Self {
        Self {
            name: name.as_bytes().to_vec(),
            value: value.as_bytes().to_vec(),
            never_indexed: false,
        }
    }

    /// A field that must never be stored in a table, such as a credential.
    pub fn sensitive(name: &str, value: &str) -> Self {
        Self {
            never_indexed: true,
            ..Self::new(name, value)
        }
    }

    /// Name as text. Bytes that are not UTF-8 are replaced.
    pub fn name_str(&self) -> String {
        String::from_utf8_lossy(&self.name).into_owned()
    }

    /// Value as text. Bytes that are not UTF-8 are replaced.
    pub fn value_str(&self) -> String {
        String::from_utf8_lossy(&self.value).into_owned()
    }

    /// Size of this field for table and list-size accounting (name + value + 32).
    pub fn size(&self) -> usize {
        entry_size(&self.name, &self.value)
    }
}

/// Why a header block could not be decoded or encoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HpackError {
    /// The block ended in the middle of a field.
    Truncated,
    /// An integer is larger than [`MAX_INTEGER`] or uses too many bytes.
    IntegerOverflow,
    /// Index 0, or an index past the static and dynamic tables.
    InvalidIndex(usize),
    /// A Huffman string has bad padding or contains the end-of-string code.
    InvalidHuffman,
    /// A name or value is longer than [`MAX_STRING_LEN`].
    StringTooLong(usize),
    /// A dynamic table size update is above the limit the decoder allows.
    TableSizeTooLarge(usize),
    /// A dynamic table size update came after the first field of the block.
    LateTableSizeUpdate,
    /// The decoded list is larger than the caller's cap.
    HeaderListTooLarge(usize),
}

impl std::fmt::Display for HpackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Truncated => write!(f, "header block ends in the middle of a field"),
            Self::IntegerOverflow => write!(f, "header block integer is too large"),
            Self::InvalidIndex(index) => write!(f, "header table index {index} does not exist"),
            Self::InvalidHuffman => write!(f, "header string has a bad Huffman code"),
            Self::StringTooLong(len) => write!(f, "header string of {len} bytes is too long"),
            Self::TableSizeTooLarge(size) => {
                write!(f, "header table size {size} is above the limit")
            }
            Self::LateTableSizeUpdate => {
                write!(f, "header table size update after the first field")
            }
            Self::HeaderListTooLarge(size) => {
                write!(f, "header list of {size} bytes is above the limit")
            }
        }
    }
}

impl std::error::Error for HpackError {}

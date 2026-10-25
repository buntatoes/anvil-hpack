// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Buntato
//! The static table and the dynamic table (RFC 7541 sections 2.3 and 4).

use std::collections::VecDeque;

use crate::tables::STATIC_TABLE;

/// Number of static table entries. Dynamic entries start at index 62.
pub const STATIC_TABLE_LEN: usize = STATIC_TABLE.len();

/// Size of one entry for table accounting: name + value + 32 bytes.
pub fn entry_size(name: &[u8], value: &[u8]) -> usize {
    name.len() + value.len() + 32
}

/// Static entry `index` (1 through 61).
pub fn static_entry(index: usize) -> Option<(&'static str, &'static str)> {
    index
        .checked_sub(1)
        .and_then(|i| STATIC_TABLE.get(i))
        .copied()
}

/// The dynamic table of one side of a connection. Newest entry first.
#[derive(Clone, Debug)]
pub struct DynamicTable {
    entries: VecDeque<(Vec<u8>, Vec<u8>)>,
    size: usize,
    max_size: usize,
}

impl DynamicTable {
    /// An empty table that may grow to `max_size` bytes.
    pub fn new(max_size: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            size: 0,
            max_size,
        }
    }

    /// Current size in bytes, counted with [`entry_size`].
    pub fn size(&self) -> usize {
        self.size
    }

    /// Size this table may grow to.
    pub fn max_size(&self) -> usize {
        self.max_size
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when the table has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Change the size limit and evict until the table fits.
    pub fn set_max_size(&mut self, max_size: usize) {
        self.max_size = max_size;
        self.evict_to(max_size);
    }

    /// Add an entry at the front, evicting old entries to make room. An entry
    /// larger than the whole table empties the table and is not stored.
    pub fn insert(&mut self, name: Vec<u8>, value: Vec<u8>) {
        let size = entry_size(&name, &value);
        if size > self.max_size {
            self.entries.clear();
            self.size = 0;
            return;
        }
        self.evict_to(self.max_size - size);
        self.size += size;
        self.entries.push_front((name, value));
    }

    /// Entry at combined `index` (static entries first, then this table).
    pub fn get(&self, index: usize) -> Option<(&[u8], &[u8])> {
        if index == 0 {
            return None;
        }
        if let Some((name, value)) = static_entry(index) {
            return Some((name.as_bytes(), value.as_bytes()));
        }
        let (name, value) = self.entries.get(index - STATIC_TABLE_LEN - 1)?;
        Some((name.as_slice(), value.as_slice()))
    }

    /// Best combined index for a field: an exact match if there is one,
    /// otherwise a name match. The bool is true for an exact match.
    pub fn find(&self, name: &[u8], value: &[u8]) -> Option<(usize, bool)> {
        let mut name_match = None;
        for (i, (n, v)) in STATIC_TABLE.iter().enumerate() {
            if n.as_bytes() == name {
                if v.as_bytes() == value {
                    return Some((i + 1, true));
                }
                name_match.get_or_insert(i + 1);
            }
        }
        for (i, (n, v)) in self.entries.iter().enumerate() {
            if n.as_slice() == name {
                let index = STATIC_TABLE_LEN + 1 + i;
                if v.as_slice() == value {
                    return Some((index, true));
                }
                name_match.get_or_insert(index);
            }
        }
        name_match.map(|index| (index, false))
    }

    fn evict_to(&mut self, limit: usize) {
        while self.size > limit {
            match self.entries.pop_back() {
                Some((name, value)) => self.size -= entry_size(&name, &value),
                None => {
                    self.size = 0;
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_table_has_the_rfc_entries() {
        assert_eq!(STATIC_TABLE_LEN, 61);
        assert_eq!(static_entry(1), Some((":authority", "")));
        assert_eq!(static_entry(2), Some((":method", "GET")));
        assert_eq!(static_entry(8), Some((":status", "200")));
        assert_eq!(static_entry(61), Some(("www-authenticate", "")));
        assert_eq!(static_entry(0), None);
        assert_eq!(static_entry(62), None);
    }

    #[test]
    fn evicts_oldest_entries_first() {
        let mut table = DynamicTable::new(100);
        table.insert(b"a".to_vec(), b"1".to_vec()); // 34
        table.insert(b"b".to_vec(), b"2".to_vec()); // 34
        assert_eq!(table.size(), 68);
        table.insert(b"c".to_vec(), b"3".to_vec()); // evicts a
        assert_eq!(table.len(), 2);
        assert_eq!(table.get(62), Some((&b"c"[..], &b"3"[..])));
        assert_eq!(table.get(63), Some((&b"b"[..], &b"2"[..])));
        assert_eq!(table.get(64), None);
        assert_eq!(table.find(b"b", b"2"), Some((63, true)));
        assert_eq!(table.find(b"b", b"9"), Some((63, false)));
        assert_eq!(table.find(b":method", b"GET"), Some((2, true)));
        table.set_max_size(40);
        assert_eq!(table.len(), 1);
        table.insert(vec![b'x'; 50], Vec::new());
        assert!(table.is_empty());
        assert_eq!(table.size(), 0);
    }
}

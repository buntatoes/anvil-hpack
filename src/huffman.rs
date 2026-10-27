// SPDX-License-Identifier: LicenseRef-PolyForm-Noncommercial-1.0.0-OR-Commercial
// Copyright (c) 2026 Buntato
//! The HPACK Huffman code (RFC 7541 section 5.2 and Appendix B).

use std::sync::OnceLock;

use crate::HpackError;
use crate::tables::HUFFMAN_CODES;

/// Symbol 256 marks the end of a string. It must never appear inside one.
const EOS: u16 = 256;
/// Marks a child slot as a leaf. The low bits hold the symbol.
const LEAF: u32 = 1 << 31;

/// Bytes needed to Huffman-encode `input`.
pub fn huffman_encoded_len(input: &[u8]) -> usize {
    let bits: usize = input
        .iter()
        .map(|&b| HUFFMAN_CODES[b as usize].1 as usize)
        .sum();
    bits.div_ceil(8)
}

/// Huffman-encode `input` and append it to `out`. The last byte is padded
/// with the high bits of the end-of-string code, which are all ones.
pub fn huffman_encode(input: &[u8], out: &mut Vec<u8>) {
    let mut acc: u64 = 0;
    let mut bits: u32 = 0;
    for &byte in input {
        let (code, len) = HUFFMAN_CODES[byte as usize];
        acc = (acc << len) | code as u64;
        bits += len as u32;
        while bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
        acc &= (1u64 << bits) - 1;
    }
    if bits > 0 {
        let pad = 8 - bits;
        out.push(((acc << pad) | ((1u64 << pad) - 1)) as u8);
    }
}

/// Decode a Huffman string. At most `max_len` bytes come out.
///
/// The padding at the end must be fewer than 8 bits and all ones, and the
/// end-of-string code must not appear, as the RFC requires.
pub fn huffman_decode(input: &[u8], max_len: usize) -> Result<Vec<u8>, HpackError> {
    let tree = decode_tree();
    let mut out = Vec::with_capacity((input.len() * 8 / 5).min(max_len));
    let mut node = 0usize;
    let mut pending_bits = 0u32;
    let mut pending_all_ones = true;
    for &byte in input {
        for shift in (0..8).rev() {
            let bit = (byte >> shift) & 1;
            pending_bits += 1;
            pending_all_ones &= bit == 1;
            let next = tree[node][bit as usize];
            if next == 0 {
                return Err(HpackError::InvalidHuffman);
            }
            if next & LEAF != 0 {
                let symbol = (next & !LEAF) as u16;
                if symbol == EOS {
                    return Err(HpackError::InvalidHuffman);
                }
                if out.len() >= max_len {
                    return Err(HpackError::StringTooLong(out.len() + 1));
                }
                out.push(symbol as u8);
                node = 0;
                pending_bits = 0;
                pending_all_ones = true;
            } else {
                node = next as usize;
            }
        }
    }
    if pending_bits > 7 || !pending_all_ones {
        return Err(HpackError::InvalidHuffman);
    }
    Ok(out)
}

/// Binary tree for decoding, built once from the code table. Each node holds
/// two child slots: 0 for "no child", a node index, or `LEAF | symbol`.
fn decode_tree() -> &'static [[u32; 2]] {
    static TREE: OnceLock<Vec<[u32; 2]>> = OnceLock::new();
    TREE.get_or_init(|| {
        let mut tree: Vec<[u32; 2]> = vec![[0, 0]];
        for (symbol, &(code, len)) in HUFFMAN_CODES.iter().enumerate() {
            let mut node = 0usize;
            for i in (0..len).rev() {
                let bit = ((code >> i) & 1) as usize;
                if i == 0 {
                    tree[node][bit] = LEAF | symbol as u32;
                } else {
                    let next = tree[node][bit];
                    node = if next == 0 {
                        tree.push([0, 0]);
                        let index = tree.len() - 1;
                        tree[node][bit] = index as u32;
                        index
                    } else {
                        next as usize
                    };
                }
            }
        }
        tree
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn matches_the_rfc_strings() {
        // Strings from the Huffman request and response examples (C.4 and C.6).
        let cases = [
            ("www.example.com", "f1e3c2e5f23a6ba0ab90f4ff"),
            ("no-cache", "a8eb10649cbf"),
            ("custom-key", "25a849e95ba97d7f"),
            ("custom-value", "25a849e95bb8e8b4bf"),
            ("302", "6402"),
            ("private", "aec3771a4b"),
            (
                "Mon, 21 Oct 2013 20:13:21 GMT",
                "d07abe941054d444a8200595040b8166e082a62d1bff",
            ),
            (
                "https://www.example.com",
                "9d29ad171863c78f0b97c8e9ae82ae43d3",
            ),
        ];
        for (text, encoded) in cases {
            let mut out = Vec::new();
            huffman_encode(text.as_bytes(), &mut out);
            assert_eq!(out, hex(encoded), "{text}");
            assert_eq!(huffman_encoded_len(text.as_bytes()), out.len());
            assert_eq!(huffman_decode(&out, 1024).unwrap(), text.as_bytes());
        }
    }

    #[test]
    fn every_byte_round_trips() {
        let all: Vec<u8> = (0..=255u8).collect();
        let mut out = Vec::new();
        huffman_encode(&all, &mut out);
        assert_eq!(huffman_decode(&out, 1024).unwrap(), all);
        assert_eq!(huffman_decode(&[], 10).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn rejects_bad_padding_and_end_of_string() {
        // 'a' is 00011 (5 bits); padding with zeros is not allowed.
        assert_eq!(huffman_decode(&[0x18], 10), Err(HpackError::InvalidHuffman));
        // Correct padding is ones.
        assert_eq!(huffman_decode(&[0x1f], 10), Ok(b"a".to_vec()));
        // A whole byte of padding is too long.
        assert_eq!(
            huffman_decode(&[0x1f, 0xff], 10),
            Err(HpackError::InvalidHuffman)
        );
        // The end-of-string code (30 ones) inside the string.
        assert_eq!(
            huffman_decode(&[0xff, 0xff, 0xff, 0xff], 10),
            Err(HpackError::InvalidHuffman)
        );
        // Output cap.
        let mut out = Vec::new();
        huffman_encode(b"aaaaaaaa", &mut out);
        assert!(matches!(
            huffman_decode(&out, 4),
            Err(HpackError::StringTooLong(_))
        ));
    }
}

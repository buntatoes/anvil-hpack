// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Buntato
//! Prefixed integers (RFC 7541 section 5.1).

use crate::{HpackError, MAX_INTEGER};

/// Append `value` with an N-bit prefix. `flags` holds the bits above the
/// prefix in the first byte (for example `0x80` for an indexed field).
/// `prefix_bits` must be 1 through 8; other values are treated as 8.
pub fn encode_integer(value: usize, prefix_bits: u8, flags: u8, out: &mut Vec<u8>) {
    let prefix_bits = if (1..=8).contains(&prefix_bits) {
        prefix_bits
    } else {
        8
    };
    let max_prefix = ((1u16 << prefix_bits) - 1) as usize;
    let flag_mask = !(max_prefix as u8);
    if value < max_prefix {
        out.push((flags & flag_mask) | value as u8);
        return;
    }
    out.push((flags & flag_mask) | max_prefix as u8);
    let mut rest = value - max_prefix;
    while rest >= 128 {
        out.push((rest % 128) as u8 | 0x80);
        rest /= 128;
    }
    out.push(rest as u8);
}

/// Read an integer with an N-bit prefix starting at `input[*pos]`.
///
/// On success `*pos` moves past the integer. Values above [`MAX_INTEGER`]
/// and encodings longer than needed for that range are rejected.
pub fn decode_integer(input: &[u8], pos: &mut usize, prefix_bits: u8) -> Result<usize, HpackError> {
    if !(1..=8).contains(&prefix_bits) {
        return Err(HpackError::IntegerOverflow);
    }
    let first = *input.get(*pos).ok_or(HpackError::Truncated)?;
    let max_prefix = ((1u16 << prefix_bits) - 1) as usize;
    let mut value = first as usize & max_prefix;
    let mut at = *pos + 1;
    if value == max_prefix {
        let mut shift = 0u32;
        loop {
            let byte = *input.get(at).ok_or(HpackError::Truncated)?;
            at += 1;
            // Five continuation bytes already cover 35 bits, more than MAX_INTEGER needs.
            if shift > 28 {
                return Err(HpackError::IntegerOverflow);
            }
            let part = ((byte & 0x7f) as usize)
                .checked_shl(shift)
                .ok_or(HpackError::IntegerOverflow)?;
            value = value.checked_add(part).ok_or(HpackError::IntegerOverflow)?;
            if value > MAX_INTEGER {
                return Err(HpackError::IntegerOverflow);
            }
            if byte & 0x80 == 0 {
                break;
            }
            shift += 7;
        }
    }
    *pos = at;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(value: usize, prefix: u8) {
        let mut out = Vec::new();
        encode_integer(value, prefix, 0, &mut out);
        let mut pos = 0;
        assert_eq!(decode_integer(&out, &mut pos, prefix), Ok(value));
        assert_eq!(pos, out.len());
    }

    #[test]
    fn matches_the_rfc_examples() {
        // C.1.1: 10 with a 5-bit prefix.
        let mut out = Vec::new();
        encode_integer(10, 5, 0, &mut out);
        assert_eq!(out, [0x0a]);
        // C.1.2: 1337 with a 5-bit prefix.
        out.clear();
        encode_integer(1337, 5, 0, &mut out);
        assert_eq!(out, [0x1f, 0x9a, 0x0a]);
        // C.1.3: 42 at an octet boundary.
        out.clear();
        encode_integer(42, 8, 0, &mut out);
        assert_eq!(out, [0x2a]);
        let mut pos = 0;
        assert_eq!(decode_integer(&[0x1f, 0x9a, 0x0a], &mut pos, 5), Ok(1337));
        assert_eq!(pos, 3);
    }

    #[test]
    fn keeps_flag_bits_and_round_trips() {
        let mut out = Vec::new();
        encode_integer(5, 7, 0x80, &mut out);
        assert_eq!(out, [0x85]);
        for prefix in 1..=8 {
            for value in [
                0,
                1,
                30,
                31,
                126,
                127,
                128,
                255,
                256,
                16_383,
                16_384,
                1 << 20,
                MAX_INTEGER,
            ] {
                round_trip(value, prefix);
            }
        }
    }

    #[test]
    fn rejects_truncated_and_oversized_integers() {
        let mut pos = 0;
        assert_eq!(decode_integer(&[], &mut pos, 5), Err(HpackError::Truncated));
        assert_eq!(
            decode_integer(&[0x1f], &mut pos, 5),
            Err(HpackError::Truncated)
        );
        assert_eq!(
            decode_integer(&[0x1f, 0x80], &mut pos, 5),
            Err(HpackError::Truncated)
        );
        assert_eq!(
            decode_integer(&[0x1f, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f], &mut pos, 5),
            Err(HpackError::IntegerOverflow)
        );
        assert_eq!(
            decode_integer(&[0x1f, 0xff, 0xff, 0xff, 0xff, 0x7f], &mut pos, 5),
            Err(HpackError::IntegerOverflow)
        );
        let long = [0x1f, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x01];
        assert_eq!(
            decode_integer(&long, &mut pos, 5),
            Err(HpackError::IntegerOverflow)
        );
        assert_eq!(pos, 0);
    }
}

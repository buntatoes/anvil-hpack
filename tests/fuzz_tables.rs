// SPDX-License-Identifier: LicenseRef-PolyForm-Noncommercial-1.0.0-OR-Commercial
// Copyright (c) 2026 Buntato
//! Time-budgeted fuzz runs for HPACK: encoder and decoder kept in step over
//! many blocks, table size changes, mutated blocks, Huffman corner cases, and
//! prefix integers.
//!
//! Each test runs seeded cases until its budget is spent (1.5 s by default,
//! `ANVIL_ROBUSTNESS_MS` to change it). A failing case prints its seed.

use std::time::{Duration, Instant};

use anvil_hpack::{
    Decoder, Encoder, Header, HpackError, MAX_INTEGER, decode_integer, encode_integer,
    huffman_decode, huffman_encode, huffman_encoded_len,
};

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    fn chance(&mut self, one_in: usize) -> bool {
        self.below(one_in) == 0
    }

    fn byte(&mut self) -> u8 {
        self.next() as u8
    }

    fn bytes(&mut self, max: usize) -> Vec<u8> {
        let len = self.below(max + 1);
        (0..len).map(|_| self.byte()).collect()
    }
}

fn budget() -> Duration {
    let ms = std::env::var("ANVIL_ROBUSTNESS_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1500);
    Duration::from_millis(ms)
}

fn for_seeds(base: u64, mut case: impl FnMut(u64)) -> usize {
    let start = Instant::now();
    let mut n = 0u64;
    while start.elapsed() < budget() && n < 200_000 {
        case(base ^ n.wrapping_mul(0x9e37_79b9_7f4a_7c15));
        n += 1;
    }
    n as usize
}

/// A header from a small vocabulary, so the tables get hits and evictions.
fn header(rng: &mut Rng) -> Header {
    const NAMES: &[&str] = &[
        ":path",
        ":authority",
        "accept",
        "cookie",
        "x-a",
        "x-bb",
        "content-type",
        "user-agent",
    ];
    const VALUES: &[&str] = &["/", "/index.html", "*/*", "a=1", "text/html", ""];
    let name = if rng.chance(6) {
        format!("x-{}", rng.below(1000))
    } else {
        NAMES[rng.below(NAMES.len())].to_string()
    };
    let value = match rng.below(4) {
        0 => VALUES[rng.below(VALUES.len())].to_string(),
        1 => "v".repeat(rng.below(300)),
        2 => rng.below(100_000).to_string(),
        _ => (0..rng.below(40))
            .map(|_| char::from(b' ' + rng.below(95) as u8))
            .collect(),
    };
    if rng.chance(8) {
        Header::sensitive(&name, &value)
    } else {
        Header::new(&name, &value)
    }
}

#[test]
fn encoder_and_decoder_stay_in_step_over_many_blocks() {
    for_seeds(0x4850_524f_554e_0001, |seed| {
        let mut rng = Rng::new(seed);
        let mut enc = Encoder::new();
        enc.set_huffman(rng.chance(2));
        let mut dec = Decoder::new();
        for block_no in 0..1 + rng.below(12) {
            // Zero, one, or several size changes before a block, as a peer
            // may send several SETTINGS between two requests.
            for _ in 0..rng.below(3) {
                let size = match rng.below(4) {
                    0 => 0,
                    1 => 4096,
                    2 => 64,
                    _ => rng.below(4097),
                };
                enc.set_max_table_size(size);
            }
            let headers: Vec<Header> = (0..rng.below(10)).map(|_| header(&mut rng)).collect();
            let mut block = Vec::new();
            enc.encode(&headers, &mut block);
            let got = dec
                .decode(&block)
                .unwrap_or_else(|err| panic!("seed {seed:#x} block {block_no}: {err:?}"));
            assert_eq!(got.len(), headers.len(), "seed {seed:#x} block {block_no}");
            for (a, b) in got.iter().zip(&headers) {
                assert_eq!(a.name, b.name, "seed {seed:#x} block {block_no}");
                assert_eq!(a.value, b.value, "seed {seed:#x} block {block_no}");
            }
            assert_eq!(
                dec.table().size(),
                enc.table().size(),
                "seed {seed:#x} block {block_no}: table sizes differ"
            );
            assert_eq!(dec.table().len(), enc.table().len(), "seed {seed:#x}");
            assert!(
                dec.table().size() <= dec.table().max_size(),
                "seed {seed:#x}"
            );
        }
    });
}

#[test]
fn shrinking_then_growing_the_table_before_one_block_keeps_both_sides_equal() {
    // RFC 7541 section 4.2: after a shrink and a grow between two blocks, the
    // next block must signal the smallest size and then the final one.
    let mut enc = Encoder::new();
    let mut dec = Decoder::new();
    let warm: Vec<Header> = (0..20)
        .map(|i| Header::new(&format!("x-{i}"), &"w".repeat(150)))
        .collect();
    let mut block = Vec::new();
    enc.encode(&warm, &mut block);
    dec.decode(&block).unwrap();
    enc.set_max_table_size(0);
    enc.set_max_table_size(4096);
    for round in 0..6 {
        let fresh: Vec<Header> = (0..12)
            .map(|i| Header::new(&format!("y-{round}-{i}"), &"f".repeat(120)))
            .chain((0..12).map(|i| Header::new(&format!("y-{round}-{i}"), &"f".repeat(120))))
            .collect();
        let mut block = Vec::new();
        enc.encode(&fresh, &mut block);
        let got = dec.decode(&block).unwrap();
        assert_eq!(got.len(), fresh.len(), "round {round}");
        for (a, b) in got.iter().zip(&fresh) {
            assert_eq!(a.name, b.name, "round {round}");
            assert_eq!(a.value, b.value, "round {round}");
        }
        assert_eq!(dec.table().size(), enc.table().size(), "round {round}");
    }
}

#[test]
fn mutated_blocks_never_panic_and_respect_the_caps() {
    for_seeds(0x4850_4d55_5441_0002, |seed| {
        let mut rng = Rng::new(seed);
        let mut enc = Encoder::new();
        enc.set_huffman(rng.chance(2));
        let headers: Vec<Header> = (0..rng.below(8)).map(|_| header(&mut rng)).collect();
        let mut block = Vec::new();
        enc.encode(&headers, &mut block);
        for _ in 0..1 + rng.below(5) {
            if block.is_empty() || rng.chance(5) {
                block.extend(rng.bytes(8));
                continue;
            }
            let at = rng.below(block.len());
            match rng.below(4) {
                0 => block[at] ^= 1 << rng.below(8),
                1 => block[at] = [0xff, 0x7f, 0x3f, 0x20, 0x80][rng.below(5)],
                2 => block.truncate(at),
                _ => {
                    let extra = rng.bytes(6);
                    block.splice(at..at, extra);
                }
            }
        }
        let max_table = rng.below(8192);
        let max_list = 1 + rng.below(16_384);
        let mut dec = Decoder::with_limits(max_table, max_list);
        if let Ok(got) = dec.decode(&block) {
            let total: usize = got.iter().map(Header::size).sum();
            assert!(
                total <= max_list,
                "seed {seed:#x}: list {total} > {max_list}"
            );
        }
        assert!(
            dec.table().size() <= dec.table().max_size(),
            "seed {seed:#x}"
        );
        assert!(dec.table().max_size() <= max_table, "seed {seed:#x}");
    });
}

#[test]
fn size_updates_past_the_limit_or_after_a_field_are_refused() {
    for_seeds(0x4850_5349_5a45_0003, |seed| {
        let mut rng = Rng::new(seed);
        let limit = rng.below(10_000);
        let mut dec = Decoder::with_limits(limit, 1 << 20);
        let size = limit + 1 + rng.below(1 << 20);
        let mut block = Vec::new();
        encode_integer(size, 5, 0x20, &mut block);
        assert_eq!(
            dec.decode(&block),
            Err(HpackError::TableSizeTooLarge(size)),
            "seed {seed:#x}"
        );
        let mut late = vec![0x82];
        encode_integer(rng.below(limit + 1), 5, 0x20, &mut late);
        assert_eq!(
            dec.decode(&late),
            Err(HpackError::LateTableSizeUpdate),
            "seed {seed:#x}"
        );
    });
}

#[test]
fn huge_declared_string_lengths_fail_without_allocating() {
    for_seeds(0x4850_4855_4745_0004, |seed| {
        let mut rng = Rng::new(seed);
        let len = match rng.below(3) {
            0 => MAX_INTEGER,
            1 => 1 << 30,
            _ => 70_000 + rng.below(1 << 24),
        };
        let mut block = vec![0x00];
        let huffman = if rng.chance(2) { 0x80 } else { 0 };
        encode_integer(len, 7, huffman, &mut block);
        block.extend(rng.bytes(16));
        let mut dec = Decoder::new();
        assert!(dec.decode(&block).is_err(), "seed {seed:#x}: {len}");
    });
}

#[test]
fn prefix_integers_round_trip_and_overflow_is_an_error() {
    for_seeds(0x4850_494e_5447_0005, |seed| {
        let mut rng = Rng::new(seed);
        let prefix = 1 + rng.below(8) as u8;
        let value = match rng.below(4) {
            0 => rng.below(1 << prefix),
            1 => (1 << prefix) - 1,
            2 => MAX_INTEGER,
            _ => rng.next() as u32 as usize,
        };
        let flags = if prefix < 8 {
            rng.byte() & !((1u16 << prefix) - 1) as u8
        } else {
            0
        };
        let mut out = Vec::new();
        encode_integer(value, prefix, flags, &mut out);
        let mut pos = 0;
        assert_eq!(
            decode_integer(&out, &mut pos, prefix),
            Ok(value),
            "seed {seed:#x}"
        );
        assert_eq!(pos, out.len(), "seed {seed:#x}");
        // Truncated forms fail.
        if out.len() > 1 {
            let cut = 1 + rng.below(out.len() - 1);
            let mut pos = 0;
            assert!(
                decode_integer(&out[..cut], &mut pos, prefix).is_err(),
                "seed {seed:#x}"
            );
        }
        // Long continuation runs and values past MAX_INTEGER fail.
        let mut long = vec![((1u16 << prefix) - 1) as u8];
        long.extend(std::iter::repeat_n(0xff, 5 + rng.below(20)));
        long.push(0x01);
        let mut pos = 0;
        assert!(
            decode_integer(&long, &mut pos, prefix).is_err(),
            "seed {seed:#x}"
        );
    });
}

#[test]
fn huffman_round_trips_and_rejects_bad_padding_and_eos() {
    for_seeds(0x4850_4855_4646_0006, |seed| {
        let mut rng = Rng::new(seed);
        let input = rng.bytes(200);
        let mut coded = Vec::new();
        huffman_encode(&input, &mut coded);
        assert_eq!(coded.len(), huffman_encoded_len(&input), "seed {seed:#x}");
        assert_eq!(
            huffman_decode(&coded, 1 << 16),
            Ok(input.clone()),
            "seed {seed:#x}"
        );
        // The output cap is honored.
        if !input.is_empty() {
            assert!(
                huffman_decode(&coded, input.len() - 1).is_err(),
                "seed {seed:#x}"
            );
        }
        // A whole byte of padding (8 or more 1 bits) is invalid, and so is
        // the EOS code (30 ones) inside the string.
        let mut padded = coded.clone();
        padded.push(0xff);
        assert!(
            huffman_decode(&padded, 1 << 16).is_err(),
            "seed {seed:#x}: 8-bit padding"
        );
        let mut eos = coded.clone();
        eos.extend_from_slice(&[0xff, 0xff, 0xff, 0xfc]);
        assert!(
            huffman_decode(&eos, 1 << 16).is_err(),
            "seed {seed:#x}: EOS"
        );
        // Padding that is not all 1 bits is invalid. Find a string whose
        // last byte has padding and clear one of the pad bits.
        let bits: usize = coded.len() * 8;
        let used = huffman_bits(&input);
        if bits > used {
            let mut bad = coded.clone();
            let last = bad.len() - 1;
            bad[last] &= !1;
            assert!(
                huffman_decode(&bad, 1 << 16).is_err(),
                "seed {seed:#x}: zero pad bit"
            );
        }
        // Random bytes never panic.
        let noise = rng.bytes(64);
        let _ = huffman_decode(&noise, 1 << 12);
    });
}

/// Bits the Huffman code uses for `input`. Eight copies of one byte take
/// exactly as many bytes as that byte's code has bits, so the encoder's own
/// length gives each code length with no table here.
fn huffman_bits(input: &[u8]) -> usize {
    input.iter().map(|&b| huffman_encoded_len(&[b; 8])).sum()
}

// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Buntato
//! Robustness checks for the HPACK decoder, the Huffman decoder, and integers.
//!
//! Inputs come from a seeded generator, so a failure names a case number
//! that reproduces on every run. Each test stops after a short time budget.
//! The decoder may reject any of these inputs. It must not panic or hang,
//! and anything the encoder writes must decode back to the same fields.

use anvil_hpack::{
    Decoder, Encoder, Header, MAX_STRING_LEN, decode_integer, huffman_decode, huffman_encode,
};

// ---- shared harness (same text in each repo's robustness test) ----

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// Wall-clock budget for one robustness test. Cases run until it is spent.
/// Set `ANVIL_ROBUSTNESS_MS` for a longer local soak run.
fn test_budget() -> Duration {
    let millis = std::env::var("ANVIL_ROBUSTNESS_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1500);
    Duration::from_millis(millis)
}

/// One input that runs longer than this counts as a hang.
const CASE_LIMIT: Duration = Duration::from_secs(10);

/// Upper bound on cases per test, so a fast machine does not loop forever.
const MAX_CASES: usize = 200_000;

/// xorshift64 generator. A fixed seed gives the same inputs on every run.
struct Rng(u64);

#[allow(dead_code)] // Not every test file uses every helper.
impl Rng {
    fn new(seed: u64) -> Self {
        Rng((seed ^ 0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// Value in `0..n`. Returns 0 when `n` is 0.
    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }

    fn byte(&mut self) -> u8 {
        self.next_u64() as u8
    }

    fn one_in(&mut self, n: usize) -> bool {
        self.below(n) == 0
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.below(items.len())]
    }

    fn bytes(&mut self, max_len: usize) -> Vec<u8> {
        let len = self.below(max_len + 1);
        (0..len).map(|_| self.byte()).collect()
    }
}

/// Apply 1 to 8 random edits: bit flips, byte overwrites, boundary values,
/// inserts, deletes, duplicated runs, and truncation.
fn mutate(rng: &mut Rng, seed: &[u8]) -> Vec<u8> {
    let mut out = seed.to_vec();
    let edits = 1 + rng.below(8);
    for _ in 0..edits {
        let len = out.len();
        match rng.below(8) {
            0 if len > 0 => {
                let i = rng.below(len);
                out[i] ^= 1 << rng.below(8);
            }
            1 if len > 0 => {
                let i = rng.below(len);
                out[i] = rng.byte();
            }
            2 if len > 0 => {
                let i = rng.below(len);
                out[i] = rng.pick(&[0x00, 0x01, 0x7F, 0x80, 0xFE, 0xFF]);
            }
            3 if len >= 4 => {
                let i = rng.below(len - 3);
                let v = rng.pick(&[0u32, 1, 0x7FFF_FFFF, 0x8000_0000, 0xFFFF_FFFF, 0xFFFF]);
                out[i..i + 4].copy_from_slice(&v.to_be_bytes());
            }
            4 => {
                let i = rng.below(len + 1);
                let extra = rng.bytes(16);
                out.splice(i..i, extra);
            }
            5 if len > 0 => {
                let i = rng.below(len);
                let n = 1 + rng.below((len - i).min(32));
                out.drain(i..i + n);
            }
            6 if len > 0 => {
                let i = rng.below(len);
                let n = 1 + rng.below((len - i).min(64));
                let run = out[i..i + n].to_vec();
                let at = rng.below(len + 1);
                out.splice(at..at, run);
            }
            _ => {
                let keep = rng.below(len + 1);
                out.truncate(keep);
            }
        }
    }
    out
}

fn preview(input: &[u8]) -> String {
    let shown: Vec<String> = input.iter().take(96).map(|b| format!("{b:02x}")).collect();
    let more = if input.len() > 96 { " ..." } else { "" };
    format!("{} bytes: {}{}", input.len(), shown.join(" "), more)
}

/// Run `target` on `input` in its own thread. Fails the test on a panic or
/// when the case runs past [`CASE_LIMIT`]. A hung thread is left behind and
/// ends with the test process.
fn check_case(label: &str, case: usize, input: Vec<u8>, target: fn(&[u8])) {
    let (done_tx, done_rx) = mpsc::channel();
    let shared = std::sync::Arc::new(input);
    let worker_input = shared.clone();
    let handle = thread::Builder::new()
        .name(format!("{label}-{case}"))
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            target(&worker_input);
            let _ = done_tx.send(());
        })
        .expect("spawn case thread");
    match done_rx.recv_timeout(CASE_LIMIT) {
        Ok(()) => {
            let _ = handle.join();
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            let _ = handle.join();
            panic!(
                "{label}: case {case} panicked on input {}",
                preview(&shared)
            );
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            panic!(
                "{label}: case {case} did not finish within {:?} on input {}",
                CASE_LIMIT,
                preview(&shared)
            );
        }
    }
}

/// Generate cases from `seed` until the budget is spent and check each one.
fn run_cases(
    label: &str,
    seed: u64,
    mut generate: impl FnMut(&mut Rng) -> Vec<u8>,
    target: fn(&[u8]),
) {
    let mut rng = Rng::new(seed);
    let budget = test_budget();
    let started = Instant::now();
    let mut case = 0;
    while case < MAX_CASES && started.elapsed() < budget {
        let input = generate(&mut rng);
        check_case(label, case, input, target);
        case += 1;
    }
    eprintln!("{label}: {case} cases in {:?}", started.elapsed());
}

/// Blocks in the shape a server sends: status, common response fields, and
/// literal fields, encoded by our own encoder with and without Huffman.
fn sample_blocks() -> Vec<Vec<u8>> {
    let lists: [&[(&str, &str)]; 4] = [
        &[
            (":status", "200"),
            ("content-type", "text/html; charset=utf-8"),
            ("content-length", "1234"),
        ],
        &[
            (":status", "302"),
            ("location", "https://www.example.com/next"),
            ("cache-control", "private"),
        ],
        &[
            (":status", "404"),
            ("set-cookie", "id=a3fWa; Max-Age=2592000; Secure"),
            ("x-custom", "value"),
        ],
        &[
            (":method", "GET"),
            (":scheme", "https"),
            (":path", "/"),
            (":authority", "example.com"),
        ],
    ];
    let mut blocks = Vec::new();
    for huffman in [false, true] {
        let mut encoder = Encoder::new();
        encoder.set_huffman(huffman);
        for list in lists {
            let fields: Vec<Header> = list.iter().map(|(n, v)| Header::new(n, v)).collect();
            let mut out = Vec::new();
            encoder.encode(&fields, &mut out);
            blocks.push(out);
        }
    }
    blocks
}

fn decode_once(input: &[u8]) {
    let mut decoder = Decoder::with_limits(4096, 64 * 1024);
    if let Ok(headers) = decoder.decode(input) {
        let total: usize = headers.iter().map(Header::size).sum();
        assert!(total <= 64 * 1024);
        assert!(decoder.table().size() <= decoder.table().max_size());
    }
}

fn decode_split(input: &[u8]) {
    // The same bytes as two blocks on one connection.
    let mut decoder = Decoder::with_limits(256, 64 * 1024);
    let half = input.len() / 2;
    let _ = decoder.decode(&input[..half]);
    let _ = decoder.decode(&input[half..]);
    assert!(decoder.table().size() <= 256);
}

fn huffman_target(input: &[u8]) {
    if let Ok(text) = huffman_decode(input, MAX_STRING_LEN) {
        // Decoding is unique: encoding the result gives back the same bytes.
        let mut again = Vec::new();
        huffman_encode(&text, &mut again);
        assert_eq!(again, input);
    }
}

fn integer_target(input: &[u8]) {
    for prefix in 1..=8 {
        let mut pos = 0;
        if decode_integer(input, &mut pos, prefix).is_ok() {
            assert!(pos <= input.len() && pos > 0);
        }
    }
}

#[test]
fn decoder_survives_random_bytes() {
    run_cases(
        "hpack-random",
        0x4850_4143_4b01,
        |rng| rng.bytes(256),
        decode_once,
    );
}

#[test]
fn decoder_survives_mutated_blocks() {
    let blocks = sample_blocks();
    run_cases(
        "hpack-mutated",
        0x4850_4143_4b02,
        move |rng| {
            let seed = &blocks[rng.below(blocks.len())];
            mutate(rng, seed)
        },
        decode_once,
    );
    let blocks = sample_blocks();
    run_cases(
        "hpack-mutated-split",
        0x4850_4143_4b03,
        move |rng| {
            let mut joined = blocks[rng.below(blocks.len())].clone();
            joined.extend_from_slice(&blocks[rng.below(blocks.len())]);
            mutate(rng, &joined)
        },
        decode_split,
    );
}

#[test]
fn huffman_and_integer_decoders_survive_random_bytes() {
    run_cases(
        "huffman-random",
        0x4850_4143_4b04,
        |rng| rng.bytes(64),
        huffman_target,
    );
    run_cases(
        "integer-random",
        0x4850_4143_4b05,
        |rng| rng.bytes(8),
        integer_target,
    );
}

#[test]
fn random_fields_round_trip_through_one_connection() {
    let mut rng = Rng::new(0x4850_4143_4b06);
    let mut encoder = Encoder::new();
    let mut decoder = Decoder::new();
    let names = [
        "content-type",
        "x-a",
        "x-b",
        "cookie",
        "set-cookie",
        ":path",
        "accept",
    ];
    for block in 0..2000 {
        if rng.one_in(50) {
            let size = rng.pick(&[0usize, 64, 256, 1024, 4096]);
            encoder.set_max_table_size(size);
        }
        encoder.set_huffman(!rng.one_in(3));
        let count = rng.below(12);
        let fields: Vec<Header> = (0..count)
            .map(|_| {
                let name = if rng.one_in(4) {
                    let len = 1 + rng.below(12);
                    (0..len)
                        .map(|_| (b'a' + rng.below(26) as u8) as char)
                        .collect()
                } else {
                    rng.pick(&names).to_string()
                };
                let max_len = if rng.one_in(10) { 3000 } else { 40 };
                let value: Vec<u8> = rng.bytes(max_len);
                Header {
                    name: name.into_bytes(),
                    value,
                    never_indexed: rng.one_in(8),
                }
            })
            .collect();
        let mut out = Vec::new();
        encoder.encode(&fields, &mut out);
        let decoded = decoder
            .decode(&out)
            .unwrap_or_else(|err| panic!("block {block}: {err}"));
        assert_eq!(decoded, fields, "block {block}");
        assert_eq!(
            decoder.table().size(),
            encoder.table().size(),
            "block {block}"
        );
    }
}

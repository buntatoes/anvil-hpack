// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Buntato
//! The worked examples from RFC 7541 Appendix C.
//!
//! Each case lists the header fields, the encoded block from the RFC, and
//! the dynamic table size after the block. Blocks in one group share one
//! decoder and one encoder, as they would on one connection.

use anvil_hpack::{Decoder, Encoder, Header};

struct Case {
    headers: &'static [(&'static str, &'static str)],
    block: &'static str,
    table_size: usize,
}

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

fn fields(case: &Case) -> Vec<Header> {
    case.headers
        .iter()
        .map(|(name, value)| Header::new(name, value))
        .collect()
}

fn run_group(cases: &[Case], table_size: usize, huffman: bool) {
    let mut decoder = Decoder::with_limits(table_size, 1 << 20);
    let mut encoder = Encoder::with_table_size(table_size);
    encoder.set_huffman(huffman);
    for (i, case) in cases.iter().enumerate() {
        let block = hex(case.block);
        let decoded = decoder.decode(&block).expect("decode");
        assert_eq!(decoded, fields(case), "decoded fields of block {i}");
        assert_eq!(
            decoder.table().size(),
            case.table_size,
            "decoder table after block {i}"
        );
        let mut encoded = Vec::new();
        encoder.encode(&fields(case), &mut encoded);
        assert_eq!(encoded, block, "encoded block {i}");
        assert_eq!(
            encoder.table().size(),
            case.table_size,
            "encoder table after block {i}"
        );
    }
}

#[test]
fn requests_without_huffman() {
    let cases = [
        // C.3.1
        Case {
            headers: &[
                (":method", "GET"),
                (":scheme", "http"),
                (":path", "/"),
                (":authority", "www.example.com"),
            ],
            block: "828684410f7777772e6578616d706c652e636f6d",
            table_size: 57,
        },
        // C.3.2
        Case {
            headers: &[
                (":method", "GET"),
                (":scheme", "http"),
                (":path", "/"),
                (":authority", "www.example.com"),
                ("cache-control", "no-cache"),
            ],
            block: "828684be58086e6f2d6361636865",
            table_size: 110,
        },
        // C.3.3
        Case {
            headers: &[
                (":method", "GET"),
                (":scheme", "https"),
                (":path", "/index.html"),
                (":authority", "www.example.com"),
                ("custom-key", "custom-value"),
            ],
            block: "828785bf400a637573746f6d2d6b65790c637573746f6d2d76616c7565",
            table_size: 164,
        },
    ];
    run_group(&cases, 4096, false);
}

#[test]
fn requests_with_huffman() {
    let cases = [
        // C.4.1
        Case {
            headers: &[
                (":method", "GET"),
                (":scheme", "http"),
                (":path", "/"),
                (":authority", "www.example.com"),
            ],
            block: "828684418cf1e3c2e5f23a6ba0ab90f4ff",
            table_size: 57,
        },
        // C.4.2
        Case {
            headers: &[
                (":method", "GET"),
                (":scheme", "http"),
                (":path", "/"),
                (":authority", "www.example.com"),
                ("cache-control", "no-cache"),
            ],
            block: "828684be5886a8eb10649cbf",
            table_size: 110,
        },
        // C.4.3
        Case {
            headers: &[
                (":method", "GET"),
                (":scheme", "https"),
                (":path", "/index.html"),
                (":authority", "www.example.com"),
                ("custom-key", "custom-value"),
            ],
            block: "828785bf408825a849e95ba97d7f8925a849e95bb8e8b4bf",
            table_size: 164,
        },
    ];
    run_group(&cases, 4096, true);
}

#[test]
fn responses_without_huffman() {
    let cases = [
        // C.5.1
        Case {
            headers: &[
                (":status", "302"),
                ("cache-control", "private"),
                ("date", "Mon, 21 Oct 2013 20:13:21 GMT"),
                ("location", "https://www.example.com"),
            ],
            block: "4803333032580770726976617465611d4d6f6e2c203231204f637420323031332032303a31333a323120474d546e1768747470733a2f2f7777772e6578616d706c652e636f6d",
            table_size: 222,
        },
        // C.5.2
        Case {
            headers: &[
                (":status", "307"),
                ("cache-control", "private"),
                ("date", "Mon, 21 Oct 2013 20:13:21 GMT"),
                ("location", "https://www.example.com"),
            ],
            block: "4803333037c1c0bf",
            table_size: 222,
        },
        // C.5.3
        Case {
            headers: &[
                (":status", "200"),
                ("cache-control", "private"),
                ("date", "Mon, 21 Oct 2013 20:13:22 GMT"),
                ("location", "https://www.example.com"),
                ("content-encoding", "gzip"),
                (
                    "set-cookie",
                    "foo=ASDJKHQKBZXOQWEOPIUAXQWEOIU; max-age=3600; version=1",
                ),
            ],
            block: "88c1611d4d6f6e2c203231204f637420323031332032303a31333a323220474d54c05a04677a69707738666f6f3d4153444a4b48514b425a584f5157454f50495541585157454f49553b206d61782d6167653d333630303b2076657273696f6e3d31",
            table_size: 215,
        },
    ];
    run_group(&cases, 256, false);
}

#[test]
fn responses_with_huffman() {
    let cases = [
        // C.6.1
        Case {
            headers: &[
                (":status", "302"),
                ("cache-control", "private"),
                ("date", "Mon, 21 Oct 2013 20:13:21 GMT"),
                ("location", "https://www.example.com"),
            ],
            block: "488264025885aec3771a4b6196d07abe941054d444a8200595040b8166e082a62d1bff6e919d29ad171863c78f0b97c8e9ae82ae43d3",
            table_size: 222,
        },
        // C.6.2
        Case {
            headers: &[
                (":status", "307"),
                ("cache-control", "private"),
                ("date", "Mon, 21 Oct 2013 20:13:21 GMT"),
                ("location", "https://www.example.com"),
            ],
            block: "4883640effc1c0bf",
            table_size: 222,
        },
        // C.6.3
        Case {
            headers: &[
                (":status", "200"),
                ("cache-control", "private"),
                ("date", "Mon, 21 Oct 2013 20:13:22 GMT"),
                ("location", "https://www.example.com"),
                ("content-encoding", "gzip"),
                (
                    "set-cookie",
                    "foo=ASDJKHQKBZXOQWEOPIUAXQWEOIU; max-age=3600; version=1",
                ),
            ],
            block: "88c16196d07abe941054d444a8200595040b8166e084a62d1bffc05a839bd9ab77ad94e7821dd7f2e6c7b335dfdfcd5b3960d5af27087f3672c1ab270fb5291f9587316065c003ed4ee5b1063d5007",
            table_size: 215,
        },
    ];
    run_group(&cases, 256, true);
}

#[test]
fn single_field_representations() {
    // C.2.1: literal with indexing.
    let mut decoder = Decoder::new();
    let got = decoder
        .decode(&hex("400a637573746f6d2d6b65790d637573746f6d2d686561646572"))
        .unwrap();
    assert_eq!(got, vec![Header::new("custom-key", "custom-header")]);
    assert_eq!(decoder.table().size(), 55);
    // C.2.2: literal without indexing.
    let mut decoder = Decoder::new();
    let got = decoder
        .decode(&hex("040c2f73616d706c652f70617468"))
        .unwrap();
    assert_eq!(got, vec![Header::new(":path", "/sample/path")]);
    assert!(decoder.table().is_empty());
    // C.2.3: literal never indexed. The encoder writes the same bytes.
    let mut decoder = Decoder::new();
    let got = decoder
        .decode(&hex("100870617373776f726406736563726574"))
        .unwrap();
    assert_eq!(got, vec![Header::sensitive("password", "secret")]);
    assert!(decoder.table().is_empty());
    let mut encoder = Encoder::new();
    encoder.set_huffman(false);
    let mut out = Vec::new();
    encoder.encode(&[Header::sensitive("password", "secret")], &mut out);
    assert_eq!(out, hex("100870617373776f726406736563726574"));
    assert!(encoder.table().is_empty());
    // C.2.4: indexed field.
    let mut decoder = Decoder::new();
    let got = decoder.decode(&hex("82")).unwrap();
    assert_eq!(got, vec![Header::new(":method", "GET")]);
}

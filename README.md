<div align="center">
  <h1>anvil-hpack</h1>
  <p>HPACK header compression for HTTP/2.</p>
  <p>
    <img src="https://img.shields.io/badge/release-v.01-e8c37a?style=flat-square" alt="v.01" />
    <img src="https://img.shields.io/badge/status-Proof_of_Concept-d4a359?style=flat-square" alt="Proof of Concept" />
    <a href="./LICENSE"><img src="https://img.shields.io/badge/license-MPL--2.0-blue?style=flat-square" alt="MPL-2.0" /></a>
    <a href="./SECURITY.md"><img src="https://img.shields.io/badge/security-policy-2ea44f?style=flat-square" alt="Security" /></a>
    <img src="https://img.shields.io/badge/rust-2024-orange?style=flat-square" alt="Rust 2024" />
  </p>
</div>

Version 0.1.1.

The browser uses it to read and write HTTP/2 headers. I wrote it from RFC 7541.

## What it does

- `Decoder` reads header blocks and keeps the dynamic table between blocks on one connection.
- `Encoder` writes header blocks. Fields that are already in a table go out as an index, others go out as literals, and strings use the Huffman code when it isn't longer.
- `Encoder::set_max_table_size` can be called more than once between blocks. The next block then sends the smallest size first and the last size after it, as RFC 7541 section 4.2 asks, so the decoder drops the same entries the encoder did. 0.1.0 only sent the last size, so after a shrink and a grow the two tables could drift apart.
- `Header::sensitive` marks a field as never indexed, so it never lands in a table. Use it for things like `authorization`.
- The integer, Huffman, and table pieces are public too: `encode_integer`, `decode_integer`, `huffman_encode`, `huffman_decode`, `DynamicTable`, and `static_entry`.

## Limits

- The dynamic table starts at 4096 bytes (`DEFAULT_TABLE_SIZE`). The peer can't ask for more than the decoder allows.
- A decoded header list is capped at 256 KiB by default (`DEFAULT_MAX_HEADER_LIST_SIZE`), counted the HTTP/2 way: name, value, and 32 bytes per field.
- One name or value can be at most 64 KiB (`MAX_STRING_LEN`).
- Integers above `u32::MAX` are rejected.

Bad input is an error, never a panic. That covers cut-off blocks, bad Huffman padding, an end-of-string code inside a string, table indexes that don't exist, and table size updates in the wrong place.

## Build

```
cargo test
```

Rust 2024 edition, Rust 1.85.1 or newer. No crates.io dependencies. The tests include every worked example from the RFC, seeded robustness runs, and seeded runs that keep an encoder and a decoder in step over many blocks. Set `ANVIL_ROBUSTNESS_MS` for a longer run.

## The other repositories

The browser is [anvil-browser](https://github.com/buntatoes/anvil-browser) (Apache-2.0). The page engine is [anvil-engine](https://github.com/buntatoes/anvil-engine). Its source is public to read and audit, but it's proprietary, not MPL-2.0, and can't be redistributed on its own. These nine libraries sit next to both, and all of them are MPL-2.0. None of them come from crates.io.

| Repository | What it is |
| --- | --- |
| [anvil-cipher](https://github.com/buntatoes/anvil-cipher) | AES-128, AES-256, CTR, AES-256-GCM, SHA-256, HMAC-SHA256, HKDF-SHA256 |
| [anvil-compress](https://github.com/buntatoes/anvil-compress) | DEFLATE, zlib, gzip |
| [anvil-hpack](https://github.com/buntatoes/anvil-hpack) | HPACK header compression for HTTP/2 |
| [anvil-image](https://github.com/buntatoes/anvil-image) | PNG, JPEG, GIF, WebP (`VP8L`), ICO, BMP, PPM |
| [anvil-json](https://github.com/buntatoes/anvil-json) | JSON |
| [anvil-pqc](https://github.com/buntatoes/anvil-pqc) | ML-KEM-768, X25519, X25519MLKEM768, SHA-3 |
| [anvil-ttf](https://github.com/buntatoes/anvil-ttf) | TrueType and OpenType (`glyf`, `CFF `, `kern`, `GPOS`) |
| [anvil-gpu](https://github.com/buntatoes/anvil-gpu) | Compositor passes for boxes, shadows, gradients, and NV12, on the CPU or a GPU |
| [anvil-video](https://github.com/buntatoes/anvil-video) | CPU frames and PCM/ADPCM audio in a small MP4 container |

## License

MPL-2.0. Copyright (c) 2026 Buntato.

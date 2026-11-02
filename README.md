<div align="center">
  <h1>Anvil HPACK</h1>
  <p>Clean-room RFC 7541 HPACK header compression for HTTP/2 in Go.</p>
  <p>
    <img src="https://img.shields.io/badge/release-v.02-e8c37a?style=flat-square" alt="v.02" />
    <img src="https://img.shields.io/badge/status-Proof_of_Concept-d4a359?style=flat-square" alt="Proof of Concept" />
    <img src="https://img.shields.io/badge/archive-Frozen_in_Time-8b949e?style=flat-square" alt="Archived and Frozen in Time" />
    <a href="./LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-blue?style=flat-square" alt="Apache-2.0" /></a>
    <a href="./SECURITY.md"><img src="https://img.shields.io/badge/security-policy-2ea44f?style=flat-square" alt="Security" /></a>
    <img src="https://img.shields.io/badge/go-1.22%2B-00ADD8?style=flat-square" alt="Go 1.22+" />
  </p>
</div>

> [!IMPORTANT]
> **Archived & Frozen in Time**: This standalone repository is archived and frozen in time as a historical snapshot. Active development now lives in-tree inside [buntatoes/anvil-browser](https://github.com/buntatoes/anvil-browser) under `engines/go/anvil-hpack`.

Version 0.2.0 (`v.02`).

The browser uses it to read and write HTTP/2 headers. Written from scratch in Go (`github.com/buntatoes/anvil-hpack`) from RFC 7541.

## What it does

- `Decoder` reads header blocks and keeps the dynamic table between blocks on one connection.
- `Encoder` writes header blocks. Fields that are already in a table go out as an index, others go out as literals, and strings use the canonical Huffman code when it isn't longer.
- `Encoder.SetMaxTableSize` can be called more than once between blocks. The next block sends the smallest size first and the last size after it, as RFC 7541 section 4.2 requires, so the decoder evicts the same entries the encoder did.
- `NewSensitiveHeader` marks a field as never indexed, so it never lands in a table.
- `DynamicTable.EntriesSnapshot` inspects the live dynamic table state.
- The integer, Huffman, and table primitives are public: `EncodeInteger`, `DecodeInteger`, `HuffmanEncode`, `HuffmanDecode`, `DynamicTable`, and `StaticEntry`.

## Limits

- The dynamic table starts at 4096 bytes (`DefaultTableSize`). The peer cannot ask for more than the decoder allows.
- A decoded header list is capped at 256 KiB by default (`DefaultMaxHeaderListSize`), counted the HTTP/2 way: name, value, and 32 bytes per field.
- One name or value can be at most 64 KiB (`MaxStringLen`).
- Integers above `MaxInteger` (`u32::MAX`) are rejected.

## Build

```
go vet ./...
go test -v ./...
```

Go 1.22 or newer (`CGO_ENABLED=0`). Zero external Go module dependencies.

## The other repositories

The browser is [anvil-browser](https://github.com/buntatoes/anvil-browser) and the page engine is [anvil-forge](https://github.com/buntatoes/anvil-forge), built alongside the nine companion libraries below.

| Repository | What it is |
| --- | --- |
| [anvil-cipher](https://github.com/buntatoes/anvil-cipher) | AES-128, AES-256, CTR, AES-256-GCM, SHA-256, HMAC-SHA256, HKDF-SHA256 |
| [anvil-compress](https://github.com/buntatoes/anvil-compress) | DEFLATE, zlib, gzip (Go) |
| [anvil-hpack](https://github.com/buntatoes/anvil-hpack) | HPACK header compression for HTTP/2 (Go) |
| [anvil-image](https://github.com/buntatoes/anvil-image) | PNG, JPEG, GIF, WebP (`VP8L` + `VP8 `), ICO, BMP, PPM |
| [anvil-json](https://github.com/buntatoes/anvil-json) | JSON & RFC 6901 JSON Pointer (Go) |
| [anvil-pqc](https://github.com/buntatoes/anvil-pqc) | ML-KEM-768, X25519, X25519MLKEM768, SHA-3 |
| [anvil-ttf](https://github.com/buntatoes/anvil-ttf) | TrueType and OpenType (`glyf`, `CFF `, `kern`, `GPOS`) |
| [anvil-gpu](https://github.com/buntatoes/anvil-gpu) | Compositor passes for boxes, shadows, gradients, and NV12/I420, on the CPU or a GPU |
| [anvil-video](https://github.com/buntatoes/anvil-video) | CPU frames (AVID/ADCT) and PCM/ADPCM/FLAC/MDCT audio in MP4, fMP4, and WebM containers |

## License

Licensed under the **Apache License, Version 2.0** (`Apache-2.0`). Copyright (c) 2026 Buntato. Pursuant to Section 6 of the Apache License 2.0, the names **"Anvil"**, **"Anvil Browser"**, **"Anvil Forge"** (**"Forge"**), **"Cinder"**, **"Prism"**, and the Anvil brand logo/emblem are copyrighted and trademarked by **Buntato** (*All Rights Reserved*) and may not be used to brand derivative works without written permission. See [LICENSE](./LICENSE).

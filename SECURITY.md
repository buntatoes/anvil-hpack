# Security

Report problems in private:

https://github.com/buntatoes/anvil-hpack/security/advisories/new

Do not put the details in a public issue.

## What is checked

- The peer cannot grow the dynamic table past the size the decoder allows. The default is 4096 bytes (`DefaultTableSize`).
- A decoded header list larger than the cap is rejected. The default is 256 KiB (`DefaultMaxHeaderListSize`).
- A single name or value longer than 64 KiB (`MaxStringLen`) is rejected, before and after Huffman decoding.
- Integers above `MaxInteger` (`u32::MAX`), cut-off fields, bad Huffman padding, and table indexes that do not exist are errors.
- Fields marked never indexed are never added to a table by the encoder.

Version 0.2.0 (`v.02`) is current.

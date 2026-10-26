# Security

Report problems in private:

https://github.com/buntatoes/anvil-hpack/security/advisories/new

The browser repository takes the same kind of report:

https://github.com/buntatoes/anvil-browser/security/advisories/new

Do not put the details in a public issue.

## What is checked

- The peer can't grow the dynamic table past the size the decoder allows. The default is 4096 bytes.
- A decoded header list larger than the cap is rejected. The default is 256 KiB.
- A single name or value longer than 64 KiB is rejected, before and after Huffman decoding.
- Integers above `u32::MAX`, cut-off fields, bad Huffman padding, and table indexes that don't exist are errors.
- Fields marked never indexed are never added to a table by the encoder.

Version 0.1.2 is current.

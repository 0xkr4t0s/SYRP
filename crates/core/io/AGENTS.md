# syrp-io

## Purpose
The L2 stream layer (master plan §5.2, Wave 1.1 item 3). `InputStream` is a trait over
`std::io::Read` with JUCE's positioned, typed API; `StdInputStream` adapts any `Read + Seek`. `MemoryBlock` is JUCE's
`MemoryBlock`. `IoError` (codes in `errors/io.toml`) converts both ways with `std::io::Error`.
Modules: `error`, `input`, `memory_block`. Later increments add `output` (`OutputStream`,
`StdOutputStream`), `memory_stream`
(`MemoryInputStream`/`MemoryOutputStream`), `buffered`, `subregion` and `source`
(`InputSource`) here (WP-1.1-03), files (WP-1.1-04) and zip/gzip (WP-1.1-05).

## Ledger rows
juce_core/InputStream, juce_core/OutputStream, juce_core/MemoryBlock,
juce_core/MemoryInputStream, juce_core/MemoryOutputStream, juce_core/BufferedInputStream,
juce_core/SubregionStream, juce_core/InputSource

## Invariants
- Typed readers use the byte layout of `from_le_bytes`/`from_be_bytes`; floats are bit-exact.
- A fixed-size read that hits the end is `SYRP-IO-0001`, not JUCE's zero (documented deviation).
- Text reads decode UTF-16 when a BOM is present (either order), drop a UTF-8 BOM, and replace
  invalid sequences with U+FFFD. `read_next_line` splits on `\n` and strips one `\r` before it.
- `StdInputStream` tracks the position themselves; the inner value is never lent mutably.
- `MemoryBlock` never panics on out-of-range offsets (clips; `copy_to` zero-fills) and never
  exposes uninitialised bytes. Resizes by a size argument use `try_reserve_exact` (`SYRP-IO-0005`).
- Bit ranges: bit `i` is bit `i % 8` of byte `i / 8`; at most 32 bits; bits past the end read as
  0 and are dropped on write.
- `IoError` binds every field in `Display`; `IoError -> std::io::Error -> IoError` gives the
  original error back, source included.
- No `unsafe`, no platform code, no third-party runtime dependencies (Miri-clean).

## Thread affinity
Any thread, not RT-safe: streams allocate and do I/O. Never use them on the audio thread.

## Stability
experimental

## How to test
```sh
cargo syrp test --crate syrp-io
cargo +nightly miri test -p syrp-io   # when Miri is installed
```
B3 uses proptest: random operation sequences against a `Vec<u8>` model.

## Open questions
- Compressed-int layout (for the differential harness, WP-0.1-11; JUCE's binary `ValueTree`
  format depends on it). JUCE documents "number of significant bytes, then up to 4 bytes
  little-endian" but not the sign. Syrp uses one length byte whose low 7 bits are the count
  (0..=4) of significant bytes of `|value|` and whose bit 7 marks a negative value, then those
  bytes little-endian. `0` is `00`, `-1` is `81 01`, `i32::MIN` is `84 00 00 00 80`. Reading a
  count above 4 is `SYRP-IO-0004`. Confirm against JUCE output before `syrp-state` relies on it.

## Known gaps
WP-1.1-03 is in progress; B1, B3 and the `IoError` part of B7 are done. Still to come:
B2 `OutputStream`/`StdOutputStream` (with `IoError::bytes_copied`), B4 `MemoryInputStream`/`MemoryOutputStream` (uses `SYRP-IO-0002`), B5 `BufferedInputStream`
and `SubregionStream`, B6 `InputSource`, the rest of B7, and the Miri run.

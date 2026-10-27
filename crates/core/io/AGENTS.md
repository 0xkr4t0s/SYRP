# syrp-io

## Purpose
The L2 stream layer (master plan §5.2, Wave 1.1 item 3). So far it holds `MemoryBlock`
(JUCE's `MemoryBlock`) and `IoError`, whose codes are in `errors/io.toml` and which converts both
ways with `std::io::Error`. Module layout: `error`, `memory_block`; WP-1.1-03's next increments
add `input` (`InputStream`, `StdInputStream`), `output` (`OutputStream`, `StdOutputStream`),
`memory_stream`, `buffered`, `subregion` and `source` (`InputSource`). WP-1.1-04 adds files and
WP-1.1-05 zip and gzip, each in its own module.

## Ledger rows
juce_core/InputStream, juce_core/OutputStream, juce_core/MemoryBlock,
juce_core/MemoryInputStream, juce_core/MemoryOutputStream, juce_core/BufferedInputStream,
juce_core/SubregionStream, juce_core/InputSource

## Invariants
- `MemoryBlock` has one private `Vec<u8>`; it never panics on out-of-range offsets (operations
  clip; `copy_to` zero-fills the outside part) and never exposes uninitialised bytes, so JUCE's
  "zero new bytes" flags are accepted but have no effect.
- Resizes by a size argument (`with_size`, `set_size`, `ensure_size`) use `try_reserve_exact`
  and return `SYRP-IO-0005`, leaving the block unchanged; they never abort.
- Bit ranges: bit `i` is bit `i % 8` of byte `i / 8`; at most 32 bits; bits past the end read as
  0 and are dropped on write.
- `IoError` binds every field in `Display`; `IoError -> std::io::Error -> IoError` gives the
  original error back, source included.
- No `unsafe`, no platform code, no third-party runtime dependencies (Miri-clean).

## Thread affinity
Any thread. Not RT-safe: `MemoryBlock` allocates. Never resize one on the audio thread.

## Stability
experimental

## How to test
```sh
cargo syrp test --crate syrp-io
cargo +nightly miri test -p syrp-io   # when Miri is installed
```
B3 uses proptest: random operation sequences against a `Vec<u8>` model.

## Open questions
- Compressed-int layout (B2) is decided in the increment that adds `OutputStream`.

## Known gaps
WP-1.1-03 is in progress. This increment: B3 and the `IoError` part of B7. Still to come:
B1 `InputStream`/`StdInputStream`, B2 `OutputStream`/`StdOutputStream`, B4 memory streams
(`SYRP-IO-0002`), B5 buffered and subregion streams, B6 `InputSource`, and the Miri run.

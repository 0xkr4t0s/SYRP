# syrp-io

## Purpose
The L2 stream layer (master plan §5.2, Wave 1.1 item 3). `InputStream` and `OutputStream` are
traits over `std::io::Read`/`Write` with JUCE's positioned, typed API; `StdInputStream` and
`StdOutputStream` adapt any `Read + Seek`/`Write + Seek`. `MemoryBlock` is JUCE's
`MemoryBlock`. `IoError` (codes in `errors/io.toml`) converts both ways with `std::io::Error`.
`MemoryInputStream`/`MemoryOutputStream` stream over bytes in memory and `InputSource` opens
fresh streams on demand. Modules: `error`, `input`, `output`, `memory_block`, `memory_stream`,
`source`. Later increments add `buffered` and `subregion` here (WP-1.1-03), files (WP-1.1-04)
and zip/gzip (WP-1.1-05).

## Ledger rows
juce_core/InputStream, juce_core/OutputStream, juce_core/MemoryBlock,
juce_core/MemoryInputStream, juce_core/MemoryOutputStream, juce_core/BufferedInputStream,
juce_core/SubregionStream, juce_core/InputSource

## Invariants
- Typed readers and writers are byte-for-byte inverses; floats are bit-exact (NaN included).
- A fixed-size read that hits the end is `SYRP-IO-0001`, not JUCE's zero (documented deviation).
- Text reads decode UTF-16 when a BOM is present (either order), drop a UTF-8 BOM, and replace
  invalid sequences with U+FFFD. `read_next_line` splits on `\n` and strips one `\r` before it.
- The std adapters track the position themselves; the inner value is never lent mutably.
- `Box` and `&mut` forward every required method and every provided one a stream may override
  with a cheaper version, so overrides are not bypassed behind `Box<dyn ...>`.
- `MemoryInputStream::set_position` past the end clips to the end (Syrp's choice).
  `MemoryOutputStream::set_position` past `len()` zero-fills up to the new position at once
  (Syrp's choice, pinned by `b4_output_set_position_past_end_zero_fills_syrp_choice`).
- A fixed-slice `MemoryOutputStream` write or seek that does not fit changes nothing and is
  `SYRP-IO-0002`; growth failures are `SYRP-IO-0005`.
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
B2 and B3 use proptest (typed round trips; `MemoryBlock` against a `Vec<u8>` model).

## Open questions
- Compressed-int layout (for the differential harness, WP-0.1-11; JUCE's binary `ValueTree`
  format depends on it). JUCE documents "number of significant bytes, then up to 4 bytes
  little-endian" but not the sign. Syrp writes one length byte whose low 7 bits are the count
  (0..=4) of significant bytes of `|value|` and whose bit 7 marks a negative value, then those
  bytes little-endian. `0` is `00`, `-1` is `81 01`, `i32::MIN` is `84 00 00 00 80`. Reading a
  count above 4 is `SYRP-IO-0004`. Confirm against JUCE output before `syrp-state` relies on it.
- `write_text` with `write_bom` writes `FF FE` for UTF-16 only; UTF-8 never gets a BOM.

## Known gaps
WP-1.1-03 is in progress; B1, B2, B3, B4, B6 and the error codes of B7 are done. Still to
come: B5 `BufferedInputStream` and `SubregionStream`, and the Miri run.

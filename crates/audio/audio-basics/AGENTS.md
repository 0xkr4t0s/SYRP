# syrp-audio-basics

## Purpose
Layer L3 audio basics (master plan §5.2, ADR 0003). `AudioBuffer<T>` is the owned planar
buffer for `f32`/`f64` (JUCE's `AudioBuffer`); it lends `syrp-audio-api` views. `vector_ops`
covers JUCE's `FloatVectorOperations`. `control` holds control-rate DSP and `voice` the
`VoicePool`. `convert` covers JUCE's `AudioData`: packed sample formats to and from planar
blocks, and interleaving.

Module and bench owners:

| Path | Owner |
|---|---|
| `src/buffer.rs`, `src/vector_ops.rs`, `benches/buffer.rs`, `tests/buffer.rs` | WP-2.1-01 |
| `src/control/`, `benches/control.rs` (stub until it lands) | WP-2.1-03 |
| `src/voice/`, `benches/voice_pool.rs` (stub until it lands) | WP-2.1-05 |
| `src/convert/`, `tests/convert.rs`, `tests/convert_simd.rs`, `benches/convert.rs` | WP-2.1-02 |

WP-2.1-03 and WP-2.1-05 add their own `tests/control.rs` and `tests/voice_pool.rs`.

## Ledger rows
juce_audio_basics/AudioBuffer, juce_audio_basics/FloatVectorOperations (WP-2.1-01).
juce_audio_basics/AudioData (WP-2.1-02).

## Invariants
- ADR 0003 D2: one `syrp_simd::AlignedBuf` allocation per buffer; every channel starts on a
  64-byte boundary; the stride is `frames` rounded up to 64 bytes (at least 64 bytes), so
  `as_block()`/`as_block_mut()` are always SIMD-aligned. Samples past `frames()` are padding.
- No public fields (ADR 0003 D3). `BufferSample` is sealed (`f32`, `f64`).
- Sizes come from user input: `new`, `set_size` and `make_copy_of` return
  `BufferError::TooLarge` (`SYRP-AUDIO-0005`) instead of overflowing, and leave the buffer
  unchanged on error.
- `set_size(.., avoid_reallocating = true)` never allocates when the shape fits the capacity.
  Content outside the kept region is unspecified unless `clear_extra`; it is never
  uninitialised memory. A new allocation is always zeroed.
- `vector_ops` follows ADR 0003 D5: `syrp_simd::kernels` where they exist, otherwise `Sample`
  packs (`F32x8`, `F64x4`) with `#[inline(always)]` from the `dispatch!` body down.
- `convert`: one scalar codec (`src/convert/codec.rs`) is the reference for every format.
  Integers decode exactly to `f64` and round once to the block type; floats encode by
  `x * 2^(bits-1)`, round half away from zero, clip, NaN to 0. A size mismatch returns a
  `ConvertError` (`SYRP-AUDIO-0007`..`0010`) before anything is written.
- `convert` pack paths (`src/convert/fast.rs`, ADR 0003 D5): little-endian `Int16`, `Int24`,
  `Int32`, `Float32` to and from `f32` on `F32x8`. They must give the codec's bits for every
  input at every level (`tests/convert_simd.rs`); `Level::Scalar` runs the codec. Kernels are
  `#[inline(always)]` down from `dispatch!` and avoid `array::from_fn` (not always inlined).
- Cleared flag (B3): set by `new`/`clear`, reset by any mutable access. Kernels see `frames`.

## Parity notes
- Deliberate divergence: gain ramps (`apply_gain_ramp`, `add_from_with_ramp`,
  `copy_from_with_ramp`) are half-open. Sample `i` of `n` gets `start + (end - start) * i / n`,
  so `end` falls on the first sample of the next block and consecutive blocks join without a
  repeated or skipped step. JUCE's `applyGainRamp` docs describe reaching `endGain` on the last
  sample; the gains differ by at most `(end - start) / n` per sample. A ledger note is pending.

## Open questions
For the differential harness: the `AudioData` documentation describes scaling by
`2^(bits-1)` and clipping, but not these parts, which are Syrp's choice (tests named
`b2_..._syrp_choice` in `tests/convert.rs`):
- float to integer rounds to nearest with ties away from zero;
- NaN becomes 0; values outside `-1..=1` saturate (`1.0` gives the largest integer);
- `Int24In32` reads the low 24 bits and ignores the top byte; writing sign-extends into it;
- `convert_bytes` narrowing (for example `Int24` to `Int16`) rounds the same way instead of
  shifting;
- `read_into` with fewer block channels than data channels skips the extra data channels.

## Thread affinity
Any thread. RT-safe: `channels`, `frames`, `channel`, `channel_mut`, `as_block`,
`as_block_mut`, B3/B4 operations and `vector_ops` (after one `Level::detect()`), and
`set_size`/`make_copy_of` with `avoid_reallocating` when the new shape
fits, and everything in `convert` (no allocation, locks or I/O). `AudioBuffer` implements `syrp_rt::RtSafe` for those methods. Not RT-safe: `new`, `clone`, dropping a buffer, and resizes that reallocate.

## Stability
experimental

## How to test
```sh
cargo syrp test --crate syrp-audio-basics
PROPTEST_CASES=5000 cargo test -p syrp-audio-basics --test buffer
cargo syrp wp check WP-2.1-01
cargo test -p syrp-audio-basics --test convert   # includes exhaustive Int16/Int24 sweeps (~10 s debug)
cargo bench -p syrp-audio-basics --bench convert # Callgrind: Scalar vs Level::best(), 2 x 512 frames
cargo bench -p syrp-audio-basics --bench buffer   # Callgrind counts (needs valgrind)
```
The bench fails if a `vector_ops` pack case at `Avx2` costs more than 70% of `Sse2`, and
`benches/convert.rs` fails if a pack path costs more than the scalar codec.
Tests install `syrp_rt::GuardedAllocator` so `no_alloc` proves allocation-free paths.

## Known gaps
`add_from`/`copy_from` cannot take a channel of the same buffer (borrow rules).
`AlignedBuf::zeroed` aborts on allocation failure (no fallible allocation in `syrp-simd` yet).

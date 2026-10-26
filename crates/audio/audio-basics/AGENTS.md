# syrp-audio-basics

## Purpose
Layer L3 audio basics (master plan §5.2, ADR 0003). `AudioBuffer<T>` is the owned planar
buffer for `f32`/`f64` (JUCE's `AudioBuffer`); it lends `syrp-audio-api` views. `vector_ops`
covers JUCE's `FloatVectorOperations`. `control` holds control-rate DSP and `voice` the
`VoicePool`.

Module and bench owners:

| Path | Owner |
|---|---|
| `src/buffer.rs`, `src/vector_ops.rs`, `benches/buffer.rs`, `tests/buffer.rs` | WP-2.1-01 |
| `src/control/`, `benches/control.rs` (stub until it lands) | WP-2.1-03 |
| `src/voice/`, `benches/voice_pool.rs` (stub until it lands) | WP-2.1-05 |

WP-2.1-03 and WP-2.1-05 add their own `tests/control.rs` and `tests/voice_pool.rs`.

## Ledger rows
juce_audio_basics/AudioBuffer, juce_audio_basics/FloatVectorOperations (WP-2.1-01).

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
- Cleared flag (B3): set by `new`/`clear`, reset by any mutable access. Kernels see `frames`.

## Thread affinity
Any thread. RT-safe: `channels`, `frames`, `channel`, `channel_mut`, `as_block`,
`as_block_mut`, B3/B4 operations and `vector_ops` (after one `Level::detect()`), and
`set_size`/`make_copy_of` with `avoid_reallocating` when the new shape
fits. Not RT-safe: `new`, `clone`, dropping a buffer, and resizes that reallocate.

## Stability
experimental

## How to test
```sh
cargo syrp test --crate syrp-audio-basics
PROPTEST_CASES=5000 cargo test -p syrp-audio-basics --test buffer
cargo syrp wp check WP-2.1-01
```
Tests install `syrp_rt::GuardedAllocator` so `no_alloc` proves allocation-free paths.

## Known gaps
WP-2.1-01 is delivered in increments. Landed: B1-B5. Still to come: B6 (layout proptest
over set_size/write/clear sequences, `benches/buffer.rs` Callgrind bench, `RtSafe`).
`add_from`/`copy_from` cannot take a channel of the same buffer (borrow rules).
`AlignedBuf::zeroed` aborts on allocation failure (no fallible allocation in `syrp-simd` yet).

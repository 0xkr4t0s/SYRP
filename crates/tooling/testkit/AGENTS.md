# syrp-testkit

## Purpose
Deterministic offline test helpers (master plan §9, L6). v0 is audio only: `render_offline`
runs a block closure over planar input and returns `Rendered`; `write_wav` writes float32 or
PCM16/24 WAV for review; `NoiseRng`/`white_noise` give seeded noise; `render_offline_no_alloc`
and `assert_no_alloc` run RT-safe code under `syrp_rt::no_alloc`. Covers the role of JUCE's
UnitTest helpers for audio; no JUCE class is ported.

## Ledger rows
None (tooling).

## Invariants
- Renders are deterministic: blocks of exactly `block_size` frames (last may be shorter), in
  order, no overlap; all buffers allocated before the first block with `try_reserve_exact`.
- The closure gets an offline `ProcessConfig` (`max_block_size == block_size`) and processes the
  `AudioBlockMut` in place; output shape always equals input shape.
- WAV output is little-endian, interleaved; PCM clamps to [-1, 1], scales by 2^(n-1)-1 and
  rounds; NaN becomes 0. Float files carry a `fact` chunk; odd data gets a pad byte.
- `NoiseRng` is SplitMix64; the `b5_` test pins its outputs. Never change the sequence.
- No public fields (ADR 0003); owned `Rendered` lends `as_view()`.

## Thread affinity
Test-only, any thread. The render loop itself does not allocate; `render_offline_no_alloc`
and `assert_no_alloc` wrap work in `syrp_rt::no_alloc`, which needs
`syrp_rt::GuardedAllocator` as the test binary's `#[global_allocator]` (debug builds).

## Stability
experimental

## How to test
```sh
cargo syrp test --crate syrp-testkit
```
The `b6_` abort test re-runs the test binary as a subprocess (debug builds only).

## Known gaps
Numeric comparators, tolerance profiles and their JSON shape (spec B3, B4) are the next
increment of WP-0.1-09. No WAV reader, golden-file store, `f64` renders, ManualLoop harness, fake host, image
comparators (§9.4) or UI driver; those arrive with the WPs that need them.

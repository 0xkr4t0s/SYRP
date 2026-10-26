# syrp-render-spike

## Purpose
Throwaway spike for WP-0.2-09 (master plan §5.7 and §11, risk R2): which CPU rasteriser,
`vello_cpu` or `tiny-skia`, becomes the reference renderer behind `syrp-render-cpu`. It renders
20 `DisplayList` scenes at four scales through both, times them, checks determinism, and ships a
golden comparator. The deliverable is `docs/design/0004-renderer-spike.md`. Do not depend on
this crate; `syrp-render-cpu` replaces it.

## Ledger rows
None (spike).

## Invariants
- Both backends get identical geometry: `replay` lowers glyphs (a synthetic outline font,
  `FontHandle` 1) and images (`ImageHandle` 1 and 2, built in) before any backend code runs.
- Output is the `RenderTarget` contract: premultiplied RGBA8, top row first.
- `VelloMode::Deterministic` pins `vello_cpu` to the architecture's baseline SIMD level and one
  thread. `NativeSimd` output differs by up to one unit on some pixels.
- Clip masks (`PushClipMask`) are out of scope and return `RenderError::Backend`.

## Thread affinity
Any thread, one renderer per thread. Not RT-safe (allocates).

## Stability
experimental (throwaway spike)

## How to test
```sh
cargo syrp test --crate syrp-render-spike
# ADR 0004 tables (timing needs release):
cargo test -p syrp-render-spike --release --test b2_b3_measure -- --ignored --nocapture
```

## Known gaps
Increment 2 of 3: timing, determinism runs and ADR 0004 arrive in increment 3.
No clip masks, no real fonts or text shaping (parley/skrifa belong to `syrp-text`), no
aarch64 determinism run, no GPU comparison. All for `syrp-render-cpu` and its WPs.

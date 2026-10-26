# syrp-simd

## Purpose
The L0 SIMD layer from ADR 0003 D5. DSP and per-voice code is written once against the
`Sample` trait and runs as `f32`, `f64` or a lane pack (`F32x4`, `F32x8`, `F64x2`, `F64x4`), so
`Voice<F32x8>` is eight voices in one pass. `Level::detect` and `dispatch!` (built on
`fearless_simd`) compile a body for the widest instruction set the CPU has. `AlignedBuf` is
64-byte aligned storage. Covers the role of JUCE's SIMDRegister and aligned HeapBlock use, not
their API (ScopedNoDenormals follows as `DenormalGuard`).

## Ledger rows
None (L0 infrastructure; no ledger rows in WP-0.2-14).

## Invariants
- No public fields (ADR 0003 D3): packs and masks go through `from_array`/`to_array`/`lane`.
- `Sample` and `Mask` are sealed. `Sample::mul_add` is fused (one rounding) for every type on
  every dispatch level, with or without hardware FMA.
- Pack and mask methods are `#[inline(always)]` so code inlined into `dispatch!` gets the
  level's target features. Lane storage is private and may change (e.g. to native vectors).
- `Level::Scalar` is the reference path; `SYRP_SIMD=scalar` forces it, read once per process.
- `AlignedBuf` never reallocates; non-empty buffers start on a 64-byte boundary. Every
  `unsafe` block has a `// SAFETY:` comment.

## Thread affinity
Any thread. RT-safe (no allocation, no locks): every `Sample` operation, `dispatch!` and
`Level::detect` after its first call. Not RT-safe: `AlignedBuf::zeroed`
and dropping an `AlignedBuf` (they allocate and free), the first `Level::detect` call.

## Stability
experimental

## How to test
```sh
cargo syrp test --crate syrp-simd
SYRP_SIMD=scalar cargo test -p syrp-simd          # forced scalar path
cargo +nightly miri test -p syrp-simd             # AlignedBuf and the rest under Miri
```

## Known gaps
Next increment of WP-0.2-14: `DenormalGuard` (B4), slice kernels (`add`, `mul`, `mul_add`, `copy_with_gain`,
`find_min_max`, `abs_max`) with proptest against the scalar reference (B5), Callgrind
instruction-count benches (B6), and the kernel and guard half of B7. `F32x16`/`F64x8` packs (AVX-512)
are deferred to a later WP; `Level::Avx512` already exists. No Miri job in CI yet.

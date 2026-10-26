# syrp-simd

## Purpose
The L0 SIMD layer from ADR 0003 D5. DSP and per-voice code is written once against the
`Sample` trait and runs as `f32`, `f64` or a lane pack (`F32x4`, `F32x8`, `F64x2`, `F64x4`), so
`Voice<F32x8>` is eight voices in one pass. `Level::detect` and `dispatch!` (built on
`fearless_simd`) compile a body for the widest instruction set the CPU has. `kernels` holds
slice operations (`add`, `mul`, `mul_add`, `copy_with_gain`, `find_min_max`, `abs_max`) with
scalar references. `AlignedBuf` is 64-byte aligned storage; `DenormalGuard` flushes denormals
for a scope. Covers the role of JUCE's SIMDRegister, FloatVectorOperations, ScopedNoDenormals
and aligned HeapBlock use, not their API.

## Ledger rows
None (L0 infrastructure; no ledger rows in WP-0.2-14).

## Invariants
- No public fields (ADR 0003 D3): packs and masks go through `from_array`/`to_array`/`lane`.
- `Sample` and `Mask` are sealed. `Sample::mul_add` is fused (one rounding) for every type on
  every dispatch level, with or without hardware FMA.
- Pack and mask methods are `#[inline(always)]` so code inlined into `dispatch!` gets the
  level's target features. Lane storage is private and may change (e.g. to native vectors).
- Inlining rule: every function on the path from a `dispatch!` body to the `Sample`
  operations must be `#[inline(always)]`. A non-inlined function compiles once at the baseline
  (no ymm/zmm, `mul_add` as a software `fma` call per lane) even when `Avx512` is selected.
- Kernels are generic `#[inline(always)]` bodies inside `dispatch!`, exact on every level
  (same bits as the scalar reference for NaN-free input); slices of different lengths panic.
- `DenormalGuard` restores the exact previous MXCSR/FPCR value; it is `!Send` and `RtSafe`.
  `DenormalGuard::new` is `unsafe`: Rust assumes the default FP environment, so callers must
  not let float results reach memory-safety decisions while it is alive. Keep it to one
  audited call site (the host or processor wrapper). DAZ is set only if `MXCSR_MASK` has it.
- `Level::Scalar` is the reference path; `SYRP_SIMD=scalar` forces it, read once per process.
- `AlignedBuf` never reallocates; non-empty buffers start on a 64-byte boundary. Every
  `unsafe` block has a `// SAFETY:` comment.

## Thread affinity
Any thread. RT-safe (no allocation, no locks): every `Sample` operation, `dispatch!`, every
kernel, `DenormalGuard` (implements `syrp_rt::RtSafe`) and `Level::detect` after its first call. Not RT-safe: `AlignedBuf::zeroed`
and dropping an `AlignedBuf` (they allocate and free), the first `Level::detect` call.

## Stability
experimental

## How to test
```sh
cargo syrp test --crate syrp-simd
SYRP_SIMD=scalar cargo test -p syrp-simd          # forced scalar path
cargo +nightly miri test -p syrp-simd             # AlignedBuf and the rest under Miri
cargo bench -p syrp-simd                          # Callgrind instruction counts (needs valgrind)
PROPTEST_CASES=20000 cargo test -p syrp-simd --test kernels
```
The bench fails if any case at `Avx2` costs more than 70% of its `Sse2` instruction count;
`voice_f32x8` there guards the inlining rule.

## Known gaps
`F32x16`/`F64x8` packs (AVX-512) are deferred to a later WP; `Level::Avx512` already exists.
No Miri job and no bench gate in CI yet. The public-API snapshot
(`contracts/syrp-simd.public-api.txt`) lands in the final increment of WP-0.2-14. `DenormalGuard` is a no-op off x86_64/aarch64 and
under Miri.

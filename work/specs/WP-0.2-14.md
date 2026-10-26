# WP-0.2-14: syrp-simd: Sample trait, lane packs, runtime dispatch, aligned allocation, denormal guard

## Purpose
The L0 SIMD layer from ADR 0003 D5 and master plan §4 (L0 Foundation). DSP and per-voice code is written once against `Sample` and runs as `f32`, `f64` or a lane pack, so `MyVoice<F32x8>` runs eight instances in one pass. It wraps `fearless_simd` (Apache-2.0 OR MIT) and gives every kernel a scalar reference path.

## Behaviour
- B1. `pub trait Sample`: `Copy + Send + Sync + 'static`, arithmetic operators, `splat`, `zero`, `one`, `mul_add`, `min`, `max`, `abs`, `sqrt`, comparisons returning a mask type, `select`, and `const LANES: usize`. Implemented for `f32`, `f64`, `F32x4`, `F32x8`, `F64x2`, `F64x4`. The trait is sealed for now (ADR 0003 D3: no public fields; lane packs expose `from_array`/`to_array`/`lane(i)`).
- B2. Runtime dispatch: `Level::detect()` and a `dispatch!`-style entry point (or fearless_simd's own) that picks the widest level the CPU supports once, then calls a monomorphised kernel. `SYRP_SIMD=scalar` forces the scalar path for tests and debugging.
- B3. `AlignedBuf<T>`: a heap buffer aligned to 64 bytes, `len`/`as_slice`/`as_mut_slice`, zero-initialised, no reallocation after construction. Its `unsafe` blocks each carry a `// SAFETY:` comment; Miri runs its tests.
- B4. `DenormalGuard`: an RAII guard that sets flush-to-zero and denormals-are-zero on x86_64 and the FZ bit on aarch64 for its scope, and restores the previous state on drop. A no-op elsewhere, documented as such.
- B5. Kernels with scalar references, enough to prove the design: `add`, `mul`, `mul_add` (in place and out of place), `copy_with_gain`, `find_min_max`, `abs_max`. Property tests (proptest) check every SIMD level against the scalar reference within 1 ulp for exact operations and documented tolerance otherwise, on unaligned lengths and offsets.
- B6. Instruction-count benchmarks for the kernels (ADR 0003 D7), runnable with `cargo bench -p syrp-simd`; CI does not gate on them yet.
- B7. No allocation and no locks in any kernel or `DenormalGuard` (they are `RtSafe` once syrp-rt merges; until then, document it and test with an allocation-counting allocator in tests).

## Crate setup
Create `crates/foundation/simd/` with `Cargo.toml` (inherit `version`, `edition`, `rust-version`, `repository`, `authors`, `publish` from the workspace; `[lints] workspace = true`), `[package.metadata.syrp] layer = "L0"`, an `AGENTS.md` following `docs/agents/crate-agents-template.md`, and `src/lib.rs`. Workspace members are globbed, so no root edit is needed.
Licence: `license = "MIT OR Apache-2.0"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-0.2-14` first and `cargo syrp wp check WP-0.2-14` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

# syrp-dsp-core

## Purpose
Layer L3, the base of JUCE's `juce_dsp` (master plan §4.9, §5.2; WP-2.1-10). It holds the
processor model every `syrp-dsp-*` crate implements: `DspSpec` (JUCE's `ProcessSpec`),
`ProcessContext` (replacing and non-replacing) and the `DspProcessor<S>` trait. Later
increments of the same WP add `ProcessorChain`, `ProcessorDuplicator` and `ProcessorWrapper`,
then the FFT, windowing, lookup tables and maths helpers. `AudioBlock` is
`syrp-audio-api`'s view; `SIMDRegister` is answered by `syrp-simd` packs.

## Ledger rows
juce_dsp/dsp::ProcessContextReplacing, juce_dsp/dsp::ProcessContextNonReplacing,
juce_dsp/dsp::ProcessorBase.
Later increments: dsp::ProcessorChain, dsp::ProcessorWrapper, dsp::ProcessorDuplicator,
dsp::ProcessorState, dsp::SIMDRegister, dsp::FFT, dsp::WindowingFunction, dsp::LookupTable,
dsp::LookupTableTransform, dsp::FastMathApproximations, dsp::LogRampedValue, dsp::Phase,
dsp::Matrix, dsp::Polynomial, dsp::SpecialFunctions.

## Invariants
- ADR 0003: no public fields; per-instance DSP is generic over `S: DspSample` (a sealed
  refinement of `syrp_simd::Sample` whose element is `f32`/`f64`), no derive macros.
- `DspProcessor::process` and `reset` never allocate, lock or do I/O; `prepare` may allocate
  and reports failure as `DspError` (`errors/dsp.toml`), never aborts. Allocations sized from
  a spec use `try_reserve_exact`.
- `ProcessContext::copy_input_to_output` copies only the region both blocks have.

## Thread affinity
Any thread. RT-safe: every `ProcessContext` and `DspSpec` method; implementors must keep
`DspProcessor::process` and `reset` RT-safe. Not RT-safe: `DspProcessor::prepare`.

## Stability
experimental

## How to test
```sh
cargo syrp test --crate syrp-dsp-core
cargo syrp wp check WP-2.1-10
```
Tests install `syrp_rt::GuardedAllocator` and wrap `process` in `syrp_rt::no_alloc`.

## Known gaps
- Later WP-2.1-10 increments: composition (B2), FFT and windowing (B3–B4), then lookup
  tables, maths helpers, the `SIMDRegister` → `syrp_simd` map and the Callgrind FFT bench
  (B5–B7).

# syrp-dsp-core

## Purpose
Layer L3, the base of JUCE's `juce_dsp` (master plan §4.9, §5.2; WP-2.1-10). It holds the
processor model every `syrp-dsp-*` crate implements: `DspSpec` (JUCE's `ProcessSpec`),
`ProcessContext` (replacing and non-replacing), the `DspProcessor<S>` trait, `ProcessorChain`,
`ProcessorDuplicator` with its shared state and `ProcessorWrapper`. Later increments of the
same WP add the FFT, windowing, lookup tables and maths helpers. `AudioBlock` is
`syrp-audio-api`'s view; `SIMDRegister` is answered by `syrp-simd` packs.

## Ledger rows
juce_dsp/dsp::ProcessContextReplacing, juce_dsp/dsp::ProcessContextNonReplacing,
juce_dsp/dsp::ProcessorBase, juce_dsp/dsp::ProcessorChain, juce_dsp/dsp::ProcessorWrapper,
juce_dsp/dsp::ProcessorDuplicator, juce_dsp/dsp::ProcessorState.
Later increments: dsp::SIMDRegister, dsp::FFT, dsp::WindowingFunction, dsp::LookupTable,
dsp::LookupTableTransform, dsp::FastMathApproximations, dsp::LogRampedValue, dsp::Phase,
dsp::Matrix, dsp::Polynomial, dsp::SpecialFunctions.

## Invariants
- ADR 0003: no public fields; per-instance DSP is generic over `S: DspSample` (a sealed
  refinement of `syrp_simd::Sample` whose element is `f32`/`f64`), no derive macros.
- `DspProcessor::process` and `reset` never allocate, lock or do I/O; `prepare` may allocate
  and reports failure as `DspError` (`errors/dsp.toml`), never aborts. Allocations sized from
  a spec use `try_reserve_exact`.
- `ProcessorChain`: a bypassed member is skipped (not called), `prepare`/`reset` reach every
  member. With a non-replacing context the first member that runs reads the input and the
  rest run in place on the output; if none runs the input is copied (Syrp's choice).
- `ProcessorDuplicator`: lane pack `P::Lanes` → channel `c` is lane `c % LANES` of instance
  `c / LANES`; one-lane `P` → one instance per channel run in place. A non-replacing input is
  copied to the output first. Blocks longer than the prepared maximum run in pieces.
- `ProcessContext::copy_input_to_output` copies only the region both blocks have.

## Thread affinity
Any thread. RT-safe: every `ProcessContext` method, `DspProcessor::process`/`reset` of the
chain, duplicator and wrapper, and all accessors. Not RT-safe: `prepare` (allocates),
`ProcessorDuplicator::new` (boxes the factory) and dropping a duplicator.

## Stability
experimental

## How to test
```sh
cargo syrp test --crate syrp-dsp-core
cargo test -p syrp-dsp-core --test composition
cargo syrp wp check WP-2.1-10
```
Tests install `syrp_rt::GuardedAllocator` and wrap `process` in `syrp_rt::no_alloc`.

## Known gaps
- Later WP-2.1-10 increments: FFT and windowing (B3–B4), then lookup tables, maths
  helpers, the `SIMDRegister` → `syrp_simd` map and the Callgrind FFT bench (B5–B7).
- `ProcessorChain` holds at most 8 members (nest chains for more).

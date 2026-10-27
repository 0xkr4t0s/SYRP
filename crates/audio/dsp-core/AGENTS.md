# syrp-dsp-core

## Purpose
Layer L3, the base of JUCE's `juce_dsp` (master plan §4.9, §5.2; WP-2.1-10). It holds the
processor model every `syrp-dsp-*` crate implements: `DspSpec` (JUCE's `ProcessSpec`),
`ProcessContext` (replacing and non-replacing), the `DspProcessor<S>` trait, `ProcessorChain`,
`ProcessorDuplicator` with its shared state and `ProcessorWrapper`; then `Fft` (wrapping
`rustfft` 6.4 and `realfft` 3.5), `WindowingFunction` and `special::bessel_i0`; then `LookupTable` and `LookupTableTransform`. Later increments of the
same WP add `FastMathApproximations`, `Phase`, `LogRampedValue`, `Matrix`, `Polynomial`, the
elliptic special functions, the `SIMDRegister` map and the FFT bench. `AudioBlock` is
`syrp-audio-api`'s view; `SIMDRegister` is answered by `syrp-simd` packs.

## Ledger rows
juce_dsp/dsp::ProcessContextReplacing, juce_dsp/dsp::ProcessContextNonReplacing,
juce_dsp/dsp::ProcessorBase, juce_dsp/dsp::ProcessorChain, juce_dsp/dsp::ProcessorWrapper,
juce_dsp/dsp::ProcessorDuplicator, juce_dsp/dsp::ProcessorState, juce_dsp/dsp::FFT,
juce_dsp/dsp::WindowingFunction, juce_dsp/dsp::LookupTable,
juce_dsp/dsp::LookupTableTransform.
Later increments: dsp::FastMathApproximations, dsp::LogRampedValue, dsp::Phase,
dsp::SIMDRegister, dsp::Matrix, dsp::Polynomial, dsp::SpecialFunctions.

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
- `Fft`: size `2^order`, `order <= Fft::MAX_ORDER` (20) else `SYRP-DSP-0002`. Plans and
  scratch are made in `new`; `perform*` never allocate. Forward unscaled, inverse `1/N`.
  Real-only buffers are `2 * size` floats, output interleaved `re, im` for all `size` bins.
- Tables need at least 2 points and a finite `min < max` range (`SYRP-DSP-0003`); reads
  are lane-generic (`S: DspSample<Element = T>`) and never allocate.
- `WindowingFunction`: symmetric tables (`x = i / (N - 1)`), `normalise` scales the sum to 1.

## Open questions
Choices made where the JUCE documentation is silent, each pinned by a test:
- FFT scaling: forward unscaled, inverse (complex and real) scaled by `1/N`
  (`b3_scaling_forward_unscaled_inverse_one_over_n_syrp_choice`).
- `perform_real_only_forward_transform` fills the negative bins even with the hint;
  `perform_frequency_only_forward_transform` zeroes everything it does not write; the
  real inverse ignores the DC and Nyquist imaginary parts.
- Window variants: Bartlett triangle (zero ends), symmetric `N - 1` denominators, flat-top
  coefficients as listed in `WindowingMethod`'s docs. `normalise` read as "sum is one" from
  "a DC amplitude response of one".
- `LookupTableTransform::calculate_max_relative_error` uses the absolute error where the
  function is zero.

## Thread affinity
Any thread. RT-safe: every `ProcessContext` method, `DspProcessor::process`/`reset` of the
chain, duplicator and wrapper, `Fft::perform*`, `WindowingFunction::multiply_with_windowing_table`
and `fill_windowing_tables_into`, and table reads, and all
accessors. Not RT-safe: `prepare` (allocates),
`ProcessorDuplicator::new`, `Fft::new`, `WindowingFunction::new`/`fill_windowing_tables`,
table `new`/`initialise`/`calculate_max_relative_error` (may allocate) and dropping any of them.

## Stability
experimental

## How to test
```sh
cargo syrp test --crate syrp-dsp-core
cargo test -p syrp-dsp-core --test fft      # fft profile: relative RMS <= 1e-6 (f32)
cargo syrp wp check WP-2.1-10
```
Tests install `syrp_rt::GuardedAllocator` and wrap `process` in `syrp_rt::no_alloc`.

## Known gaps
- Later WP-2.1-10 increments: `FastMathApproximations`, `Phase`, `LogRampedValue`,
  `Matrix`, `Polynomial`, the elliptic special functions, the `SIMDRegister` → `syrp_simd` map and the Callgrind FFT bench (B5–B7).
- `Fft` is `f32` only, like JUCE's.
- `ProcessorChain` holds at most 8 members (nest chains for more).

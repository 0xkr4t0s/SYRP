# WP-2.1-16: syrp-audio-basics: interpolators, IIRCoefficients and IIRFilter, and Reverb

## Purpose
The rest of `juce_audio_basics`' synthesis and DSP helpers (master plan §3, `juce_audio_basics` row): the interpolators of Wave 2.1 item 3, which WP-2.1-03 did not cover, and `IIRFilter` and `Reverb`, which the AudioSource family (WP-2.1-04) wraps. Numbered after the Wave 2.1 list because those two classes have no list item of their own. These are `juce_audio_basics`' simple versions; the full `juce_dsp` filters are WP-2.1-11. Delivery: 2 PRs, each at most 1,200 changed lines (B1–B2, then B3–B5).

## Behaviour
- B1. Interpolators: `Interpolator<K>` (JUCE's `GenericInterpolator`) over kernels `ZeroOrderHold`, `Linear`, `CatmullRom`, `Lagrange` and `WindowedSinc`, the set the `Interpolators` documentation lists. `process(speed_ratio, input, output) -> usize` returns the number of input samples used, `process_adding(.., gain)` adds instead of replacing, `_wrapping` variants read a circular input as the documentation describes, `reset()` clears history, and `base_latency()` reports each kernel's latency. Interpolators keep history between calls, so processing a signal in random chunk sizes gives the same output as one call (proptest, `f32-arith` profile). Kernels are lane-generic over `syrp_simd::Sample`, so one interpolator can process a lane pack of channels.
- B2. Interpolator quality: tests check each kernel reproduces DC and a linear ramp exactly where its order allows, and that `speed_ratio == 1` with zero history is an identity (after the kernel's latency).
- B3. `IirCoefficients`: `make_low_pass`, `make_high_pass`, `make_band_pass`, `make_notch_filter`, `make_all_pass`, `make_low_shelf`, `make_high_shelf` and `make_peak_filter` taking sample rate, frequency, Q and (for shelves and peaks) gain, with the defaults the `IIRCoefficients` documentation gives. The designs follow Robert Bristow-Johnson's public Audio EQ Cookbook; compare the magnitude response with the analytic cookbook response in tests (`filter` profile) and list the design source in the module rustdoc for the harness. `IirFilter<S: Sample>`: `set_coefficients`, `make_inactive`, `reset`, `process_single_sample_raw` and `process_samples(&mut [S])`. JUCE's `IIRFilter` locks so coefficients can change from another thread; here coefficient changes take `&mut self`, and the rustdoc shows how to publish new coefficients through `syrp_rt::RtHandoff`. Record that as a deliberate divergence in the crate `AGENTS.md`, with `SingleThreadedIIRFilter` mapped to the same type.
- B4. `Reverb`: `Parameters { room_size, damping, wet_level, dry_level, width, freeze_mode }` with the documented ranges and defaults, `set_parameters`, `set_sample_rate`, `reset`, `process_stereo(&mut [f32], &mut [f32])` and `process_mono`. The `Reverb` documentation says it is based on the technique and tunings of Freeverb, a public-domain design (Jezar at Dreampoint): build it from that public description. Parameter changes are smoothed so they do not click. Tolerance is `algorithmic` (master plan §9.3): RT60 and spectral envelope checks rather than sample parity.
- B5. Every B1, B3 and B4 processing call is allocation-free (`syrp_rt::no_alloc` tests) and implements `syrp_rt::RtSafe`. New error codes, if any, continue `errors/audio.toml`. Update the crate `AGENTS.md`.

## Not in this WP
`juce_dsp`'s IIR, FIR and reverb (WP-2.1-11, WP-2.1-12), the AudioSource wrappers (WP-2.1-04), and the `rubato` high-quality resampler.

## Crate setup
The whole crate `crates/audio/audio-basics/`, which WP-2.1-01 creates; this WP starts after WP-2.1-02 merges and may edit any file there, including `Cargo.toml` and `AGENTS.md` (add this WP's ledger rows, invariants and gaps). No new crate.
Licence: `license = "AGPL-3.0-only OR BUSL-1.1"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-2.1-16` first and `cargo syrp wp check WP-2.1-16` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

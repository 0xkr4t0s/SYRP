# WP-2.1-03: syrp-audio-basics: lane-generic SmoothedValue, ADSR and Decibels

## Purpose
The control-rate building blocks from master plan §5.8 and ADR 0003 D5 (Wave 2.1 item 3): parameter smoothing, the ADSR envelope and decibel conversion, written against `syrp_simd::Sample` so that `Adsr<F32x8>` runs eight envelopes at once. Voices built on `VoicePool` (WP-2.1-05) use them, but neither WP needs the other. Delivery: 1–2 PRs (B1–B3, then B4–B6).

## Owned paths
`crates/audio/audio-basics/src/control/` (the module WP-2.1-01 declared), `tests/control.rs` and `benches/control.rs` (replacing the stub) in that crate. You may not edit `src/lib.rs`, `Cargo.toml` or `AGENTS.md`; put module notes in the `control` module's rustdoc, and if you need a dependency or a re-export at the crate root, file a change request on the WP issue. `cargo syrp wp check` runs the whole workspace's tests for a WP without a `crate`; use `cargo syrp test --crate syrp-audio-basics` while iterating.

## Behaviour
- B1. `decibels`: `gain_to_decibels(gain, minus_infinity_db)` and `decibels_to_gain(db, minus_infinity_db)` for `f32` and `f64` (`Sample` has no logarithm; packs convert per lane through `to_array`), with the default floor of −100 dB that the `Decibels` documentation shows; gains at or below the floor's gain map to the floor and back to exactly zero. `to_string(db, decimal_places, minus_infinity_text)` for display.
- B2. `SmoothedValue<S: Sample, M: Mapping>` with `Linear` and `Multiplicative` mappings: `reset(sample_rate, ramp_seconds)` (or `reset_steps(n)`), `set_current_and_target`, `set_target`, `next_value`, `skip(n)` (equal to calling `next_value` n times, within tolerance), `current`, `target`, `is_smoothing` (a per-lane mask for packs). A linear ramp reaches its target exactly after the ramp length; setting a new target mid-ramp restarts from the current value. `Multiplicative` rejects zero or sign-crossing values with a debug assertion, as documented. `apply_gain(&mut [S])` ramps a block.
- B3. Control rate (ADR 0003 D5): `SmoothedValue::with_control_rate(n)` updates the ramp every `n` samples (16 or 32 typical) and interpolates linearly per sample between updates; the result stays within the `nonlinear` tolerance of the per-sample ramp, and the per-sample path stays the default.
- B4. `Adsr<S: Sample>` with `AdsrParams { attack, decay, sustain, release }` in seconds and a sustain level: `set_sample_rate`, `set_params`, `note_on(lanes: S::Mask)`, `note_off(lanes)`, `next_sample`, `apply_to(&mut [S])`, `is_active` (per-lane mask), `reset`. Segments are linear (Syrp's choice, documented, not yet compared with JUCE); a zero-length segment jumps; a release starts from the current level; a `note_on` during release restarts the attack from the current level. Stage changes are written with masks and `select`, never per-lane branches.
- B5. ADR 0003 D7: every lane-generic type is tested with `f32`, `f64`, `F32x4` and `F32x8`, with different per-lane targets, gates and timings, and each pack lane matches the scalar run within the `f32-arith` tolerance.
- B6. Everything here is allocation-free after construction (`syrp_rt::no_alloc` tests) and `RtSafe`; `benches/control.rs` counts instructions for `Adsr<f32>` versus `Adsr<F32x8>` over 512 samples and for `SmoothedValue` per-sample versus control rate.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-2.1-03` first and `cargo syrp wp check WP-2.1-03` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

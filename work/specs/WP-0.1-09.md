# WP-0.1-09: auricle-testkit v0: offline audio render and numeric comparators

## Purpose
Deterministic offline testing helpers. v0 covers audio; UI driving arrives with the UI phases.

## Behaviour
- B1. `render_offline(processor_fn, input: &[&[f32]], block_size, sample_rate) -> Rendered` runs a block-processing closure over the input in fixed blocks and returns the output channels. Deterministic for the same inputs.
- B2. WAV writer for 32-bit float and 16/24-bit PCM output (`hound` is acceptable) so renders can be inspected and attached to reviews.
- B3. Numeric comparators: max absolute error, RMS error, peak dBFS difference, and an `assert_close(actual, expected, Tolerance)` with named tolerance profiles (`exact`, `dsp-default` = 1e-6 abs, `dsp-loose` = -96 dBFS RMS).
- B4. Comparison results serialise to JSON matching a documented shape so `cargo auricle` can embed them in envelopes.
- B5. Seeded RNG helpers for reproducible noise inputs.
- B6. Run every audio test inside `auricle_rt::no_alloc` where the closure is meant to be RT-safe; expose a helper for that.

## Crate setup
Create `crates/tooling/testkit/` with `Cargo.toml` (inherit `version`, `edition`, `rust-version`, `repository`, `authors` from the workspace; `[lints] workspace = true`), `[package.metadata.auricle] layer = "L6"`, an `AGENTS.md` following `docs/agents/crate-agents-template.md`, and `src/lib.rs`. Workspace members are globbed, so no root edit is needed.
Licence: `license = "AGPL-3.0-only OR BUSL-1.1"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo auricle wp show WP-0.1-09` first and `cargo auricle wp check WP-0.1-09` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

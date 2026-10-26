# WP-1.9-02: syrp-compat-audio: SynthesiserVoice and Synthesiser over VoicePool, per-channel processor duplication

## Purpose
The optional compatibility front-end from ADR 0003 D1/D5 and master plan §5.10: JUCE-style one-object-per-voice synthesis and per-channel processor duplication, implemented over the lane-packed `VoicePool` in `syrp-audio-basics`. It owns no storage.

**Status: parked.** Blocked until the `syrp-audio-basics` WP (which defines `VoicePool`) and the `syrp-dsp` / `syrp-processors` WPs exist and merge; the spec room writes those in phase 2. `depends_on` gains them then, and wave 1.9 is enabled.

## Behaviour
- B1. `SynthesiserVoice` trait with the documented hooks (`can_play_sound`, `start_note`, `stop_note`, `pitch_wheel_moved`, `controller_moved`, `render_next_block`) and `Synthesiser` with note stealing and sound selection as documented.
- B2. A compat voice runs in one lane of a `VoicePool` scalar slot; the native lane-generic path is the escape hatch (`.handle()`).
- B3. `ProcessorDuplicator`-equivalent: runs a mono processor per channel by packing channels into lanes where the processor is `Sample`-generic, and falls back to one instance per channel otherwise.
- B4. No allocation or locks on the audio thread; tests use the syrp-rt allocation guard.
- B5. Dependency direction: `syrp-compat-audio -> audio-basics, dsp, processors` only.

## Crate setup
Create `crates/compat/audio/` with `Cargo.toml` (inherit `version`, `edition`, `rust-version`, `repository`, `authors`, `publish` from the workspace; `[lints] workspace = true`), `[package.metadata.syrp] layer = "L5"`, an `AGENTS.md` following `docs/agents/crate-agents-template.md`, and `src/lib.rs`. Workspace members are globbed, so no root edit is needed.
Licence: `license = "AGPL-3.0-only OR BUSL-1.1"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-1.9-02` first and `cargo syrp wp check WP-1.9-02` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

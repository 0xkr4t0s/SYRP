# WP-0.2-05: auricle-device-api: audio and MIDI device backend traits

## Behaviour
- B1. `trait AudioDeviceType` (a backend: CoreAudio, WASAPI, ALSA...) enumerating devices and creating `AudioDevice`s.
- B2. `trait AudioDevice`: open with channel masks, sample rate and buffer size; start with a callback; stop; report latency, xruns and available rates/sizes.
- B3. The callback receives input and output blocks (from `auricle-audio-api`) and an RT context; it must be callable without allocation.
- B4. `trait MidiDeviceType`/`MidiInput`/`MidiOutput` with device identifiers stable across runs, delivering `auricle-midi` messages with timestamps.
- B5. Device-change notifications are posted to the message loop (`auricle-loop-api`).
- B6. A `fake` module with an in-memory device for tests (no real hardware).
- B7. Contract freeze as in WP-0.2-01.

## Crate setup
Create `crates/contracts/device-api/` with `Cargo.toml` (inherit `version`, `edition`, `rust-version`, `repository`, `authors` from the workspace; `[lints] workspace = true`), `[package.metadata.auricle] layer = "L1"`, an `AGENTS.md` following `docs/agents/crate-agents-template.md`, and `src/lib.rs`. Workspace members are globbed, so no root edit is needed.
Licence: `license = "MIT OR Apache-2.0"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo auricle wp show WP-0.2-05` first and `cargo auricle wp check WP-0.2-05` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

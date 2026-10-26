# WP-0.2-02: auricle-audio-api: buffer views, ChannelSet, BusLayout, ProcessConfig, PlayHead

## Purpose
Audio data types shared by processors, devices, formats and hosts (master plan §5.8).

## Behaviour
- B1. Non-owning buffer views: `AudioBlock<'a, T>` and `AudioBlockMut<'a, T>` over planar channels, generic over `f32`/`f64`, with channel and sub-range slicing that never allocates.
- B2. `ChannelSet` covering mono, stereo, LCR, quad, 5.0/5.1, 6.x, 7.x, 7.1.4 and discrete N, with named speaker positions; `BusLayout` = ordered input and output buses with names and `ChannelSet`s; layout negotiation result types.
- B3. `ProcessConfig`: sample rate, max block size, precision, offline/realtime flag.
- B4. Transport and play-head: `PlayHeadInfo` with optional fields (bpm, time signature, ppq position, bar start, loop points, sample position, playing/recording/looping flags, host time) mirroring what JUCE documents for its position info.
- B5. Everything here is `RtSafe` where it is used on the audio thread (from `auricle-rt`).
- B6. Contract freeze as in WP-0.2-01.

## Crate setup
Create `crates/contracts/audio-api/` with `Cargo.toml` (inherit `version`, `edition`, `rust-version`, `repository`, `authors` from the workspace; `[lints] workspace = true`), `[package.metadata.auricle] layer = "L1"`, an `AGENTS.md` following `docs/agents/crate-agents-template.md`, and `src/lib.rs`. Workspace members are globbed, so no root edit is needed.
Licence: `license = "MIT OR Apache-2.0"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo auricle wp show WP-0.2-02` first and `cargo auricle wp check WP-0.2-02` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

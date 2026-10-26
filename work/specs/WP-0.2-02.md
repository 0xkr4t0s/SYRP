# WP-0.2-02: syrp-audio-api: buffer views, ChannelSet, BusLayout, ProcessConfig, PlayHead

## Purpose
Audio data types shared by processors, devices, formats and hosts (master plan §5.8).

## Data layout
Read `docs/design/0003-data-layout.md` (ADR 0003) first. Hot-path types in this crate have no public fields (D3): expose constructors, accessors, views, handles and iterators, so storage can change without a contract change.

## Behaviour
- B1. Non-owning buffer views: `AudioBlock<'a, T>` and `AudioBlockMut<'a, T>` over planar channels stored in one region with a fixed channel stride, generic over `f32`/`f64`, with channel and sub-range slicing that never allocates. Views expose `channels()`, `frames()`, `stride()`, `channel(ch)` / `channel_mut(ch)` (length `frames`), `channel_padded_mut(ch)` (length `stride`, for SIMD kernels) and `split_channels_mut()` yielding disjoint mutable channel slices.
- B2. `ChannelSet` covering mono, stereo, LCR, quad, 5.0/5.1, 6.x, 7.x, 7.1.4 and discrete N, with named speaker positions; `BusLayout` = ordered input and output buses with names and `ChannelSet`s; layout negotiation result types.
- B3. `ProcessConfig`: sample rate, max block size, precision, offline/realtime flag.
- B4. Transport and play-head: `PlayHeadInfo` with optional fields (bpm, time signature, ppq position, bar start, loop points, sample position, playing/recording/looping flags, host time) mirroring what JUCE documents for its position info.
- B5. Alignment contract: a view reports `is_simd_aligned()`, true when every channel starts on a 64-byte boundary and the stride in bytes is a multiple of 64 (16 `f32` or 8 `f64` samples). Owning buffers (in `syrp-audio-basics`) must always produce aligned views. Two unsafe constructors exist: `from_raw_parts` accepts any layout, and `from_raw_parts_aligned` debug-asserts the alignment contract and panics in debug builds when it does not hold. Sub-range views may be unaligned and report it.
- B6. Everything here is `RtSafe` where it is used on the audio thread (from `syrp-rt`).
- B7. Contract freeze as in WP-0.2-01.

## Crate setup
Create `crates/contracts/audio-api/` with `Cargo.toml` (inherit `version`, `edition`, `rust-version`, `repository`, `authors`, `publish` from the workspace; `[lints] workspace = true`), `[package.metadata.syrp] layer = "L1"`, an `AGENTS.md` following `docs/agents/crate-agents-template.md`, and `src/lib.rs`. Workspace members are globbed, so no root edit is needed.
Licence: `license = "MIT OR Apache-2.0"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-0.2-02` first and `cargo syrp wp check WP-0.2-02` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

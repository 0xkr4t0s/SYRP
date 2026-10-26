# WP-0.2-03: auricle-midi: MidiMessage, MidiBuffer, UMP packets and codecs

## Purpose
MIDI 1.0 and MIDI 2.0 (UMP) data types and codecs (JUCE: MidiMessage, MidiBuffer, ump namespace; public docs only).

## Behaviour
- B1. `MidiMessage`: construct, classify and read note on/off, controller, program change, pitch bend, channel and poly pressure, SysEx, meta events, with timestamps.
- B2. `MidiBuffer`: a sample-position-ordered event list with stable ordering for equal positions, iteration over a range, and a pre-allocated variant usable on the RT thread without allocation (`with_capacity`, fails when full).
- B3. UMP: packet types for every message type in the MIDI 2.0 UMP spec (utility, system, MIDI 1.0 channel voice, data 64, MIDI 2.0 channel voice, data 128, flex data, stream), parse and serialise to `u32` words.
- B4. Conversions between MIDI 1.0 byte streams and UMP (both directions), including SysEx splitting and running-status handling for byte streams.
- B5. Fuzz-ready parsing: every parser returns an error instead of panicking on arbitrary input (proptest with random bytes).
- B6. Contract freeze as in WP-0.2-01.

## Crate setup
Create `crates/contracts/midi/` with `Cargo.toml` (inherit `version`, `edition`, `rust-version`, `repository`, `authors` from the workspace; `[lints] workspace = true`), `[package.metadata.auricle] layer = "L1"`, an `AGENTS.md` following `docs/agents/crate-agents-template.md`, and `src/lib.rs`. Workspace members are globbed, so no root edit is needed.
Licence: `license = "MIT OR Apache-2.0"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo auricle wp show WP-0.2-03` first and `cargo auricle wp check WP-0.2-03` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

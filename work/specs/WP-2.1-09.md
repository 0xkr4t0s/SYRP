# WP-2.1-09: syrp-midi-utils: MIDI 1.0 and MIDI 2.0 protocol translation and UMP dispatchers

## Purpose
Wave 2.1 item 9: the UMP conversion and dispatch parts of JUCE's `universal_midi_packets` namespace. The `syrp-midi` contract (WP-0.2-03) already has `UmpPacket`, `UmpBuffer`, `BytesToUmp` (a MIDI 1.0 byte stream to MIDI 1.0 UMP packets) and `UmpPacket::midi1_bytes`; its `AGENTS.md` lists MIDI 1.0 ↔ MIDI 2.0 channel voice translation as not yet planned. This WP adds that translation and the dispatchers that turn byte streams and word streams into packets for a receiver. The rules come from the public MIDI 2.0 specifications (the UMP and MIDI 2.0 Protocol specification, M2-104-UM, including its translation appendix) and JUCE's documentation for behaviour; `midi2` 0.11 (MIT OR Apache-2.0) may be read as a reference only (master plan §4.2). Delivery: 2 PRs, each at most 1,200 changed lines (B1–B3, then B4–B6).

## Owned paths
`crates/audio/midi-utils/src/ump/` (the module WP-2.1-07 declared), `tests/ump.rs` and `benches/ump.rs` (replacing the stubs). You may not edit `src/lib.rs`, `Cargo.toml` or `AGENTS.md`; put module notes in the `ump` module's rustdoc, and if you need a dependency or a crate-root re-export, file a change request on the WP issue. WP-2.1-08 works in the same crate at the same time on `src/file/`. Use `cargo syrp test --crate syrp-midi-utils` while iterating.

## Behaviour
- B1. Value scaling: `scale_up(value, source_bits, dest_bits)` and `scale_down` implementing the min-centre-max scaling of the MIDI 2.0 specification (7-bit and 14-bit to 16-bit and 32-bit and back), so that minimum, centre and maximum map exactly; tests check every 7-bit and 14-bit value round-trips.
- B2. `Midi1ToMidi2Translator`: turns MIDI 1.0 channel voice packets (message type 2) into MIDI 2.0 channel voice packets (type 4), as the specification's default translation describes: note on with velocity 0 becomes note off, controllers 6/38/98/99/100/101 are gathered per group and channel into registered and assignable controller messages, bank select is held and attached to the next program change, and all other packets pass through unchanged. `reset()` clears the per-channel state. It never allocates and holds fixed-size state for 16 groups × 16 channels.
- B3. `midi2_to_midi1(packet, emit)`: the reverse translation, emitting one or more MIDI 1.0 packets (registered controllers become the CC 101/100/6/38 sequence, program change with bank becomes CC 0/32 then program change, per-note messages with no MIDI 1.0 equivalent are dropped as the specification says). Proptest: for every MIDI 1.0 channel voice message, translating up and back gives the original message, apart from the cases the rustdoc lists.
- B4. Dispatchers, each calling a `Receiver` (`fn packet(&mut self, UmpPacket, timestamp: f64)`): `BytestreamToUmpDispatcher` (bytes in, using the contract's `BytesToUmp`, with an optional protocol: MIDI 1.0 packets out, or translated through B2 to MIDI 2.0), `U32ToUmpDispatcher` (reassembles packets from a stream of 32-bit words, resynchronising after a bad message type as the specification allows), and `ToBytestreamDispatcher` (packets in, MIDI 1.0 bytes out, translating MIDI 2.0 packets through B3 and reassembling SysEx7 across packets). The constructors take a fixed SysEx capacity; a SysEx larger than that is dropped and counted, never reallocated.
- B5. RT safety: every B1–B4 call after construction is allocation-free (`syrp_rt::no_alloc` tests) and the types implement `syrp_rt::RtSafe`. `benches/ump.rs` counts instructions (Callgrind) for translating 1,024 mixed channel voice messages.
- B6. Tests use byte and word vectors built from the specification's tables, plus proptests that feeding random bytes or words never panics. If you need error codes, use the new `errors/ump.toml` domain (`SYRP-UMP-0001`...). Add scenario files under `parity/scenarios/midi/ump/` (the WP-0.1-11 format) that send the same streams through JUCE's converters; scenario files hold inputs only, and expected outputs come solely from harness runs.

## Not in this WP
MIDI-CI (`syrp-midi-ci`), UMP endpoints and sessions (`syrp-midi-io`), flex data and stream message helpers beyond pass-through, and JUCE's `universal_midi_packets::DeviceInfo` and string utilities.

## Crate setup
No new crate. This WP also owns `parity/scenarios/midi/ump/` for B6.


## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-2.1-09` first and `cargo syrp wp check WP-2.1-09` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

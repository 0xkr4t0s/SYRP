# WP-2.1-06: syrp-mpe: MPEZoneLayout, MPEValue and MPENote, MPEInstrument, MPEMessages, and channel assignment and remapping

## Purpose
Wave 2.1 item 6: `syrp-mpe` from master plan §5.2, the MIDI Polyphonic Expression model of `juce_audio_basics`. It follows the public MPE specification (MIDI Association, MPE v1.1) for zones, member channels and the MPE Configuration Message, and JUCE's documentation for the classes' behaviour. It reads RPNs with `syrp-midi-utils`' `MidiRpnDetector` (WP-2.1-07), so it starts after that crate merges. This WP creates `syrp-mpe`. Delivery: 3 PRs, each at most 1,200 changed lines (B1–B2, then B3, then B4–B6).

## Behaviour
- B1. Values: `MpeValue` (a 14-bit value with `from_7bit`, `from_14bit`, `from_unsigned_float`, `from_signed_float`, `min_value`, `centre_value`, `max_value`, `as_7bit`, `as_14bit`, `as_unsigned_float`, `as_signed_float`, as the `MPEValue` documentation describes) and `MpeNote` (note id, MIDI channel, initial note, velocity, pitchbend, pressure, timbre, release velocity, total pitchbend in semitones, key state, and `frequency_hz(a4_hz)`; no public fields).
- B2. Zones: `MpeZone` (lower or upper, member channel count, per-note and master pitch-bend ranges with the documented defaults of 48 and 2 semitones) and `MpeZoneLayout` (`set_lower_zone`, `set_upper_zone`, `clear_all_zones`, `lower_zone`, `upper_zone`, `is_active`, and `process_next_midi_event` / `process_next_midi_buffer`, which apply MPE Configuration Messages (RPN 6) and pitch-bend range RPNs (RPN 0) read through `MidiRpnDetector`). Overlapping zones shrink as the MPE specification and the documentation describe. A listener reports layout changes.
- B3. `MpeInstrument`: `set_zone_layout`, `process_next_midi_event`, note tracking per zone with `note_on`, `note_off`, `pitchbend`, `pressure`, `timbre` and `sustain_pedal`/`sostenuto_pedal`, the tracking modes for master-channel and per-note dimensions (`last_note_played`, `lowest_note`, `highest_note`, `all_notes`), `set_legacy_mode_enabled(channel_range, pitchbend_range)`, `num_playing_notes`, `note(index)`, `note_with_id`, `most_recent_note`, and a listener with `note_added`, `note_pressure_changed`, `note_pitchbend_changed`, `note_timbre_changed`, `note_key_state_changed` and `note_released`, as the `MPEInstrument` documentation describes. Notes live in a fixed-capacity table sized at construction; processing an event never allocates (`syrp_rt::no_alloc` test).
- B4. `MpeMessages`: builders returning `syrp_midi::MidiMessage`s for setting zones, pitch-bend ranges and clearing zones (`set_lower_zone`, `set_upper_zone`, `set_zone_layout`, `clear_all_zones`, ...), using `MidiRpnGenerator`. Round trip: messages built from a random layout and fed to `MpeZoneLayout` reproduce the layout (proptest).
- B5. `MpeChannelAssigner` (free member channel for a new note, reusing the channel last used for the same note, round-robin otherwise, legacy-mode range, as documented) and `MpeChannelRemapper` (moves an incoming MPE source's channels into a zone with a source id, as documented).
- B6. Tests replay MIDI streams built from the MPE specification's examples and check note state after each event. Errors use the new `errors/mpe.toml` domain (`SYRP-MPE-0001`...). Dependencies: `syrp-midi`, `syrp-midi-utils`, `syrp-error` and `syrp-rt`.

## Not in this WP
`MPESynthesiser`, `MPESynthesiserBase` and `MPESynthesiserVoice` (JUCE-shaped voices are compat, over `VoicePool`), an MPE driver for `VoicePool` (it needs per-note pressure in `VoiceEvent`, a later `syrp-audio-basics` change), and `MPEKeyboardComponent`.

## Crate setup
Create `crates/audio/mpe/` with `Cargo.toml` (inherit `version`, `edition`, `rust-version`, `repository`, `authors`, `publish` from the workspace; `[lints] workspace = true`), `[package.metadata.syrp] layer = "L3"`, an `AGENTS.md` following `docs/agents/crate-agents-template.md`, and `src/lib.rs`. Workspace members are globbed, so no root edit is needed.
Licence: `license = "AGPL-3.0-only OR BUSL-1.1"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-2.1-06` first and `cargo syrp wp check WP-2.1-06` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

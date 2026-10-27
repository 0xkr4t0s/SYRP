# WP-2.1-08: syrp-midi-utils: MidiFile, Standard MIDI File reading and writing

## Purpose
Wave 2.1 item 8: `MidiFile` from `juce_audio_basics`, reading and writing Standard MIDI Files (SMF) as a set of `MidiMessageSequence` tracks (WP-2.1-07). Master plan §4.2 decides that Syrp builds its own reader because behaviour must match JUCE's `MidiFile`, and master plan §9.3 puts MidiFile in the `exact` profile. The SMF layout comes from the public Standard MIDI Files 1.0 specification (MIDI Association). Delivery: 2 PRs, each at most 1,200 changed lines (B1–B3, then B4–B6).

## Owned paths
`crates/audio/midi-utils/src/file/` (the module WP-2.1-07 declared) and `tests/file.rs` (replacing the stub). You may not edit `src/lib.rs`, `Cargo.toml` or `AGENTS.md`; put module notes in the `file` module's rustdoc, and if you need a dependency or a crate-root re-export, file a change request on the WP issue. WP-2.1-09 works in the same crate at the same time on `src/ump/`. Use `cargo syrp test --crate syrp-midi-utils` while iterating.

## Behaviour
- B1. `MidiFile`: `new()`, `num_tracks`, `track(i) -> Option<&MidiMessageSequence>`, `add_track(&MidiMessageSequence)`, `clear`, `time_format()` and `set_ticks_per_quarter_note(n)` / `set_smpte_time_format(frames_per_second, subframe_resolution)` (the two forms the SMF header allows), and `last_timestamp()`.
- B2. Reading: `read_from(&mut dyn std::io::Read, create_matching_note_offs: bool, limits: ReadLimits) -> Result<(MidiFile, SmfType), MidiFileError>`, where `SmfType` is `Single`, `Multi` or `Pattern` (types 0, 1 and 2) (any `syrp-io` `InputStream` is a `std::io::Read`). Timestamps are in ticks, as the documentation says. It handles running status, meta and SysEx events (`F0` and `F7` escape forms), unknown chunks (skipped), and files types 0, 1 and 2. `create_matching_note_offs` adds a note-off at the end for notes left on, as documented. Every length and count is checked against the remaining input and `ReadLimits` (maximum tracks, events and bytes, with documented defaults) before allocating; corrupt input is a coded error in the new `errors/midifile.toml` domain (`SYRP-MIDIFILE-0001`...), never a panic. Where JUCE's documentation does not say how it treats a malformed file that it could partly read (a truncated last track, a wrong chunk length), choose, pin it with a `b2_..._syrp_choice` test and list it under "Open questions" in the module rustdoc for the harness.
- B3. Writing: `write_to(&mut dyn std::io::Write, SmfType) -> Result<(), MidiFileError>`, using running status as the documentation describes and adding an end-of-track event when a track lacks one. A type-0 write of more than one track is a coded error.
- B4. Times: `convert_timestamp_ticks_to_seconds()` uses the file's tempo events (from any track, as documented) and time format; `find_all_tempo_events`, `find_all_time_sig_events` and `find_all_key_sig_events` collect into a `MidiMessageSequence`. Tests: tempo changes mid-file, SMPTE time formats and a file with no tempo event (default 120 bpm).
- B5. Round trips: proptest over random sequences (channel, meta and SysEx events) for `write_to` then `read_from` giving the same tracks; a byte-flip and truncation proptest over valid files that never panics and never allocates past `ReadLimits`; and byte-exact vectors built from the SMF specification's examples.
- B6. Add scenario files under `parity/scenarios/midi/midi_file/` (the WP-0.1-11 format) in which JUCE reads a set of files (including malformed ones) and writes sequences; scenario files hold inputs only, and expected outputs come solely from harness runs.

## Not in this WP
Format-specific extensions such as RMID or XMF wrappers, and MIDI 2.0 clip files.

## Crate setup
No new crate. This WP also owns `parity/scenarios/midi/midi_file/` for B6.


## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-2.1-08` first and `cargo syrp wp check WP-2.1-08` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

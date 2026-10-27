# syrp-midi-utils

## Purpose
Layer L3 MIDI utilities (master plan §5.2) over the frozen `syrp-midi` contract, which a WP
cannot extend: message helpers (JUCE's named meta events, float velocity, note names,
controller and General MIDI names), `MidiMessageSequence`, `MidiKeyboardState`, the RPN
detector and generator, `MidiFile` and MIDI 1.0 / 2.0 UMP translation. Messages are always
`syrp_midi::MidiMessage`; this crate never redefines contract types.

Module and file owners (WP-2.1-08 and WP-2.1-09 never edit `Cargo.toml`, `lib.rs` or this file):

| Path | Owner |
|---|---|
| `src/message/`, `src/sequence.rs`, `src/keyboard.rs`, `src/rpn.rs`; `tests/message.rs`, `tests/sequence.rs`, `tests/keyboard.rs`, `tests/rpn.rs`, `tests/errors.rs` | WP-2.1-07 |
| `src/file/`, `tests/file.rs` (stub until it lands) | WP-2.1-08 |
| `src/ump/`, `tests/ump.rs`, `benches/ump.rs` (stubs until it lands) | WP-2.1-09 |


## Ledger rows
juce_audio_basics/MidiMessageSequence, juce_audio_basics/MidiKeyboardState,
juce_audio_basics/MidiRPNDetector, juce_audio_basics/MidiRPNGenerator,
juce_audio_basics/MidiRPNMessage (WP-2.1-07). WP-2.1-08 and WP-2.1-09 list their own.

## Invariants
- Builders validate like the contract's constructors: out-of-range arguments give
  `SYRP-MIDI-0002` (`MidiError::ValueOutOfRange`), never a panic or a clamped value
  (except float velocity, which is documented as clamping).
- Readers accept any `MidiData` and return `None` for data no builder here can produce
  (wrong length, a denominator power of 32 or more, a key outside -7..=7).
- Channels are 1 to 16 in every API, as in `syrp-midi`.
- `MidiMessageSequence`: events sorted by `f64::total_cmp` of their timestamps, equal times
  in insertion order (until `message_mut` changes a time; `sort` restores it). Note-off
  matches are indices; every edit remaps them, so a match always names the same event or is
  cleared when that event is deleted. Merges reserve with `try_reserve_exact` first and fail
  with `SYRP-MIDI-0006`, leaving the sequence unchanged; bad indices are `SYRP-MIDI-0005`.
- `MidiKeyboardState`: one `AtomicU16` per note (bit `c - 1` = channel `c`). UI key events
  go through a `syrp_rt::spsc` queue of `QUEUE_CAPACITY` (2048) raw 3-byte messages; each
  queue end sits in a private `Claim` (one compare-exchange, never blocks): the only
  `unsafe` in the crate (an `unsafe impl Sync` and one block). A UI event updates the state
  only after it is queued (`SYRP-MIDI-0007` when full, state unchanged). Injected events are
  not fed back into the state. Listeners are added with `&mut self`, before sharing.
- RPN: both parameter bytes before the value; a value LSB (38) before the MSB (6) makes a
  14-bit value and is used once; selecting a parameter forgets a pending LSB; 127/127 is
  null (RPN and NRPN). The generator writes 14-bit values LSB first and 7-bit values as 6
  then 38 = 0, so generated messages always parse back (proptest in `tests/rpn.rs`).
- Errors continue `errors/midi.toml`: this crate owns `SYRP-MIDI-0005` to `0007` and
  reuses `0002` and `0003` for the cases those entries describe.
- Name tables follow the MIDI 1.0 and General MIDI Level 1 lists; the wording is Syrp's.

## Thread affinity
`message`: builders allocate (they return `MidiMessage`), so message thread only; readers,
`controller_name` and the General MIDI names are RT-safe (no allocation), `note_name`
allocates. `sequence`: message thread (edits allocate); `&self` reads are any-thread.
`keyboard`: `MidiKeyboardState` is `Send + Sync`. RT-safe (when the listeners are):
`process_next_midi_buffer`, `process_next_midi_event`, `is_note_on*` (`tests/keyboard.rs`
proves it under `syrp_rt::no_alloc`). UI thread: `note_on`, `note_off`, `all_notes_off` (they
may wait for another UI thread). `new`, `add_listener`, `remove_listener`: message thread.
Listener callbacks run on the thread that changed the state.
`rpn`: all RT-safe (`MidiRpnDetector::try_parse` never allocates); the generator returns
owned `MidiMessage`s, so it allocates.

## Stability
experimental

## How to test
```sh
cargo syrp test --crate syrp-midi-utils
cargo syrp wp check WP-2.1-07
```
Test binaries may install `syrp_rt::GuardedAllocator` (dev-dependency) to prove allocation-free
paths with `syrp_rt::no_alloc`; `proptest` is a dev-dependency.

## Known gaps
WP-2.1-07 is complete (B1 to B6). `MidiKeyboardState` has no `reset()`, and listeners
cannot be added after the state is shared. Data increment/decrement (96, 97) are not
interpreted by the RPN detector. None of the Syrp's-choice behaviours above has been
compared with JUCE by a parity scenario yet.

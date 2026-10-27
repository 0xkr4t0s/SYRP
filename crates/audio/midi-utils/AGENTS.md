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
| `src/message/`, `tests/message.rs` | WP-2.1-07 |
| `src/file/`, `tests/file.rs` (stub until it lands) | WP-2.1-08 |
| `src/ump/`, `tests/ump.rs`, `benches/ump.rs` (stubs until it lands) | WP-2.1-09 |

WP-2.1-07 adds `sequence`, `keyboard` and `rpn` in later increments.

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
- Name tables follow the MIDI 1.0 and General MIDI Level 1 lists; the wording is Syrp's.

## Thread affinity
`message`: builders allocate (they return `MidiMessage`), so message thread only; readers,
`controller_name` and the General MIDI names are RT-safe (no allocation), `note_name`
allocates.

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
WP-2.1-07 increment 1 (B1 and the B5 layout). Next: `MidiMessageSequence` (B2),
`MidiKeyboardState` (B3), RPN (B4), error codes from `SYRP-MIDI-0005` (B6).

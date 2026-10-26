# syrp-midi

## Purpose
The L1 MIDI contract (master plan §5, ADR 0003 D2/D3): `MidiMessage` (owned MIDI 1.0 message
with a timestamp), `MidiMessageRef<'_>` (borrowed bytes plus sample offset), the sealed
`MidiData` accessor trait both implement, `MidiMessageKind`, `MidiError`, and `MidiBuffer`
(packed, sample-ordered events with `Events` iteration). Covers JUCE's MidiMessage,
MidiMessageMetadata and MidiBuffer, plus MIDI 2.0 UMP: `UmpPacket`, `UmpMessageType`,
`SysExStatus` and `UmpBuffer`/`UmpEvent`/`UmpEvents` (JUCE's `ump` namespace). Codecs follow.

## Ledger rows
juce_audio_basics/MidiMessage, juce_audio_basics/MidiBuffer (UMP rows when the spec room adds them)

## Invariants
- A `MidiMessage` or `MidiMessageRef` always holds exactly one well-formed message: every
  constructor and parser validates, and parsers return `SYRP-MIDI-0001` instead of panicking.
  Byte layout: channel messages as status + data; SysEx `F0 .. F7`; SMF meta `FF type
  varlen data`; a lone `FF` is System Reset. A note on with velocity 0 is a note off.
- Channels are 1 to 16 in the API; out-of-range constructor arguments give `SYRP-MIDI-0002`.
- `MidiBuffer`: heads sorted by sample offset, equal offsets in insertion order; one byte
  arena, append-only until `clear`. `try_add_event` never allocates and returns
  `SYRP-MIDI-0003` when heads or arena are full, leaving the buffer unchanged.
- `UmpPacket` is always well formed for its message type: constructors check every field
  (`SYRP-MIDI-0002`), `read` checks size and constrained fields (`SYRP-MIDI-0004`) and never
  panics. Groups are field values 0 to 15; channels are 1 to 16 as in the MIDI 1.0 API.
  `UmpBuffer` has `MidiBuffer`'s layout over a `u32` word arena; iteration yields `UmpEvent`
  by value; `try_add` never allocates (`SYRP-MIDI-0003` when full).
- Contract types have no public fields (ADR 0003 D3); the `b6_` test scans the sources.

## Thread affinity
RT-safe: `MidiBuffer::{try_add_event, clear, len, iter, range, ..}`, the same `UmpBuffer`
methods (`try_add`), every `UmpPacket` method, `MidiMessageRef` (`RtSafe`)
and every `MidiData` accessor (`tests/buffer.rs` runs the buffer under `syrp_rt::no_alloc`).
Message thread: `MidiMessage` constructors, `from_bytes`, `MidiMessageRef::to_message`,
`MidiBuffer::{add_event, with_capacity, reserve}`, `UmpBuffer::{add, with_capacity}`, and dropping a `MidiBuffer`.

## Stability
contract (frozen in `contracts/syrp-midi.public-api.txt` after review). Add `MidiData`
methods only with default bodies; the trait is sealed so that stays non-breaking.

## How to test
```sh
cargo syrp test --crate syrp-midi
```
B5 uses proptest: random bytes into every parser, and built messages parsed back.
`tests/buffer.rs` installs `syrp_rt::GuardedAllocator`, so an allocation inside `no_alloc`
aborts that test binary. `tests/ump.rs` does the same for `UmpBuffer`.

## Known gaps
WP-0.2-03 ships in increments. Still to come: B4 MIDI 1.0 byte stream <-> UMP conversion
with SysEx splitting and running status, with its B5 fuzz tests. Not planned yet:
`MidiBuffer` merge (`addEvents`) and `clear(range)`; typed constructors for every MIDI 2.0
opcode, flex data and stream status (generic constructors cover them now); JUCE's
float-velocity and named-meta helpers (tempo, time signature, text).

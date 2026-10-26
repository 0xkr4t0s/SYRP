# syrp-midi

## Purpose
The L1 MIDI contract (master plan §5, ADR 0003 D2/D3): `MidiMessage` (owned MIDI 1.0 message
with a timestamp), `MidiMessageRef<'_>` (borrowed bytes plus sample offset), the sealed
`MidiData` accessor trait both implement, `MidiMessageKind` and `MidiError`. Covers JUCE's
MidiMessage and MidiMessageMetadata; MidiBuffer, UMP packets and codecs come in later increments.

## Ledger rows
juce_audio_basics/MidiMessage (MidiBuffer and UMP rows with later increments)

## Invariants
- A `MidiMessage` or `MidiMessageRef` always holds exactly one well-formed message: every
  constructor and parser validates, and parsers return `SYRP-MIDI-0001` instead of panicking.
  Byte layout: channel messages as status + data; SysEx `F0 .. F7`; SMF meta `FF type
  varlen data`; a lone `FF` is System Reset. A note on with velocity 0 is a note off.
- Channels are 1 to 16 in the API; out-of-range constructor arguments give `SYRP-MIDI-0002`.
- Contract types have no public fields (ADR 0003 D3); the `b6_` test scans the sources.

## Thread affinity
Any thread. `MidiMessageRef` is `RtSafe` and every `MidiData` accessor is RT-safe (no
allocation). `MidiMessage` constructors, `from_bytes` and `MidiMessageRef::to_message`
allocate: message thread or other non-RT code only.

## Stability
contract (frozen in `contracts/syrp-midi.public-api.txt` after review). Add `MidiData`
methods only with default bodies; the trait is sealed so that stays non-breaking.

## How to test
```sh
cargo syrp test --crate syrp-midi
```
B5 uses proptest: random bytes into every parser, and built messages parsed back.

## Known gaps
WP-0.2-03 ships in increments. Still to come: B2 `MidiBuffer` (packed event heads plus one
byte arena, non-allocating `try_add_event`, range iteration, `SYRP-MIDI-0003` when full);
B3 UMP packet types for every message type and `UmpBuffer`; B4 MIDI 1.0 byte stream <-> UMP
conversion with SysEx splitting and running status; their B5 fuzz tests. Not planned yet:
JUCE's float-velocity and named-meta helpers (tempo, time signature, text).

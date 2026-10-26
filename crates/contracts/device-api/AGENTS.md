# syrp-device-api

## Purpose
The L1 device backend contract (master plan §3, `juce_audio_devices`): traits that every audio
and MIDI backend crate implements, plus in-memory fakes for tests. No platform code lives here.
Audio: `AudioDeviceType` enumerates and creates `AudioDevice`s (open with an
`AudioDeviceSetup` of rate, buffer size and `ChannelMask`s; start with an `AudioCallback` that
gets a `DeviceRtCtx`; stop; latency, xruns, rates and sizes). MIDI: `MidiDeviceType` opens
`MidiInput`s (timestamped `syrp_midi::MidiMessage`s to a `MidiInputCallback`) and
`MidiOutput`s. Hot-plug changes reach the message loop as `DeviceChange`s through a
`DeviceChangeNotifier`. `fake` has in-memory versions of all of it. Covers JUCE's
AudioIODeviceType, AudioIODevice, AudioIODeviceCallback, AudioDeviceManager::AudioDeviceSetup,
MidiInput, MidiOutput and MidiDeviceInfo.

## Ledger rows
juce_audio_devices/AudioIODevice, juce_audio_devices/AudioIODeviceCallback,
juce_audio_devices/AudioIODeviceType, juce_audio_devices/MidiInput, juce_audio_devices/MidiOutput

## Invariants
- Contract types have no public fields (ADR 0003 D3); the `b7_` test scans the sources.
- `DeviceId`s come from persistent backend identifiers, never enumeration indices.
- The callback's input and output blocks have one channel per active mask bit, in ascending
  channel order, `frames() <= buffer_size`; the output is zeroed before `process`.
- MIDI timestamps are seconds on one monotonic clock per `MidiDeviceType`.
- `DeviceChangeNotifier::notify` never runs the listener inline; it posts a `Task`.
- `about_to_start` and `stopped` run off the audio thread; `stop` hands the callback back so
  it is dropped off the audio thread.

## Thread affinity
`AudioCallback::process` runs on the audio thread and must not allocate, lock or block.
`ChannelMask`, `AudioDeviceSetup` and `DeviceRtCtx` are `RtSafe`. `AudioDevice` methods are
called from any thread but the audio thread. `MidiInputCallback::message` runs on the backend's MIDI thread; device-change listeners run on
the message thread. Fakes lock a mutex in `run_block`: they are test
doubles, not RT code.

## Stability
contract (frozen once the PR manager commits `contracts/syrp-device-api.public-api.txt`).

## How to test
```sh
cargo syrp test --crate syrp-device-api
```

## Known gaps
f64 device I/O, MIDI 2.0 UMP endpoints and virtual MIDI ports are not covered.

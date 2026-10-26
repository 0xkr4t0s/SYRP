# syrp-device-api

## Purpose
The L1 device backend contract (master plan §3, `juce_audio_devices`): traits that every audio
and MIDI backend crate implements, plus in-memory fakes for tests. No platform code lives here.
Increment 1 has `AudioDevice` (open with an `AudioDeviceSetup` of rate, buffer size and
`ChannelMask`s; start with an `AudioCallback`; stop; latency, xruns, rates and sizes),
`AudioCallback` and its `DeviceRtCtx`, `DeviceId`, `DeviceError`, and
`fake::{FakeAudioDevice, FakeAudioSpec, FakeAudioDriver}`. Covers JUCE's AudioIODevice,
AudioIODeviceCallback and AudioDeviceManager::AudioDeviceSetup.

## Ledger rows
juce_audio_devices/AudioIODevice, juce_audio_devices/AudioIODeviceCallback

## Invariants
- Contract types have no public fields (ADR 0003 D3).
- `DeviceId`s come from persistent backend identifiers, never enumeration indices.
- The callback's input and output blocks have one channel per active mask bit, in ascending
  channel order, `frames() <= buffer_size`; the output is zeroed before `process`.
- `about_to_start` and `stopped` run off the audio thread; `stop` hands the callback back so
  it is dropped off the audio thread.

## Thread affinity
`AudioCallback::process` runs on the audio thread and must not allocate, lock or block.
`ChannelMask`, `AudioDeviceSetup` and `DeviceRtCtx` are `RtSafe`. `AudioDevice` methods are
called from any thread but the audio thread. Fakes lock a mutex in `run_block`: they are test
doubles, not RT code.

## Stability
contract (frozen once the PR manager commits `contracts/syrp-device-api.public-api.txt`).

## How to test
```sh
cargo syrp test --crate syrp-device-api
```

## Known gaps
Next WP-0.2-05 increment: `AudioDeviceType` (B1), MIDI device type, input and output traits
(B4), device-change notifications through `syrp-loop-api` (B5), and their fakes. f64 device
I/O and UMP endpoints are not covered.

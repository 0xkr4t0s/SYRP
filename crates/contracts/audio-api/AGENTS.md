# syrp-audio-api

## Purpose
The L1 audio data contract (master plan §5.8, ADR 0003 D2/D3): non-owning planar views
`AudioBlock`/`AudioBlockMut` over `f32`/`f64` (`AudioSample`) and `ProcessConfig` (rate, max
block, `Precision`, `ProcessMode`). Covers JUCE's dsp::AudioBlock, the view side of AudioBuffer
and dsp::ProcessSpec. `ChannelSet`/`BusLayout` (AudioChannelSet, BusesLayout) and the play-head
types (AudioPlayHead::PositionInfo) arrive in the next increment.

## Ledger rows
juce_dsp/AudioBlock, juce_audio_basics/AudioChannelSet, juce_audio_basics/AudioPlayHead,
juce_dsp/ProcessSpec (owning AudioBuffer is `syrp-audio-basics`)

## Invariants
- Contract types have no public fields (ADR 0003 D3); the `b7_` test scans the sources.
- A view is `channels` channels of `frames` samples, channel `c` at `c * stride`;
  `frames <= padded_frames() <= stride`. Padding is scratch for SIMD kernels.
- A frame sub-range keeps the padding only when it reaches its parent's last frame;
  otherwise `padded_frames() == frames()`, so kernels never touch samples outside the range.
- `is_simd_aligned()`: every channel start on 64 bytes and stride in bytes a multiple of 64.
  `from_raw_parts_aligned` debug-asserts it. Owning buffers must always be aligned.
- Slicing, splitting and reborrowing views never allocate.

## Thread affinity
Views, `ChannelsMut`, `ProcessConfig`, `Precision` and `ProcessMode` are `RtSafe` and
any-thread (views are `Send`/`Sync` like the slices they borrow). Nothing here needs
`MessageThread`.

## Stability
contract (frozen in `contracts/syrp-audio-api.public-api.txt` once committed after merge).

## How to test
```sh
cargo syrp test --crate syrp-audio-api
```

## Known gaps
B2 (`ChannelSet`, `Speaker`, `Bus`, `BusLayout`, `LayoutResponse`) and B4 (`PlayHeadInfo`,
`PlayHead`) are the next WP-0.2-02 increment. Owning aligned buffers, converters and
`AudioWorkgroup` are `syrp-audio-basics`.

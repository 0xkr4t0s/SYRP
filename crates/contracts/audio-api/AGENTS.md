# syrp-audio-api

## Purpose
The L1 audio data contract (master plan §5.8, ADR 0003 D2/D3): non-owning planar views
`AudioBlock`/`AudioBlockMut` over `f32`/`f64` (`AudioSample`), `ChannelSet` and `Speaker`
(mono to 7.1.4 plus discrete N), `Bus`, `BusLayout` and the negotiation answer `LayoutResponse`,
`ProcessConfig` (rate, max block, `Precision`, `ProcessMode`), and transport position
`PlayHeadInfo` with the `PlayHead` source trait. Covers JUCE's dsp::AudioBlock, the view side of
AudioBuffer, AudioChannelSet, AudioProcessor::BusesLayout, dsp::ProcessSpec and
AudioPlayHead::PositionInfo.

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
- Slicing, splitting and reborrowing views never allocate. `impl Iterator` returns name their
  captures (`use<'a, T>`, `use<>`) so they never borrow the view itself; `into_*` methods on
  `AudioBlockMut` hand out data for the full `'a`.
- Canonical speaker order is the order `ChannelSet::speakers()` yields; never reorder a named
  layout (wrappers map host orders to it).

## Thread affinity
Views, `ChannelsMut`, `ChannelSet`, `Speaker`, `ProcessConfig`, `Precision`, `ProcessMode`,
`PlayHeadInfo`, `TimeSignature` and `LoopPoints` are `RtSafe` and any-thread (views are
`Send`/`Sync` like the slices they borrow). `Bus` and `BusLayout` own strings and vectors: build
and negotiate them on the message thread. `PlayHead::position` must be RT-safe.

## Stability
contract (frozen in `contracts/syrp-audio-api.public-api.txt`).

## How to test
```sh
cargo syrp test --crate syrp-audio-api
```

## Known gaps
Owning aligned buffers, converters and `AudioWorkgroup` are `syrp-audio-basics`. No Atmos
layouts beyond 7.1.4 (`Speaker` is `#[non_exhaustive]`); no bar count, frame rate or edit
origin on `PlayHeadInfo` yet.

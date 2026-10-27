# syrp-format-wav

## Purpose
Layer L3 WAV codec (WP-2.1-14, master plan §4.2): `WavAudioFormat` implements the
`syrp-format-api` contract for RIFF WAVE, `WAVE_FORMAT_EXTENSIBLE` and RF64/BW64. Covers JUCE's
`WavAudioFormat`. Built from public layouts only: Microsoft RIFF and `WAVEFORMATEXTENSIBLE`
docs, EBU Tech 3285 (BWF), EBU Tech 3306 (RF64), ITU-R BS.2088 (BW64). Delivered in stacked
increments: B1–B2 plain RIFF, B2 extensible/masks/RF64 (this one), B3 metadata, B4 writer,
B5–B6 tests and scenarios.

| Path | What |
|---|---|
| `src/riff.rs` | Chunk walk, `ds64`, `fmt ` parsing |
| `src/layout.rs` | Channel mask ↔ `ChannelSet` (private table of Microsoft's speaker bits) |
| `src/reader.rs` | `WavReader`, sample decoding |
| `src/error.rs` | `WavError` (`SYRP-FORMAT-0006`..`0012`) |

## Ledger rows
juce_audio_formats/WavAudioFormat

## Invariants
- Integer samples decode as `value / 2^(valid_bits - 1)` (8-bit offset by 128 first), the
  WP-2.1-02 B2 mapping, implemented locally (no `syrp-audio-basics` dependency). Bits below
  `valid_bits_per_sample` are padding and ignored.
- Channels come out in `ChannelSet` canonical order; `layout::from_mask` gives the permutation.
- No panic on any input. Every allocation sized by the input is checked against the input
  length first and uses `try_reserve_exact` (`SYRP-FORMAT-0011` on failure).
- `WavError` converts into `FormatError` with its code at the start of the reason text.

## Open questions (Syrp's choices, compare with the harness)
- Streamed files: a `data` size of `0xFFFFFFFF`, or 0 when the RIFF size is 0 or
  `0xFFFFFFFF`, runs to the end of the input (`b2_streamed_data_size_runs_to_the_end_of_the_input_syrp_choice`).
- A `data` chunk past the end of the input is cut to the whole frames present
  (`b2_data_past_the_end_is_cut_to_whole_frames_present_syrp_choice`).
- A missing `fmt ` or `data` chunk is `SYRP-FORMAT-0008` (`b2_missing_fmt_or_data_is_0008_syrp_choice`);
  `fmt ` may come after `data` when `data` has a known size.
- Plain (non-extensible) headers: 1 channel is mono, 2 stereo, more discrete
  (`b2_plain_header_is_mono_stereo_or_discrete_syrp_choice`).
- `valid_bits_per_sample` of 0 means the whole container (`b2_valid_bits_zero_means_the_whole_container_syrp_choice`).
- Back left/right map to the surround pair, or to the rear pair when the mask also has side
  speakers. A mask whose bit count differs from the channel count, that uses a bit with no
  contract speaker, or that forms no named layout gives a discrete layout in file order.
- `sample_rates()` is empty (any whole rate); `name()` is `"WAV file"`.
- An unknown chunk that runs past the end ends the walk without an error.

## Thread affinity
Any thread. Readers do I/O and are not for the audio thread.

## Stability
experimental

## How to test
```sh
cargo syrp test --crate syrp-format-wav
cargo syrp wp check WP-2.1-14
```
Test files are built byte by byte in `tests/common/mod.rs` from the public layouts.

## Known gaps
- Metadata chunks (B3) and the writer (B4; `create_writer` returns `SYRP-FORMAT-0003`).
- Byte-flip/truncation proptest (B5) and parity scenarios (B6): final increment.
- Not in this WP: Wave64, ADPCM and other compressed codecs, memory-mapped readers.

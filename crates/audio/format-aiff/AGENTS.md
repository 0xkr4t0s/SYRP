# syrp-format-aiff

## Purpose
AIFF and AIFF-C for the `syrp-format-api` contract: `AiffAudioFormat` (JUCE's
`AiffAudioFormat`) and the 80-bit extended helpers `extended_to_f64` / `f64_to_extended`.
Built only from Apple's public *Audio Interchange File Format* 1.3 and AIFF-C specifications
and JUCE's public documentation for behaviour. Reading: plain AIFF integer PCM (1 to 32 bits)
and AIFF-C `NONE`, `sowt`, `fl32`/`FL32`, `fl64`/`FL64`.

## Ledger rows
juce_audio_formats/AiffAudioFormat

## Invariants
- Chunks are walked big-endian with the pad byte after odd sizes; unknown chunks become
  `MetadataItem::Other(id, bytes)`. `COMM`, `SSND` and AIFF-C `FVER` are structure, not metadata.
- Every size and count is checked against the remaining input before allocating
  (`try_reserve_exact`); malformed input is `SYRP-FORMAT-0002`, unknown compression
  `SYRP-FORMAT-0003` naming the four-character code. No panics on any input.
- Integers decode as `value / 2^(bits - 1)` (8-bit is signed), computed in `f64` and rounded once
  to `f32` when the destination is `f32`. Samples narrower than their container are
  left-justified; the low pad bits are dropped.
- Metadata is in file order. In this increment every chunk other than `COMM`, `SSND` and
  `FVER` is `Other(id, bytes)`; typed `MARK`/`INST`/text decoding (B3) is next.
- `extended_to_f64` rounds to nearest even (exact for every `f64`-representable value) and
  rejects infinities, NaNs, unnormals, pseudo-denormals and values beyond `f64::MAX`.

## Open questions (Syrp's choices, pinned by `_syrp_choice` tests, for the harness)
- `b2_frame_count_mismatch_uses_the_smaller_syrp_choice`: length = min(COMM frames, whole frames in SSND).
- `b2_truncated_ssnd_is_cut_to_the_bytes_present_syrp_choice`: an SSND running past the end
  of the stream (or FORM) is cut to the bytes present; a FORM claiming more than the stream is
  walked as far as the stream goes; bytes after the FORM and a trailing partial chunk header
  are ignored. Any other chunk running past the end is an error.
- A chunk id that is not UTF-8 decodes one character per byte.
- AIFF-C `fl32` requires a COMM sample size of 32 and `fl64` of 64.
- `name()` is `"AIFF file"` and `sample_rates()` is empty (any rate); the public documentation
  does not give JUCE's values.
- Channel layout: 1 → mono, 2 → stereo, otherwise `discrete(n)`.

## Thread affinity
`AiffAudioFormat` is `Send + Sync`; readers are `Send` and used by one thread at a time. Not
RT-safe (I/O and allocation).

## Stability
experimental

## How to test
```sh
cargo syrp test --crate syrp-format-aiff
```

## Known gaps
Typed metadata reading (B3), writing (B4), round-trip and fuzz tests (B5) and parity
scenarios (B6) arrive in later increments of WP-2.1-17. No compressed AIFF-C codecs (`ima4`, `ulaw`, `alaw`), no
memory-mapped reader.

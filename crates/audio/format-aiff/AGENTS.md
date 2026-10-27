# syrp-format-aiff

## Purpose
AIFF and AIFF-C for the `syrp-format-api` contract: `AiffAudioFormat` (JUCE's
`AiffAudioFormat`) and the 80-bit extended helpers `extended_to_f64` / `f64_to_extended`.
Built only from Apple's public *Audio Interchange File Format* 1.3 and AIFF-C specifications
and JUCE's public documentation for behaviour. Reading: plain AIFF integer PCM (1 to 32 bits)
and AIFF-C `NONE`, `sowt`, `fl32`/`FL32`, `fl64`/`FL64`, with `MARK`, `INST` and text chunks.
Writing: plain AIFF for 8/16/24/32-bit integers, AIFF-C `fl32` (with `FVER`) for 32-bit float.

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
- Metadata is in file order. `MARK` → `Cue` (id, position, Pascal-string name as label);
  `INST` → its sustain then release loop as `Loop` (forward or ping-pong; start and end are the
  referenced markers' positions), then `Other("INST", the 20+ raw bytes)` for base note, detune,
  ranges and gain. `NAME` → `Tag::Title`, `AUTH` → `Tag::Artist`, `(c) ` → `Tag::Copyright`,
  `ANNO` → `Tag::Comment` (each `ANNO` is its own item). Everything else → `Other`.
- Writing: chunks are `FORM`, [`FVER`], `COMM`, `MARK`, `INST`, then text tags and `Other`
  items in their order, then `SSND` (offset 0, block size 0). Sizes are patched by `flush`
  (so the file so far is readable), `finish`, and best-effort on drop; odd data gets a pad
  byte. Float to integer: scale by 2^(bits - 1), round half away from zero, clip, NaN → 0
  (WP-2.1-02 B2's rule, implemented locally). A FORM over 4 GiB is `SYRP-FORMAT-0003`.
- `AiffAudioFormat::unwritable_items` and the writer share one plan, so they always agree.
- `extended_to_f64` rounds to nearest even (exact for every `f64`-representable value) and
  rejects infinities, NaNs, unnormals, pseudo-denormals and values beyond `f64::MAX`.

## Mapping from JUCE's documented metadata keys
JUCE documents `appleOneShot`, `appleRootSet`, `appleRootNote`, `appleBeats`,
`appleDenominator`, `appleNumerator`, `appleTag` and `appleKey` as read from a `basc` chunk.
Syrp keeps `basc` (and `trns`, `cate`) as `Other(id, bytes)`; no typed mapping yet.

## Open questions (Syrp's choices, pinned by `_syrp_choice` tests, for the harness)
- `b2_frame_count_mismatch_uses_the_smaller_syrp_choice`: length = min(COMM frames, whole frames in SSND).
- `b2_truncated_ssnd_is_cut_to_the_bytes_present_syrp_choice`: an SSND running past the end
  of the stream (or FORM) is cut to the bytes present; a FORM claiming more than the stream is
  walked as far as the stream goes; bytes after the FORM and a trailing partial chunk header
  are ignored. Any other chunk running past the end is an error.
- `b3_unresolvable_loops_are_dropped_syrp_choice`: an `INST` loop with an unknown play mode, a
  missing marker or end < start adds no `Loop` (the raw `INST` still carries it).
- `INST` loop end = end marker position, as the spec states (the contract calls `end`
  inclusive; AIFF markers sit between samples). Harness to confirm JUCE's value.
- `b4_float_to_int_rounds_half_away_clips_and_zeroes_nan_syrp_choice`: the WP-2.1-02 B2 rule.
- `b4_loops_inst_cannot_hold_are_dropped_syrp_choice`: only the first two forward or
  ping-pong loops with play count 0 and 32-bit ends are written (sustain, then release);
  backward loops, play counts, and cues with ids outside 1..=32767, repeated ids, positions
  over 32 bits or labels over 255 bytes are dropped, as are tags other than title, artist,
  copyright and comment, `Bext`, extra `INST` items, and `Other` ids that are not four bytes
  or that the writer makes itself. Loop ends reuse a cue at the same position, otherwise a
  new unnamed marker (which reads back as a label-less cue). Without a raw `INST` item the
  instrument fields default to note 60, detune 0, keys 0..127, velocities 1..127, gain 0.
- Text that is not UTF-8 decodes one character per byte; trailing NULs of text chunks are
  dropped; an empty marker name gives no label.
- AIFF-C `fl32` requires a COMM sample size of 32 and `fl64` of 64; a marker id <= 0 is an error.
- `name()` is `"AIFF file"` and `sample_rates()` is empty (any rate); the public documentation
  does not give JUCE's values.
- Channel layout: 1 → mono, 2 → stereo, otherwise `discrete(n)`.

## Parity scenarios
`parity/scenarios/formats/aiff/` (schema `scenario.schema.json`): `read/*.json` embed every B5
catalogue file (`tests/common/files.rs`), malformed ones included (`bad_*`, `choice_*`);
`write/*.json` write 8/16/24/32-bit integer and 32-bit float with cues, loops and text and
re-read. `b6_scenarios_hold_the_test_files` keeps them in sync (`SYRP_BLESS=1` regenerates).
Inputs only; the oracle supplies expected outputs.

## Thread affinity
`AiffAudioFormat` is `Send + Sync`; readers are `Send` and used by one thread at a time. Not
RT-safe (I/O and allocation).

## Stability
experimental

## How to test
```sh
cargo syrp test --crate syrp-format-aiff
PROPTEST_CASES=20000 cargo test -p syrp-format-aiff --test fuzz   # longer fuzz run
```
`tests/fuzz.rs` installs a counting global allocator (one test per binary on purpose).

## Known gaps
No compressed AIFF-C codecs (`ima4`, `ulaw`, `alaw`), no
memory-mapped reader.

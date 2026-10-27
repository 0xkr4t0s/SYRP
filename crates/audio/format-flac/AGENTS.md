# syrp-format-flac

## Purpose
FLAC files behind the `syrp-format-api` contract: `FlacAudioFormat` (JUCE's `FlacAudioFormat`).
Decoding wraps `claxon` 0.4; the metadata blocks are parsed here from RFC 9639's layout, because
`claxon` skips `SEEKTABLE`, `CUESHEET` and `PICTURE`. Encoding wraps `flacenc` 0.5.
Ogg FLAC, the format manager and memory-mapped readers are out of scope.

| Module | What |
|---|---|
| `lib.rs` | `FlacAudioFormat`: name, extensions, writable rates and depths, `can_read` |
| `meta.rs` | `fLaC` marker, ID3v2 skip, `STREAMINFO`, `SEEKTABLE`, Vorbis comments, `CUESHEET` |
| `tags.rs` | Vorbis comments to `Tag`s, `CUESHEET` to `CuePoint`s, `APPLICATION`/`PICTURE` to `Other` |
| `writer.rs` | `FlacWriter`: block-by-block encoding, `VORBIS_COMMENT`, `STREAMINFO` patch |
| `reader.rs` | `FlacReader`: frames through `claxon`, random access over known frame offsets |
| `source.rs` | Buffered source with an absolute byte position, implementing `claxon`'s `ReadBytes` |

## Ledger rows
juce_audio_formats/FlacAudioFormat. Scenarios: `parity/scenarios/formats/flac/` (`read.json`
reads the writer-made files in `inputs/`; `write.json` writes the ramp at depths 16 and 24 and
quality indices 0 to 8), compared on decoded samples at the `exact` profile.

## Invariants
- Samples are divided by 2^(bits − 1) (WP-2.1-02 B2's rule, implemented locally: format crates
  do not depend on `syrp-audio-basics`).
- Forward reads never decode a frame twice. Other starts restart from the nearest known frame:
  the first, a `SEEKTABLE` point, or one decoded frame per 65,536 samples (`REMEMBER_SPACING`).
- Every malformed input is a coded `FormatError` (`0001` I/O, `0002` bad data, `0003`
  unsupported), never a panic. `FormatError` is a frozen enum, so FLAC failures reuse those
  codes with FLAC-specific reasons. Lengths are checked against the bytes left before any
  allocation, and allocations use `try_reserve_exact`.
- Decoded frames must match `STREAMINFO` (channel count, largest block, sample range) and the
  expected position (full-size frames); a `SEEKTABLE` point naming the wrong frame is `0002`.
- Metadata stays in file order.
- The writer holds at most one block of samples (1,152 or 4,096 frames) and writes each frame
  as soon as its block is full. `finish` (or a best-effort `Drop`) writes the last, shorter
  frame and seeks back to patch `STREAMINFO` (sizes, length, MD5 of the decoded audio).
- Float input is quantised by WP-2.1-02 B2's rule: × 2^(bits − 1), round half away from
  zero, clip, NaN → 0.

## Thread affinity
`FlacAudioFormat` is `Send + Sync` and stateless. Readers and writers are `Send`, used by one background
thread at a time, and not RT-safe (I/O and allocation).

## Stability
experimental

## How to test
```sh
cargo syrp test --crate syrp-format-flac
SYRP_BLESS=1 cargo test -p syrp-format-flac --test scenarios   # after a writer change
```
`tests/props.rs` holds the B5 proptests (24 cases each; `PROPTEST_CASES` does not override the
in-file count, so raise it there for a long fuzz run).
Test streams come from `flacenc` and the B4 writer, plus metadata blocks
built from RFC 9639's layout in `tests/common/mod.rs`; nothing is copied from elsewhere.

## Dependencies
- `claxon` 0.4.3, Apache-2.0 (decoding).
- `flacenc` 0.5.1, Apache-2.0, default features off (encoding; no threads, logging or serde).

## Open questions (Syrp's choices, for the parity harness)
- `name()` is `"FLAC file"`; the writer's rates (8 kHz to 384 kHz, 15 common rates) and depths
  (16, 24) are Syrp's: the public `FlacAudioFormat` documentation lists none of them.
- A leading ID3v2 tag is skipped when reading; `can_read` is true only when `fLaC` follows the
  tag inside the given header (`b1_leading_id3v2_tag_is_skipped_syrp_choice`).
- A `STREAMINFO` total length of 0 (unknown) is `SYRP-FORMAT-0003`
  (`b2_unknown_total_length_is_refused_syrp_choice`).
- Compression levels 0 to 8 (`FlacAudioFormat::quality_options`, default 5) map to `flacenc`
  block size, stereo decorrelation and LPC order in `writer.rs`; the vendor string is
  `Syrp syrp-format-flac <version>`. Tags are written at creation; other items are skipped.
- An empty stream is written with total length 0, which this reader then refuses (`0003`).
- Channel layouts: 1 is mono, 2 stereo, anything else `ChannelSet::discrete(n)`.
- A cue point's id is `track_number << 8 | index_number`, at track offset + index offset, with
  no label. The Vorbis vendor string is not reported as a tag.

## Known gaps
- Reading: `claxon` 0.4 decodes only frames whose header names the depth (8, 12, 16, 20, 24
  bits). Other depths, including 32-bit, are refused with `SYRP-FORMAT-0003`.
- Writing: 16 and 24-bit only (`flacenc` 0.5 cannot write 32-bit; 8, 12 and 20 are not
  offered). No `SEEKTABLE`, `CUESHEET` or `PICTURE` is written.
- The parity scenarios have no harness outputs yet (WP-0.1-11); `cargo deny check` has not
  been run locally for the two dependencies (CI gate G5 runs it).

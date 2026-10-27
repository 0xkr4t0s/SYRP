# syrp-format-flac

## Purpose
FLAC files behind the `syrp-format-api` contract: `FlacAudioFormat` (JUCE's `FlacAudioFormat`).
Decoding wraps `claxon` 0.4; the metadata blocks are parsed here from RFC 9639's layout, because
`claxon` skips `SEEKTABLE`. Encoding will wrap `flacenc` 0.5 (B4).
Ogg FLAC, the format manager and memory-mapped readers are out of scope.

| Module | What |
|---|---|
| `lib.rs` | `FlacAudioFormat`: name, extensions, writable rates and depths, `can_read` |
| `meta.rs` | `fLaC` marker, ID3v2 skip, `STREAMINFO`, `SEEKTABLE`; other blocks skipped |
| `reader.rs` | `FlacReader`: frames through `claxon`, random access over known frame offsets |
| `source.rs` | Buffered source with an absolute byte position, implementing `claxon`'s `ReadBytes` |

## Ledger rows
juce_audio_formats/FlacAudioFormat

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

## Thread affinity
`FlacAudioFormat` is `Send + Sync` and stateless. Readers are `Send`, used by one background
thread at a time, and not RT-safe (I/O and allocation).

## Stability
experimental

## How to test
```sh
cargo syrp test --crate syrp-format-flac
```
Test streams come from `flacenc` (a dev-dependency in this increment) plus `SEEKTABLE` blocks
built from RFC 9639's layout in `tests/common/mod.rs`; nothing is copied from elsewhere.

## Dependencies
- `claxon` 0.4.3, Apache-2.0 (decoding).
- `flacenc` 0.5.1, Apache-2.0, default features off (test streams now; encoding in B4).

## Open questions (Syrp's choices, for the parity harness)
- `name()` is `"FLAC file"`; the writer's rates (8 kHz to 384 kHz, 15 common rates) and depths
  (16, 24) are Syrp's: the public `FlacAudioFormat` documentation lists none of them.
- A leading ID3v2 tag is skipped when reading; `can_read` is true only when `fLaC` follows the
  tag inside the given header (`b1_leading_id3v2_tag_is_skipped_syrp_choice`).
- A `STREAMINFO` total length of 0 (unknown) is `SYRP-FORMAT-0003`
  (`b2_unknown_total_length_is_refused_syrp_choice`).
- Channel layouts: 1 is mono, 2 stereo, anything else `ChannelSet::discrete(n)`.

## Known gaps
- Reading: `claxon` 0.4 decodes only frames whose header names the depth (8, 12, 16, 20, 24
  bits). Other depths, including 32-bit, are refused with `SYRP-FORMAT-0003`.
- Metadata (B3): `VORBIS_COMMENT`, `CUESHEET`, `APPLICATION` and `PICTURE` blocks are skipped.
- Writing (B4), the round-trip, seek and corruption proptests (B5) and the parity scenarios
  (B6) arrive in later increments of WP-2.1-15.

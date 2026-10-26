# syrp-format-api

## Purpose
The L1 audio file format contract (master plan §4, layer L1): the `AudioFormat` trait (name,
extensions, writable rates and bit depths, header sniffing with `can_read`, `create_reader`,
`create_writer`), `AudioReader` (stream shape, length, metadata, zero-filling `read`),
`AudioWriter` (`write`, `flush`, `finish`), `AudioSpec`/`SampleKind`/`WriterOptions`, and the
metadata model (`Metadata`, `MetadataItem`, `Bext`, `CuePoint`, `SampleLoop`, `LoopKind`, `Tag`).
Covers JUCE's AudioFormat, AudioFormatReader, AudioFormatWriter and their metadata maps.
Codecs (`syrp-format-*`) and the manager (`syrp-formats`) implement and use it.

## Ledger rows
juce_audio_formats/AudioFormat, juce_audio_formats/AudioFormatReader,
juce_audio_formats/AudioFormatWriter

## Invariants
- Samples move only through `syrp-audio-api` views. `AnyBlock`/`AnyBlockMut` are sums of
  `AudioBlock`/`AudioBlockMut` over `f32`/`f64`; they own nothing and add no layout.
- `AudioReader::read` (provided) accepts any start (negative or past the end) and any channel
  count; it zero-fills frames outside `0..length()` and channels beyond the file's, and calls
  the required `read_samples` only with an in-file, non-empty range and `dest.channels() <=`
  the file's channel count.
- `AudioWriter::write` (provided) rejects a channel-count mismatch (`SYRP-FORMAT-0005`) and
  skips empty blocks before calling `write_samples`. Writers report I/O failure as
  `SYRP-FORMAT-0001` and never panic on it; `finish(self: Box<Self>)` keeps the traits
  object-safe.
- `AudioSpec` is always valid (checked in `new`). Contract structs have no public fields
  (ADR 0003 D3); the `b6_` test scans the sources.
- `Metadata` keeps file order; `set_tag` keeps one value per `Tag`.

## Thread affinity
`AudioFormat` is `Send + Sync`; readers and writers are `Send` and belong to one background
thread at a time. None of this is RT-safe: readers and writers do I/O and may allocate. Feed
the audio thread through a buffering source (`syrp-formats`).

## Stability
contract (to be frozen in `contracts/syrp-format-api.public-api.txt` by the PR manager).

## How to test
```sh
cargo syrp test --crate syrp-format-api
```

## Known gaps
First increment: the `AudioFormat`, `AudioReader` and `AudioWriter` traits and the
`AnyBlock`/`AnyBlockMut` view enums are the second increment of WP-0.2-04; the invariants above
that name them describe that increment.
Streams are `std::io` `Read + Seek` / `Write + Seek` until `syrp-io` lands. No mmap, subsection
readers or manager (`syrp-formats`); no `bext` UMID or loudness fields (use `Other`).

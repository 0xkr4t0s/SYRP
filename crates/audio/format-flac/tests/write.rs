// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B4: writing through flacenc, block by block, with STREAMINFO patched on finish.

mod common;

use std::io::{Cursor, Seek, SeekFrom, Write};
use std::sync::{Arc, Mutex};

use common::*;
use syrp_audio_api::{AudioBlock, ChannelSet};
use syrp_format_api::{AudioFormat, AudioSpec, Metadata, MetadataItem, SampleKind, Tag};
use syrp_format_api::{AudioWriter, WriterOptions};
use syrp_format_flac::FlacAudioFormat;

/// An output whose bytes the test can inspect while the writer still owns it.
#[derive(Clone, Default)]
struct Shared(Arc<Mutex<Cursor<Vec<u8>>>>);

impl Write for Shared {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().write(b)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Seek for Shared {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        self.0.lock().unwrap().seek(to)
    }
}

impl Shared {
    fn bytes(&self) -> Vec<u8> {
        self.0.lock().unwrap().get_ref().clone()
    }
}

fn spec(channels: u32, bits: u32) -> AudioSpec {
    AudioSpec::new(
        44_100.0,
        ChannelSet::canonical(channels),
        bits,
        SampleKind::Int,
    )
    .unwrap()
}

fn writer(options: &WriterOptions) -> (Box<dyn AudioWriter>, Shared) {
    let out = Shared::default();
    let w = FlacAudioFormat::new()
        .create_writer(Box::new(out.clone()), options)
        .unwrap();
    (w, out)
}

/// Writes channel-after-channel `data` in uneven chunks.
fn write_all(w: &mut dyn AudioWriter, data: &[f64], channels: usize) {
    let frames = data.len() / channels;
    let mut at = 0;
    for chunk in [1, 999, 4096, 3, 7000].iter().cycle() {
        if at >= frames {
            break;
        }
        let n = (*chunk).min(frames - at);
        let mut part = Vec::with_capacity(n * channels);
        for ch in 0..channels {
            part.extend_from_slice(&data[ch * frames + at..ch * frames + at + n]);
        }
        w.write(AudioBlock::new(&part, channels, n, n).unwrap().into())
            .unwrap();
        at += n;
    }
}

/// Decodes `bytes` with claxon into interleaved integers.
fn claxon_samples(bytes: &[u8]) -> (claxon::metadata::StreamInfo, Vec<i32>) {
    let mut r = claxon::FlacReader::new(Cursor::new(bytes)).unwrap();
    let info = r.streaminfo();
    (info, r.samples().map(Result::unwrap).collect())
}

/// Interleaved integers to channel-after-channel floats on the `bits` grid.
fn to_floats(ints: &[i32], channels: usize, bits: u32) -> Vec<f64> {
    let frames = ints.len() / channels;
    let scale = f64::from(1u32 << (bits - 1));
    (0..channels * frames)
        .map(|k| f64::from(ints[(k % frames) * channels + k / frames]) / scale)
        .collect()
}

#[test]
fn b4_quality_levels_are_listed_and_checked() {
    let flac = FlacAudioFormat::new();
    assert_eq!(FlacAudioFormat::quality_options().len(), 9);
    for q in 0..9 {
        let (w, out) = writer(&WriterOptions::new(spec(2, 16)).with_quality(q));
        w.finish().unwrap();
        assert!(out.bytes().starts_with(b"fLaC"));
    }
    let make = |o: WriterOptions| flac.create_writer(Box::new(Cursor::new(Vec::new())), &o);
    assert_eq!(
        code(make(WriterOptions::new(spec(2, 16)).with_quality(9))),
        "SYRP-FORMAT-0003"
    );
    assert_eq!(
        code(make(WriterOptions::new(spec(2, 20)))),
        "SYRP-FORMAT-0003"
    );
    let odd_rate = AudioSpec::new(12_345.0, ChannelSet::mono(), 16, SampleKind::Int).unwrap();
    assert_eq!(code(make(WriterOptions::new(odd_rate))), "SYRP-FORMAT-0003");
    let float = AudioSpec::new(44_100.0, ChannelSet::mono(), 32, SampleKind::Float).unwrap();
    assert_eq!(code(make(WriterOptions::new(float))), "SYRP-FORMAT-0003");
    assert_eq!(
        code(make(WriterOptions::new(spec(9, 16)))),
        "SYRP-FORMAT-0003"
    );
}

#[test]
fn b4_written_file_decodes_with_claxon_to_the_same_integers() {
    for (bits, channels, q) in [
        (16, 2, None),
        (24, 1, Some(0)),
        (16, 6, Some(8)),
        (24, 2, Some(3)),
    ] {
        let ints = signal(20_000, channels, bits, 3);
        let options = WriterOptions::new(spec(channels as u32, bits));
        let options = if let Some(q) = q {
            options.with_quality(q)
        } else {
            options
        };
        let (mut w, out) = writer(&options);
        write_all(&mut *w, &to_floats(&ints, channels, bits), channels);
        w.finish().unwrap();
        let (info, decoded) = claxon_samples(&out.bytes());
        assert_eq!((info.samples, info.bits_per_sample), (Some(20_000), bits));
        assert_eq!(
            decoded, ints,
            "{bits}-bit, {channels} channels, quality {q:?}"
        );
    }
}

#[test]
fn b4_streaminfo_is_patched_with_length_and_md5() {
    let ints = signal(10_000, 2, 16, 4);
    let (mut w, out) = writer(&WriterOptions::new(spec(2, 16)));
    write_all(&mut *w, &to_floats(&ints, 2, 16), 2);
    w.finish().unwrap();
    let bytes = out.bytes();
    let (reference, _) = encode(&ints, 2, 16, 44_100, 4096);
    assert_eq!(
        &bytes[26..42],
        &reference[26..42],
        "MD5 of the decoded audio"
    );
    let (info, _) = claxon_samples(&bytes);
    assert_eq!(
        (info.min_block_size, info.max_block_size, info.samples),
        (4096, 4096, Some(10_000))
    );
    assert!(info.min_frame_size.is_some() && info.max_frame_size >= info.min_frame_size);
}

#[test]
fn b4_blocks_are_encoded_as_samples_arrive() {
    let (mut w, out) = writer(&WriterOptions::new(spec(1, 16)).with_quality(5));
    let header = out.bytes().len();
    let block = vec![0.25f32; 4095];
    w.write(AudioBlock::new(&block, 1, 4095, 4095).unwrap().into())
        .unwrap();
    assert_eq!(out.bytes().len(), header, "a partial block waits");
    w.write(AudioBlock::new(&block[..1], 1, 1, 1).unwrap().into())
        .unwrap();
    w.flush().unwrap();
    assert!(
        out.bytes().len() > header,
        "a full block is written at once"
    );
    drop(w); // Dropping without finish still completes the file.
    let (info, decoded) = claxon_samples(&out.bytes());
    assert_eq!((info.samples, decoded.len()), (Some(4096), 4096));
}

#[test]
fn b4_float_input_is_rounded_and_clipped_by_the_converter_rule() {
    let step = 1.0 / 32_768.0;
    let input = [
        1.5 * step,
        -1.5 * step,
        2.5 * step,
        0.49 * step,
        1.0,
        2.0,
        -1.0,
        -3.0,
        f64::NAN,
    ];
    let (mut w, out) = writer(&WriterOptions::new(spec(1, 16)));
    w.write(AudioBlock::new(&input, 1, 9, 9).unwrap().into())
        .unwrap();
    w.finish().unwrap();
    let (_, decoded) = claxon_samples(&out.bytes());
    assert_eq!(decoded, [2, -2, 3, 0, 32_767, 32_767, -32_768, -32_768, 0]);
}

#[test]
fn b4_tags_are_written_as_a_vorbis_comment_block() {
    let mut meta = Metadata::new();
    meta.set_tag(Tag::Title, "Take 3");
    meta.set_tag(Tag::Custom("Mood".into()), "calm");
    meta.push(MetadataItem::Cue(syrp_format_api::CuePoint::new(1, 10)));
    let (w, out) = writer(&WriterOptions::new(spec(1, 16)).with_metadata(meta));
    w.finish().unwrap();
    let bytes = out.bytes();
    let r = claxon::FlacReader::new(Cursor::new(&bytes)).unwrap();
    assert!(r.vendor().unwrap().starts_with("Syrp syrp-format-flac"));
    let tags: Vec<(&str, &str)> = r.tags().collect();
    assert_eq!(tags, [("TITLE", "Take 3"), ("Mood", "calm")]);
}

#[test]
fn b4_custom_tags_vorbis_cannot_hold_are_skipped_syrp_choice() {
    let mut meta = Metadata::new();
    meta.set_tag(Tag::Custom("A=B".into()), "x");
    meta.set_tag(Tag::Custom("Caf\u{e9}".into()), "x");
    meta.set_tag(Tag::Custom("Mood".into()), "calm");
    let (w, out) = writer(&WriterOptions::new(spec(1, 16)).with_metadata(meta));
    w.finish().unwrap();
    let r = claxon::FlacReader::new(Cursor::new(out.bytes())).unwrap();
    let tags: Vec<(&str, &str)> = r.tags().collect();
    assert_eq!(tags, [("Mood", "calm")]);
}

#[test]
fn b4_every_writable_rate_writes_and_higher_rates_are_refused() {
    let flac = FlacAudioFormat::new();
    for &rate in flac.sample_rates() {
        let s = AudioSpec::new(f64::from(rate), ChannelSet::mono(), 16, SampleKind::Int).unwrap();
        let (mut w, out) = writer(&WriterOptions::new(s));
        write_all(&mut *w, &[0.25; 100], 1);
        w.finish().unwrap();
        let (info, decoded) = claxon_samples(&out.bytes());
        assert_eq!((info.sample_rate, decoded.len()), (rate, 100));
    }
    for rate in [176_400.0, 192_000.0, 384_000.0, 44_100.5] {
        let s = AudioSpec::new(rate, ChannelSet::mono(), 16, SampleKind::Int).unwrap();
        let made = flac.create_writer(Box::new(Cursor::new(Vec::new())), &WriterOptions::new(s));
        assert_eq!(code(made), "SYRP-FORMAT-0003", "{rate}");
    }
}

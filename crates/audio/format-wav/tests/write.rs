// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B4: writing samples, headers, metadata and RF64.

mod common;

use common::{Shared, open, read_all_f32, write_file};
use syrp_audio_api::{AudioBlock, ChannelSet};
use syrp_error::Diagnostic;
use syrp_format_api::{
    AudioFormat, AudioSpec, Bext, CuePoint, LoopKind, Metadata, MetadataItem, SampleKind,
    SampleLoop, Tag, WriterOptions,
};
use syrp_format_wav::WavAudioFormat;

fn options(ch: ChannelSet, bits: u32, kind: SampleKind) -> WriterOptions {
    WriterOptions::new(AudioSpec::new(48_000.0, ch, bits, kind).unwrap())
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// Offset of the first chunk `id` after the 12-byte RIFF header.
fn find(b: &[u8], id: &[u8; 4]) -> usize {
    let mut at = 12;
    while &b[at..at + 4] != id {
        let size = u32_at(b, at + 4) as usize;
        at += 8 + size + (size & 1);
    }
    at
}

#[test]
fn b4_every_depth_round_trips_values_on_its_grid() {
    for (bits, kind) in [
        (8, SampleKind::Int),
        (16, SampleKind::Int),
        (24, SampleKind::Int),
        (32, SampleKind::Int),
        (32, SampleKind::Float),
    ] {
        let q = 2f32.powi(-(bits.min(24) as i32 - 1));
        let ch = vec![
            vec![-1.0, -q, 0.0, q, 0.5, 1.0 - q],
            vec![0.25, -0.25, q, -q, 0.0, -1.0],
        ];
        let file = write_file(
            WavAudioFormat::new(),
            &options(ChannelSet::stereo(), bits, kind),
            &ch,
        );
        let mut r = open(file).unwrap();
        assert_eq!(r.spec().bits_per_sample(), bits);
        assert_eq!(read_all_f32(r.as_mut()), ch, "{bits} {kind:?}");
    }
}

#[test]
fn b4_rounds_ties_away_clips_and_zeroes_nan_syrp_choice() {
    let s = 1.0 / 32768.0;
    let ch = vec![vec![0.5 * s, -0.5 * s, 1.5 * s, 1.5, -2.0, f32::NAN]];
    let file = write_file(
        WavAudioFormat::new(),
        &options(ChannelSet::mono(), 16, SampleKind::Int),
        &ch,
    );
    let data = find(&file, b"data") + 8;
    let got: Vec<i16> = file[data..data + 12]
        .chunks(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect();
    assert_eq!(got, [1, -1, 2, i16::MAX, i16::MIN, 0]);
    let file = write_file(
        WavAudioFormat::new(),
        &options(ChannelSet::mono(), 8, SampleKind::Int),
        &[vec![1.0, -1.0]],
    );
    let data = find(&file, b"data") + 8;
    assert_eq!(file[data..data + 2], [255, 0]);
}

#[test]
fn b4_plain_header_up_to_two_channels_and_16_bits_else_extensible() {
    let cases = [
        (ChannelSet::mono(), 16, SampleKind::Int, 1, 0),
        (ChannelSet::stereo(), 8, SampleKind::Int, 1, 0),
        (ChannelSet::stereo(), 24, SampleKind::Int, 0xFFFE, 0x3),
        (ChannelSet::lcr(), 16, SampleKind::Int, 0xFFFE, 0x7),
        (
            ChannelSet::surround_7_1(),
            16,
            SampleKind::Int,
            0xFFFE,
            0x63F,
        ),
        (ChannelSet::discrete(3), 16, SampleKind::Int, 0xFFFE, 0),
        (ChannelSet::mono(), 32, SampleKind::Float, 0xFFFE, 0x4),
    ];
    for (set, bits, kind, tag, mask) in cases {
        let n = set.channel_count();
        let file = write_file(
            WavAudioFormat::new(),
            &options(set, bits, kind),
            &vec![vec![0.0]; n],
        );
        let f = find(&file, b"fmt ");
        assert_eq!(u16_at(&file, f + 8), tag, "{set:?} {bits}");
        if tag == 0xFFFE {
            assert_eq!(u32_at(&file, f + 4), 40);
            assert_eq!(u32_at(&file, f + 28), mask, "{set:?}");
            assert_eq!(
                u16_at(&file, f + 32),
                if kind == SampleKind::Float { 3 } else { 1 }
            );
        }
        assert_eq!(open(file).unwrap().spec().channels(), set);
    }
}

#[test]
fn b4_surround_channels_land_in_wav_order_and_read_back_canonical() {
    let set = ChannelSet::surround_7_1();
    let ch: Vec<Vec<f32>> = (0..8).map(|i| vec![i as f32 / 8.0]).collect();
    let file = write_file(
        WavAudioFormat::new(),
        &options(set, 16, SampleKind::Int),
        &ch,
    );
    let data = find(&file, b"data") + 8;
    // WAV order L R C LFE BL(=Lrs) BR(=Rrs) SL(=Ls) SR(=Rs).
    let order: Vec<i16> = file[data..data + 16]
        .chunks(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]) / 4096)
        .collect();
    assert_eq!(order, [0, 1, 2, 3, 6, 7, 4, 5]);
    assert_eq!(read_all_f32(open(file).unwrap().as_mut()), ch);
}

#[test]
fn b4_junk_is_reserved_and_riff_and_data_sizes_are_patched() {
    let file = write_file(
        WavAudioFormat::new(),
        &options(ChannelSet::mono(), 8, SampleKind::Int),
        &[vec![0.0; 3]],
    );
    assert_eq!(&file[..4], b"RIFF");
    assert_eq!(u32_at(&file, 4) as usize, file.len() - 8);
    assert_eq!((&file[12..16], u32_at(&file, 16)), (&b"JUNK"[..], 28));
    let data = find(&file, b"data");
    assert_eq!(u32_at(&file, data + 4), 3);
    assert_eq!(file.len(), data + 8 + 4, "odd data gets a pad byte");
}

#[test]
fn b4_rf64_when_the_riff_size_passes_the_threshold() {
    let opts = options(ChannelSet::stereo(), 16, SampleKind::Int);
    let ch = vec![vec![0.5; 100], vec![-0.5; 100]];
    let small = write_file(
        WavAudioFormat::new().with_rf64_threshold(10_000),
        &opts,
        &ch,
    );
    assert_eq!(&small[..4], b"RIFF");
    let file = write_file(WavAudioFormat::new().with_rf64_threshold(100), &opts, &ch);
    assert_eq!((&file[..4], u32_at(&file, 4)), (&b"RF64"[..], u32::MAX));
    assert_eq!(&file[12..16], b"ds64");
    let ds64 = |at: usize| u64::from_le_bytes(file[at..at + 8].try_into().unwrap());
    assert_eq!(ds64(20), file.len() as u64 - 8);
    assert_eq!((ds64(28), ds64(36)), (400, 100));
    assert_eq!(u32_at(&file, find(&file, b"data") + 4), u32::MAX);
    let mut r = open(file).unwrap();
    assert_eq!(read_all_f32(r.as_mut()), ch);
}

#[test]
fn b4_metadata_round_trips_in_order_between_fmt_and_data() {
    let mut meta = Metadata::new();
    let bext = Bext::new()
        .with_description("d")
        .with_originator("o")
        .with_originator_reference("r")
        .with_origination_date("2026-10-27")
        .with_origination_time("01:02:03")
        .with_time_reference(1 << 33)
        .with_coding_history("A=PCM\r\n");
    meta.push(MetadataItem::Bext(bext));
    meta.push(MetadataItem::Cue(CuePoint::new(1, 10).with_label("a")));
    meta.push(MetadataItem::Cue(CuePoint::new(2, 20)));
    meta.push(MetadataItem::Loop(
        SampleLoop::new(LoopKind::PingPong, 2, 9)
            .unwrap()
            .with_play_count(2),
    ));
    meta.push(MetadataItem::Tag(Tag::Title, "T".into()));
    meta.push(MetadataItem::Tag(Tag::Custom("IENG".into()), "E".into()));
    meta.push(MetadataItem::Other("iXML".into(), b"<x/>".to_vec()));
    meta.push(MetadataItem::Other("acid".into(), vec![1, 2, 3]));
    let opts = options(ChannelSet::mono(), 16, SampleKind::Int).with_metadata(meta.clone());
    let file = write_file(WavAudioFormat::new(), &opts, &[vec![0.0; 4]]);
    assert!(find(&file, b"fmt ") < find(&file, b"bext"));
    assert!(find(&file, b"acid") < find(&file, b"data"));
    assert_eq!(open(file).unwrap().metadata(), &meta);
}

#[test]
fn b4_bext_extra_bytes_from_a_read_file_are_written_back_once() {
    let mut raw = vec![0u8; 602];
    raw[0] = b'x';
    raw[400] = 7; // a UMID byte
    let mut meta = Metadata::new();
    meta.push(MetadataItem::Bext(Bext::new().with_description("x")));
    meta.push(MetadataItem::Other("bext".into(), raw.clone()));
    let opts = options(ChannelSet::mono(), 16, SampleKind::Int).with_metadata(meta.clone());
    let file = write_file(WavAudioFormat::new(), &opts, &[vec![0.0]]);
    assert_eq!(open(file).unwrap().metadata(), &meta);
}

#[test]
fn b4_other_items_with_writer_owned_or_invalid_ids_are_skipped_syrp_choice() {
    let mut meta = Metadata::new();
    for id in ["data", "fmt ", "ds64", "toolong", "ab"] {
        meta.push(MetadataItem::Other(id.into(), vec![1, 2]));
    }
    meta.push(MetadataItem::Tag(
        Tag::Custom("not four".into()),
        "v".into(),
    ));
    let opts = options(ChannelSet::mono(), 16, SampleKind::Int).with_metadata(meta);
    let file = write_file(WavAudioFormat::new(), &opts, &[vec![0.25]]);
    let mut r = open(file).unwrap();
    assert!(r.metadata().is_empty());
    assert_eq!(read_all_f32(r.as_mut()), [[0.25]]);
}

#[test]
fn b4_fractional_rates_and_positions_past_32_bits_are_unsupported() {
    let spec = AudioSpec::new(44_100.5, ChannelSet::mono(), 16, SampleKind::Int).unwrap();
    let err = WavAudioFormat::new()
        .create_writer(Box::new(Shared::default()), &WriterOptions::new(spec))
        .err()
        .unwrap();
    assert_eq!(err.code().as_str(), "SYRP-FORMAT-0003");
    let mut meta = Metadata::new();
    meta.push(MetadataItem::Cue(CuePoint::new(1, 1 << 32)));
    let opts = options(ChannelSet::mono(), 16, SampleKind::Int).with_metadata(meta);
    let err = WavAudioFormat::new()
        .create_writer(Box::new(Shared::default()), &opts)
        .err()
        .unwrap();
    assert_eq!(err.code().as_str(), "SYRP-FORMAT-0003");
}

#[test]
fn b4_dropping_an_unfinished_writer_still_patches_the_sizes() {
    let sink = Shared::default();
    let opts = options(ChannelSet::mono(), 16, SampleKind::Int);
    let mut w = WavAudioFormat::new()
        .create_writer(Box::new(sink.clone()), &opts)
        .unwrap();
    let data = [0.5f64; 4];
    w.write(AudioBlock::new(&data, 1, 4, 4).unwrap().into())
        .unwrap();
    w.flush().unwrap();
    drop(w);
    assert_eq!(open(sink.bytes()).unwrap().length(), 4);
}

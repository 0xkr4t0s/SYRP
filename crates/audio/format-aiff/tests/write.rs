// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B4: writing AIFF and AIFF-C.

mod common;

use common::{Shared, open, read_all, write_file};
use syrp_audio_api::{AudioBlock, ChannelSet};
use syrp_error::Diagnostic;
use syrp_format_aiff::{AiffAudioFormat, f64_to_extended};
use syrp_format_api::{
    AudioFormat, AudioSpec, CuePoint, LoopKind, Metadata, MetadataItem, SampleKind, SampleLoop,
    Tag, WriterOptions,
};

fn options(bits: u32, kind: SampleKind, channels: ChannelSet) -> WriterOptions {
    WriterOptions::new(AudioSpec::new(44_100.0, channels, bits, kind).unwrap())
}

#[test]
fn b4_integer_writes_plain_aiff_with_exact_sizes() {
    let file = write_file(
        &options(16, SampleKind::Int, ChannelSet::stereo()),
        2,
        &[0.5, -0.5, 0.25, 1.0],
    );
    assert_eq!(&file[..12], b"FORM\0\0\0\x36AIFF");
    assert_eq!(&file[12..20], b"COMM\0\0\0\x12");
    assert_eq!(&file[20..28], [0, 2, 0, 0, 0, 2, 0, 16]);
    assert_eq!(file[28..38], f64_to_extended(44_100.0).unwrap());
    assert_eq!(&file[38..54], b"SSND\0\0\0\x10\0\0\0\0\0\0\0\0");
    // Interleaved L R L R; 1.0 clips to the largest 16-bit value.
    assert_eq!(&file[54..], [0x40, 0, 0x20, 0, 0xC0, 0, 0x7F, 0xFF]);
    assert_eq!(file.len(), 0x36 + 8);
}

#[test]
fn b4_float_writes_aifc_fl32_with_fver() {
    let file = write_file(
        &options(32, SampleKind::Float, ChannelSet::mono()),
        1,
        &[0.75, -2.0],
    );
    assert_eq!(&file[8..12], b"AIFC");
    assert_eq!(&file[12..24], b"FVER\0\0\0\x04\xA2\x80\x51\x40");
    assert_eq!(&file[24..32], b"COMM\0\0\0\x24");
    assert_eq!(&file[50..54], b"fl32");
    let mut r = open(file).unwrap();
    assert_eq!(r.spec().sample_kind(), SampleKind::Float);
    assert_eq!(read_all(&mut *r), [0.75, -2.0]);
}

#[test]
fn b4_odd_data_is_padded_and_sizes_patched_in_finish() {
    let file = write_file(
        &options(8, SampleKind::Int, ChannelSet::mono()),
        1,
        &[0.5, -1.0, 0.0],
    );
    let form = u32::from_be_bytes(file[4..8].try_into().unwrap()) as usize;
    assert_eq!(form + 8, file.len());
    assert_eq!(file.len() % 2, 0);
    assert_eq!(&file[file.len() - 4..], [0x40, 0x80, 0, 0]);
    assert_eq!(open(file).unwrap().length(), 3);
}

#[test]
fn b4_float_to_int_rounds_half_away_clips_and_zeroes_nan_syrp_choice() {
    let lsb = 1.0 / 128.0;
    let input = [
        0.5 * lsb,
        -0.5 * lsb,
        1.5 * lsb,
        2.0,
        -2.0,
        f32::NAN,
        f32::INFINITY,
        -1.0,
    ];
    let file = write_file(&options(8, SampleKind::Int, ChannelSet::mono()), 1, &input);
    assert_eq!(
        &file[file.len() - 8..],
        [1, 0xFF, 2, 0x7F, 0x80, 0, 0x7F, 0x80]
    );
}

#[test]
fn b4_flush_makes_frames_so_far_readable_and_drop_finishes() {
    let sink = Shared::default();
    let opts = options(24, SampleKind::Int, ChannelSet::mono());
    let mut w = AiffAudioFormat::new()
        .create_writer(Box::new(sink.clone()), &opts)
        .unwrap();
    w.write(AudioBlock::new(&[0.5f64], 1, 1, 1).unwrap().into())
        .unwrap();
    w.flush().unwrap();
    assert_eq!(open(sink.bytes()).unwrap().length(), 1);
    w.write(AudioBlock::new(&[0.25f32], 1, 1, 1).unwrap().into())
        .unwrap();
    drop(w);
    assert_eq!(read_all(&mut *open(sink.bytes()).unwrap()), [0.5, 0.25]);
}

#[test]
fn b4_rejects_streams_aiff_cannot_write() {
    let f = AiffAudioFormat::new();
    for (bits, kind) in [(12, SampleKind::Int), (64, SampleKind::Float)] {
        let err = f.create_writer(
            Box::new(Shared::default()),
            &options(bits, kind, ChannelSet::mono()),
        );
        assert_eq!(err.err().unwrap().code().as_str(), "SYRP-FORMAT-0003");
    }
    let wide = options(16, SampleKind::Int, ChannelSet::discrete(40_000));
    let err = f.create_writer(Box::new(Shared::default()), &wide);
    assert_eq!(err.err().unwrap().code().as_str(), "SYRP-FORMAT-0003");
}

#[test]
fn b4_failing_output_is_an_io_error() {
    struct Full;
    impl std::io::Write for Full {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("disk full"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl std::io::Seek for Full {
        fn seek(&mut self, _: std::io::SeekFrom) -> std::io::Result<u64> {
            Ok(0)
        }
    }
    let opts = options(16, SampleKind::Int, ChannelSet::mono());
    let err = AiffAudioFormat::new()
        .create_writer(Box::new(Full), &opts)
        .err()
        .unwrap();
    assert_eq!(err.code().as_str(), "SYRP-FORMAT-0001");
}

fn lp(kind: LoopKind, s: u64, e: u64) -> MetadataItem {
    MetadataItem::Loop(SampleLoop::new(kind, s, e).unwrap())
}

#[test]
fn b4_metadata_is_written_before_ssnd_and_reads_back() {
    let mut meta = Metadata::new();
    meta.push(MetadataItem::Cue(CuePoint::new(1, 0).with_label("start")));
    meta.push(MetadataItem::Cue(CuePoint::new(2, 3).with_label("end")));
    meta.push(lp(LoopKind::Forward, 0, 3));
    meta.push(lp(LoopKind::PingPong, 1, 2));
    let inst = [
        61, 2, 10, 90, 5, 120, 0xFF, 0xFA, 0, 1, 0, 1, 0, 2, 0, 2, 0, 3, 0, 4,
    ];
    meta.push(MetadataItem::Other("INST".into(), inst.to_vec()));
    meta.push(MetadataItem::Tag(Tag::Title, "Take 3".into()));
    meta.push(MetadataItem::Tag(Tag::Artist, "Someone".into()));
    meta.push(MetadataItem::Tag(Tag::Copyright, "2026".into()));
    meta.push(MetadataItem::Tag(Tag::Comment, "one".into()));
    meta.push(MetadataItem::Tag(Tag::Comment, "two".into()));
    meta.push(MetadataItem::Other("APPL".into(), b"odd".to_vec()));
    assert!(AiffAudioFormat::unwritable_items(&meta).is_empty());
    let opts = options(16, SampleKind::Int, ChannelSet::mono()).with_metadata(meta.clone());
    let file = write_file(&opts, 1, &[0.0; 4]);
    let ssnd = file.windows(4).position(|w| w == b"SSND").unwrap();
    assert!(file.windows(4).position(|w| w == b"APPL").unwrap() < ssnd);
    let got = open(file).unwrap().metadata().items().to_vec();
    // MARK, then INST (its loops, then the raw chunk), then the text and other chunks.
    let cue = |id, pos, label: &str| MetadataItem::Cue(CuePoint::new(id, pos).with_label(label));
    let mut want = vec![cue(1, 0, "start"), cue(2, 3, "end")];
    want.push(MetadataItem::Cue(CuePoint::new(3, 1)));
    want.push(MetadataItem::Cue(CuePoint::new(4, 2)));
    want.extend([lp(LoopKind::Forward, 0, 3), lp(LoopKind::PingPong, 1, 2)]);
    want.push(MetadataItem::Other("INST".into(), inst.to_vec()));
    want.extend_from_slice(&meta.items()[5..]);
    assert_eq!(got, want);
}

#[test]
fn b4_loops_inst_cannot_hold_are_dropped_syrp_choice() {
    let mut meta = Metadata::new();
    meta.push(lp(LoopKind::Backward, 0, 1));
    meta.push(lp(LoopKind::Forward, 0, 1));
    meta.push(MetadataItem::Loop(
        SampleLoop::new(LoopKind::Forward, 0, 2)
            .unwrap()
            .with_play_count(3),
    ));
    meta.push(lp(LoopKind::PingPong, 2, 3));
    meta.push(lp(LoopKind::Forward, 4, 5));
    meta.push(lp(LoopKind::Forward, 0, 1 << 33));
    meta.push(MetadataItem::Cue(CuePoint::new(0, 1)));
    meta.push(MetadataItem::Cue(CuePoint::new(70_000, 1)));
    meta.push(MetadataItem::Tag(Tag::Album, "x".into()));
    meta.push(MetadataItem::Other("COMM".into(), vec![]));
    meta.push(MetadataItem::Other("toolong".into(), vec![]));
    let dropped = AiffAudioFormat::unwritable_items(&meta);
    let want: Vec<_> = [0, 2, 4, 5, 6, 7, 8, 9, 10]
        .iter()
        .map(|&i| &meta.items()[i])
        .collect();
    assert_eq!(dropped, want);
    let opts = options(16, SampleKind::Int, ChannelSet::mono()).with_metadata(meta.clone());
    let got = open(write_file(&opts, 1, &[0.0; 6]))
        .unwrap()
        .metadata()
        .items()
        .to_vec();
    let loops: Vec<_> = got
        .iter()
        .filter(|i| matches!(i, MetadataItem::Loop(_)))
        .collect();
    assert_eq!(loops, [&meta.items()[1], &meta.items()[3]]);
}

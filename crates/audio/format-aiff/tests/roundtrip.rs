// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B5: round trips and the catalogue of test files.

mod common;

use common::files::catalogue;
use common::{open, open_code, read_all, write_file};
use syrp_audio_api::{AudioBlockMut, ChannelSet};
use syrp_format_api::{
    AudioSpec, CuePoint, LoopKind, Metadata, MetadataItem, SampleKind, SampleLoop, Tag,
    WriterOptions,
};

fn opts(bits: u32, kind: SampleKind) -> WriterOptions {
    WriterOptions::new(AudioSpec::new(96_000.0, ChannelSet::stereo(), bits, kind).unwrap())
}

/// Writes stereo `data` (channel-major) and reads it back as f32.
fn round_trip(options: &WriterOptions, data: &[f32]) -> Vec<f32> {
    let mut r = open(write_file(options, 2, data)).unwrap();
    assert_eq!(r.spec().sample_rate(), 96_000.0);
    let n = data.len() / 2;
    let mut out = vec![0.0f32; data.len()];
    r.read(AudioBlockMut::new(&mut out, 2, n, n).unwrap().into(), 0)
        .unwrap();
    out
}

#[test]
fn b5_every_format_round_trips_exactly_on_its_grid() {
    for bits in [8u32, 16, 24, 32] {
        let full = 2f64.powi(i32::try_from(bits).unwrap() - 1);
        // The extremes, zero, one step, and steps an f32 can hold exactly.
        let steps = [
            -full,
            full - 1.0,
            0.0,
            1.0,
            -1.0,
            (full / 3.0).floor(),
            -(full / 2.0),
        ];
        let grid: Vec<f32> = steps
            .iter()
            .map(|&k| {
                if bits == 32 && k == full - 1.0 {
                    full - 128.0
                } else {
                    k
                }
            })
            .map(|k| (k / full) as f32)
            .collect();
        let mut data = grid.clone();
        data.extend(grid.iter().rev());
        assert_eq!(
            round_trip(&opts(bits, SampleKind::Int), &data),
            data,
            "{bits}-bit"
        );
    }
    let floats = [
        0.5f32,
        -0.0,
        1.0e-40,
        f32::MAX,
        -3.25,
        f32::MIN_POSITIVE,
        7.0,
        -1.0,
    ];
    assert_eq!(round_trip(&opts(32, SampleKind::Float), &floats), floats);
}

#[test]
fn b5_markers_loops_and_text_round_trip() {
    let mut meta = Metadata::new();
    meta.push(MetadataItem::Cue(CuePoint::new(1, 0).with_label("a")));
    meta.push(MetadataItem::Cue(CuePoint::new(5, 3).with_label("odd")));
    meta.push(MetadataItem::Cue(CuePoint::new(9, 7)));
    meta.push(MetadataItem::Loop(
        SampleLoop::new(LoopKind::PingPong, 3, 7).unwrap(),
    ));
    let inst = [
        64, 3, 1, 120, 2, 126, 0, 6, 0, 2, 0, 5, 0, 9, 0, 0, 0, 0, 0, 0,
    ];
    meta.push(MetadataItem::Other("INST".into(), inst.to_vec()));
    for (tag, text) in [
        (Tag::Title, "T"),
        (Tag::Comment, "c1"),
        (Tag::Artist, "Ä"),
        (Tag::Comment, "c2"),
    ] {
        meta.push(MetadataItem::Tag(tag, text.into()));
    }
    meta.push(MetadataItem::Tag(Tag::Copyright, "(c) 2026".into()));
    meta.push(MetadataItem::Other("ID3 ".into(), b"ID3\x04\0".to_vec()));
    let options = opts(24, SampleKind::Int).with_metadata(meta.clone());
    let r = open(write_file(&options, 2, &[0.0; 16])).unwrap();
    assert_eq!(r.metadata(), &meta);
}

#[test]
fn b5_catalogue_reads_as_built() {
    for case in catalogue() {
        let name = case.name;
        if name.starts_with("bad_") {
            let want = if name == "bad_compression_ima4" {
                "SYRP-FORMAT-0003"
            } else {
                "SYRP-FORMAT-0002"
            };
            assert_eq!(open_code(case.bytes), want, "{name}");
            continue;
        }
        let mut r = open(case.bytes).unwrap();
        let s = read_all(&mut *r);
        let want: Option<&[f64]> = match name {
            "int8_mono" => Some(&[-1.0, -0.5, 0.5, 127.0 / 128.0]),
            "int16_stereo" | "aifc_none_stereo" | "aifc_sowt_stereo" => {
                Some(&[0.5, 32_767.0 / 32_768.0, -0.5, -1.0])
            }
            "int24_mono" => Some(&[-1.0, 1.0 / 8_388_608.0]),
            "int32_mono" => Some(&[2_147_483_647.0 / 2_147_483_648.0, -0.5]),
            "int12_mono" => Some(&[-1.0, 2_047.0 / 2_048.0]),
            "aifc_fl32_stereo" => Some(&[0.5, 1.5, -0.25, f64::from(-1.0e-3f32)]),
            "aifc_fl64_mono" => Some(&[0.1, -0.75, 2.0]),
            "odd_and_unknown_chunks" => Some(&[1.0 / 128.0, 2.0 / 128.0, 3.0 / 128.0]),
            "ssnd_offset_and_block" | "metadata" | "choice_comm_frames_exceed_ssnd" => {
                Some(&[0.5, -0.5, 32_767.0 / 32_768.0, -1.0])
            }
            "choice_truncated_ssnd" => Some(&[0.5, -0.5]),
            _ => None,
        };
        assert_eq!(Some(s.as_slice()), want, "{name}");
        if name == "metadata" {
            let items = r.metadata().items();
            assert_eq!(
                items.len(),
                3 + 2 + 1 + 5 + 2,
                "cues, loops, INST, tags, others"
            );
        }
    }
}

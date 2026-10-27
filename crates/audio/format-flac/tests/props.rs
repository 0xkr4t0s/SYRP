// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B5: round trips through the B4 writer, seeks against one long read, and damaged files.

mod common;

use std::io::Cursor;

use common::*;
use proptest::prelude::*;
use syrp_audio_api::{AudioBlock, AudioBlockMut, ChannelSet};
use syrp_format_api::WriterOptions;
use syrp_format_api::{AudioFormat, AudioSpec, Metadata, MetadataItem, SampleKind, Tag};
use syrp_format_flac::FlacAudioFormat;

/// Writes channel-after-channel `ints` on the `bits` grid as `f32` with the B4 writer.
fn write_flac(ints: &[i32], channels: usize, bits: u32, quality: u32, meta: Metadata) -> Vec<u8> {
    let frames = ints.len() / channels;
    let scale = (1u32 << (bits - 1)) as f32;
    let floats: Vec<f32> = ints.iter().map(|&v| v as f32 / scale).collect();
    let spec = AudioSpec::new(
        48_000.0,
        ChannelSet::canonical(channels as u32),
        bits,
        SampleKind::Int,
    )
    .unwrap();
    let options = WriterOptions::new(spec)
        .with_quality(quality)
        .with_metadata(meta);
    let out = Shared::default();
    let mut w = FlacAudioFormat::new()
        .create_writer(Box::new(out.clone()), &options)
        .unwrap();
    w.write(
        AudioBlock::new(&floats, channels, frames, frames)
            .unwrap()
            .into(),
    )
    .unwrap();
    w.finish().unwrap();
    out.bytes()
}

fn grid(bits: u32, channels: usize, frames: usize) -> impl Strategy<Value = Vec<i32>> {
    let half = 1i32 << (bits - 1);
    prop::collection::vec(-half..half, channels * frames)
}

fn cases() -> impl Strategy<Value = (u32, usize, Vec<i32>, u32)> {
    (
        prop::sample::select(vec![16u32, 24]),
        1usize..=8,
        1usize..3000,
    )
        .prop_flat_map(|(bits, ch, frames)| (Just(bits), Just(ch), grid(bits, ch, frames), 0u32..9))
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 24, ..ProptestConfig::default() })]

    #[test]
    fn b5_every_depth_round_trips_exactly((bits, channels, ints, quality) in cases()) {
        let bytes = write_flac(&ints, channels, bits, quality, Metadata::new());
        let frames = ints.len() / channels;
        let mut r = open(bytes.clone()).unwrap();
        prop_assert_eq!(r.length(), frames as u64);
        let mut back = vec![0.0f32; ints.len()];
        r.read(AudioBlockMut::new(&mut back, channels, frames, frames).unwrap().into(), 0).unwrap();
        let scale = (1u32 << (bits - 1)) as f32;
        prop_assert_eq!(back, ints.iter().map(|&v| v as f32 / scale).collect::<Vec<_>>());
        // claxon decodes the same integers, interleaved.
        let mut c = claxon::FlacReader::new(Cursor::new(bytes)).unwrap();
        let decoded: Vec<i32> = c.samples().map(Result::unwrap).collect();
        let interleaved: Vec<i32> =
            (0..ints.len()).map(|k| ints[(k % channels) * frames + k / channels]).collect();
        prop_assert_eq!(decoded, interleaved);
    }

    #[test]
    fn b5_metadata_round_trips(
        values in prop::collection::vec("[a-zA-Z0-9 ]{0,12}", 1..6),
        custom in "[A-Z_]{1,8}",
    ) {
        let tags = [Tag::Title, Tag::Artist, Tag::Album, Tag::Comment, Tag::Custom(custom)];
        let mut meta = Metadata::new();
        for (tag, value) in tags.iter().zip(&values) {
            meta.push(MetadataItem::Tag(tag.clone(), value.clone()));
        }
        let bytes = write_flac(&[0; 10], 1, 16, 5, meta.clone());
        let r = open(bytes).unwrap();
        prop_assert_eq!(r.metadata(), &meta);
    }

    #[test]
    fn b5_seeks_read_what_one_long_read_does(
        reads in prop::collection::vec((-100i64..60_000, 1usize..5000), 1..12),
    ) {
        let samples = signal(50_000, 2, 16, 9);
        let (bytes, _) = encode(&samples, 2, 16, 44_100, 1024);
        let mut r = open(bytes).unwrap();
        let whole = read_f64(&mut *r, 0, 50_000).unwrap();
        for (start, n) in reads {
            let got = read_f64(&mut *r, start, n).unwrap();
            for ch in 0..2 {
                for k in 0..n {
                    let at = start + k as i64;
                    let want =
                        if (0..50_000).contains(&at) { whole[ch * 50_000 + at as usize] } else { 0.0 };
                    prop_assert_eq!(got[ch * n + k], want);
                }
            }
        }
    }

    #[test]
    fn b5_damaged_files_never_panic(
        flips in prop::collection::vec((any::<prop::sample::Index>(), 1u8..), 0..4),
        keep in any::<prop::sample::Index>(),
        start in 0i64..4000,
    ) {
        let mut meta = Metadata::new();
        meta.set_tag(Tag::Title, "t");
        let mut bytes = write_flac(&signal(4000, 2, 16, 1), 2, 16, 5, meta);
        bytes = with_blocks(&bytes, &[(3, seektable(&[(0, 0)])), (5, vec![0; 396])]);
        for (at, mask) in flips {
            let i = at.index(bytes.len());
            bytes[i] ^= mask;
        }
        bytes.truncate(keep.index(bytes.len() + 1));
        if let Ok(mut r) = open(bytes) {
            let _ = read_f64(&mut *r, start, 3000);
            let _ = read_f64(&mut *r, 0, 4000);
        }
    }
}

// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B1–B2: the format description and reading samples with random access.

mod common;

use common::*;
use syrp_audio_api::{AudioBlockMut, ChannelSet};
use syrp_format_api::AudioFormat;
use syrp_format_flac::FlacAudioFormat;

/// Interleaved integers to the reader's layout (channel after channel), divided by 2^(bits-1).
fn expected(samples: &[i32], channels: usize, bits: u32, start: usize, frames: usize) -> Vec<f64> {
    let scale = f64::from(1u32 << (bits - 1));
    let mut out = vec![0.0; frames * channels];
    for ch in 0..channels {
        for i in 0..frames {
            if let Some(&v) = samples.get((start + i) * channels + ch) {
                out[ch * frames + i] = f64::from(v) / scale;
            }
        }
    }
    out
}

#[test]
fn b1_describes_itself_as_flac() {
    let flac = FlacAudioFormat::new();
    assert_eq!(flac.name(), "FLAC file");
    assert_eq!(flac.file_extensions(), &["flac"]);
    assert!(flac.matches_extension("mix.Flac") && !flac.matches_extension("mix.wav"));
    assert!(flac.sample_rates().contains(&44_100) && flac.sample_rates().contains(&48_000));
    assert!(flac.sample_rates().windows(2).all(|w| w[0] < w[1]));
    assert_eq!(flac.bit_depths(), &[16, 24]);
}

#[test]
fn b1_can_read_needs_the_flac_marker() {
    let flac = FlacAudioFormat::new();
    assert!(flac.can_read(b"fLaC"));
    for other in [&b""[..], b"fLa", b"RIFF\0\0\0\0WAVE", b"OggS", b"FLAC"] {
        assert!(!flac.can_read(other), "{other:?}");
    }
}

#[test]
fn b1_leading_id3v2_tag_is_skipped_syrp_choice() {
    let samples = signal(5000, 1, 16, 1);
    let (plain, _) = encode(&samples, 1, 16, 44_100, 1024);
    // ID3v2.4 header, 20-byte body (syncsafe size), then the FLAC stream.
    let mut tagged = b"ID3\x04\0\0\0\0\0\x14".to_vec();
    tagged.extend_from_slice(&[0u8; 20]);
    tagged.extend_from_slice(&plain);
    let flac = FlacAudioFormat::new();
    assert!(flac.can_read(&tagged[..64]));
    // A tag followed by something else, or a malformed size, is not FLAC.
    assert!(!flac.can_read(b"ID3\x04\0\0\0\0\0\x00RIFF"));
    assert!(!flac.can_read(b"ID3\x04\0\0\0\0\x80\x00fLaC"));
    let mut r = open(tagged).unwrap();
    assert_eq!(r.length(), 5000);
    assert_eq!(
        read_f64(&mut *r, 0, 5000).unwrap(),
        expected(&samples, 1, 16, 0, 5000)
    );
}

#[test]
fn b2_spec_and_length_come_from_streaminfo() {
    let (bytes, _) = encode(&signal(10_000, 2, 24, 2), 2, 24, 96_000, 4096);
    let r = open(bytes).unwrap();
    assert_eq!(r.spec().sample_rate(), 96_000.0);
    assert_eq!(r.spec().channels(), ChannelSet::stereo());
    assert_eq!(r.spec().bits_per_sample(), 24);
    assert!(!r.spec().is_float());
    assert_eq!(r.length(), 10_000);
    assert!(r.metadata().is_empty());
}

#[test]
fn b2_every_decodable_depth_divides_by_two_to_the_bits_minus_one() {
    for bits in [8, 12, 16, 20, 24] {
        let samples = signal(3000, 3, bits, u64::from(bits));
        let (bytes, _) = encode(&samples, 3, bits, 48_000, 512);
        let mut r = open(bytes).unwrap();
        assert_eq!(r.spec().channels(), ChannelSet::discrete(3));
        let want = expected(&samples, 3, bits, 0, 3000);
        assert_eq!(
            read_f64(&mut *r, 0, 3000).unwrap(),
            want,
            "{bits}-bit as f64"
        );
        let mut f = vec![0.0f32; 9000];
        r.read(AudioBlockMut::new(&mut f, 3, 3000, 3000).unwrap().into(), 0)
            .unwrap();
        let want32: Vec<f32> = want.iter().map(|&v| v as f32).collect();
        assert_eq!(f, want32, "{bits}-bit as f32");
        assert!(
            want.contains(&-1.0),
            "the grid's lowest value reads as -1.0"
        );
    }
}

#[test]
fn b2_unknown_total_length_is_refused_syrp_choice() {
    let (mut bytes, _) = encode(&signal(2000, 1, 16, 3), 1, 16, 44_100, 1024);
    patch_streaminfo(&mut bytes, |w| w & !0xF_FFFF_FFFF);
    assert_eq!(code(open(bytes)), "SYRP-FORMAT-0003");
}

#[test]
fn b2_depths_claxon_cannot_decode_are_coded_errors() {
    let (bytes, _) = encode(&signal(2000, 1, 16, 4), 1, 16, 44_100, 1024);
    let with_bits = |bits: u64| {
        let mut b = bytes.clone();
        patch_streaminfo(&mut b, |w| (w & !(0x1F << 36)) | ((bits - 1) << 36));
        b
    };
    assert_eq!(code(open(with_bits(32))), "SYRP-FORMAT-0003");
    assert_eq!(code(open(with_bits(17))), "SYRP-FORMAT-0003");
    assert_eq!(code(open(with_bits(3))), "SYRP-FORMAT-0002");
}

#[test]
fn b2_random_access_matches_one_long_read() {
    let (frames, channels) = (200_000, 2);
    let samples = signal(frames, channels, 16, 5);
    let (bytes, _) = encode(&samples, channels, 16, 44_100, 1152);
    let mut r = open(bytes).unwrap();
    let whole = read_f64(&mut *r, 0, frames).unwrap();
    let starts = [
        150_000i64, 3, 199_990, 70_001, 70_000, 69_999, 0, 131_072, 5, -40, 199_000,
    ];
    for (i, &start) in starts.iter().enumerate() {
        let n = 1 + (i * 997) % 5000;
        let got = read_f64(&mut *r, start, n).unwrap();
        for ch in 0..channels {
            for k in 0..n {
                let at = start + k as i64;
                let want = if (0..frames as i64).contains(&at) {
                    whole[ch * frames + at as usize]
                } else {
                    0.0
                };
                assert_eq!(
                    got[ch * n + k],
                    want,
                    "start {start} channel {ch} frame {k}"
                );
            }
        }
    }
}

#[test]
fn b2_seektable_points_are_used_and_checked() {
    let samples = signal(40_000, 1, 16, 6);
    let (plain, sizes) = encode(&samples, 1, 16, 44_100, 4096);
    let mut points = Vec::new();
    let mut offset = 0;
    for (n, size) in sizes.iter().enumerate() {
        points.push((n as u64 * 4096, offset));
        offset += size;
    }
    let mut table = seektable(&points);
    table.extend_from_slice(&seektable(&[(u64::MAX, 0)])); // A placeholder.
    let mut r = open(with_blocks(&plain, &[(3, table)])).unwrap();
    assert_eq!(
        read_f64(&mut *r, 30_000, 5000).unwrap(),
        expected(&samples, 1, 16, 30_000, 5000)
    );
    assert_eq!(
        read_f64(&mut *r, 100, 50).unwrap(),
        expected(&samples, 1, 16, 100, 50)
    );

    // A point outside the stream, points out of order, and a point that names the wrong frame.
    let outside = seektable(&[(4096, 1 << 40)]);
    assert_eq!(
        code(open(with_blocks(&plain, &[(3, outside)]))),
        "SYRP-FORMAT-0002"
    );
    let unordered = seektable(&[(8192, points[2].1), (4096, points[1].1)]);
    assert_eq!(
        code(open(with_blocks(&plain, &[(3, unordered)]))),
        "SYRP-FORMAT-0002"
    );
    let lying = seektable(&[(8192, points[1].1)]);
    let mut r = open(with_blocks(&plain, &[(3, lying)])).unwrap();
    assert_eq!(code(read_f64(&mut *r, 9000, 10)), "SYRP-FORMAT-0002");
}

#[test]
fn b2_damaged_streams_are_coded_errors() {
    let (bytes, _) = encode(&signal(20_000, 2, 16, 7), 2, 16, 44_100, 4096);
    let first = frames_start(&bytes);
    let mut flipped = bytes.clone();
    flipped[first + (bytes.len() - first) / 2] ^= 0x10;
    let mut r = open(flipped).unwrap();
    assert_eq!(code(read_f64(&mut *r, 0, 20_000)), "SYRP-FORMAT-0002");
    let mut r = open(bytes[..bytes.len() - 100].to_vec()).unwrap();
    assert_eq!(code(read_f64(&mut *r, 0, 20_000)), "SYRP-FORMAT-0002");
    assert_eq!(code(open(bytes[..30].to_vec())), "SYRP-FORMAT-0002");
    assert_eq!(
        code(open(b"RIFF\0\0\0\0WAVEfmt ".to_vec())),
        "SYRP-FORMAT-0002"
    );
    let mut no_info = bytes.clone();
    no_info[4] = 0x84; // The first block claims to be a VORBIS_COMMENT.
    assert_eq!(code(open(no_info)), "SYRP-FORMAT-0002");
}

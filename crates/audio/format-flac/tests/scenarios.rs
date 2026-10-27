// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B6: the parity scenario inputs under `parity/scenarios/formats/flac/` are exactly what the
//! B4 writer produces. Run with `SYRP_BLESS=1` to regenerate them after a writer change.

mod common;

use std::path::PathBuf;

use common::*;
use syrp_audio_api::{AudioBlock, ChannelSet};
use syrp_format_api::{AudioFormat, AudioSpec, Metadata, SampleKind, Tag, WriterOptions};
use syrp_format_flac::FlacAudioFormat;

/// The scenario signal (see `scenario.schema.json`), channel after channel, as floats.
fn ramp(frames: usize, channels: usize, bits: u32) -> Vec<f32> {
    let (modulus, half) = (1i64 << bits, 1i64 << (bits - 1));
    let mut out = Vec::with_capacity(frames * channels);
    for c in 0..channels as i64 {
        for i in 0..frames as i64 {
            let v = (i * (37 + 6 * c) + c * 1000).rem_euclid(modulus) - half;
            out.push(v as f32 / half as f32);
        }
    }
    out
}

/// (file name, rate, channels, bits, frames, quality, tags)
type Input = (
    &'static str,
    u32,
    u32,
    u32,
    usize,
    u32,
    &'static [(Tag, &'static str)],
);

const INPUTS: [Input; 3] = [
    ("mono_16.flac", 44_100, 1, 16, 3000, 5, &[]),
    (
        "stereo_24_tags.flac",
        48_000,
        2,
        24,
        2000,
        8,
        &[(Tag::Title, "Ramp"), (Tag::Artist, "Syrp")],
    ),
    ("six_16_q0.flac", 96_000, 6, 16, 1200, 0, &[]),
];

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../parity/scenarios/formats/flac")
}

fn produce(input: &Input) -> Vec<u8> {
    let (_, rate, channels, bits, frames, quality, tags) = input;
    let spec = AudioSpec::new(
        f64::from(*rate),
        ChannelSet::canonical(*channels),
        *bits,
        SampleKind::Int,
    )
    .unwrap();
    let mut meta = Metadata::new();
    for (tag, value) in tags.iter() {
        meta.set_tag(tag.clone(), *value);
    }
    let options = WriterOptions::new(spec)
        .with_quality(*quality)
        .with_metadata(meta);
    let out = Shared::default();
    let mut w = FlacAudioFormat::new()
        .create_writer(Box::new(out.clone()), &options)
        .unwrap();
    let data = ramp(*frames, *channels as usize, *bits);
    w.write(
        AudioBlock::new(&data, *channels as usize, *frames, *frames)
            .unwrap()
            .into(),
    )
    .unwrap();
    w.finish().unwrap();
    out.bytes()
}

#[test]
fn b6_scenario_inputs_are_what_the_writer_produces() {
    let bless = std::env::var_os("SYRP_BLESS").is_some();
    let read = std::fs::read_to_string(dir().join("read.json")).unwrap();
    for input in &INPUTS {
        let path = dir().join("inputs").join(input.0);
        let bytes = produce(input);
        if bless {
            std::fs::write(&path, &bytes).unwrap();
        }
        assert_eq!(
            std::fs::read(&path).unwrap(),
            bytes,
            "{} is stale: rerun with SYRP_BLESS=1",
            input.0
        );
        assert!(
            read.contains(&format!("\"inputs/{}\"", input.0)),
            "read.json names {}",
            input.0
        );
        let r = open(bytes).unwrap();
        assert_eq!(
            (r.length(), r.spec().channel_count()),
            (input.4 as u64, input.2 as usize)
        );
    }
}

#[test]
fn b6_scenario_signal_matches_the_schema_formula() {
    let ramp = ramp(3, 2, 16);
    let scale = 32_768.0;
    // Channel 0: -32768 + 37 i; channel 1: 1000 + 43 i - 32768.
    let want = [
        -32_768.0, -32_731.0, -32_694.0, -31_768.0, -31_725.0, -31_682.0,
    ];
    assert_eq!(ramp, want.map(|v: f32| v / scale));
}

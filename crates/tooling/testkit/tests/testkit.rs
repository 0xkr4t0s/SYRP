// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Behaviour tests for syrp-testkit, named after the spec's B1..B6.

use syrp_audio_api::AudioBlock;
use syrp_error::Diagnostic;
use syrp_testkit::{
    NoiseRng, Tolerance, WavFormat, assert_close, assert_no_alloc, compare, peak_dbfs,
    render_offline, render_offline_no_alloc, white_noise, write_wav,
};

fn mono(samples: &[f32]) -> AudioBlock<'_, f32> {
    AudioBlock::new(samples, 1, samples.len(), samples.len()).unwrap()
}

#[global_allocator]
static ALLOC: syrp_rt::GuardedAllocator = syrp_rt::GuardedAllocator;

/// A one-pole low-pass with per-channel state carried across blocks.
fn one_pole(
    state: &mut [f32; 2],
) -> impl FnMut(&syrp_audio_api::ProcessConfig, syrp_audio_api::AudioBlockMut<'_, f32>) + '_ {
    move |_, mut block| {
        for (ch, z) in state.iter_mut().enumerate().take(block.channels()) {
            for s in block.channel_mut(ch) {
                *z += 0.1 * (*s - *z);
                *s = *z;
            }
        }
    }
}

#[test]
fn b1_render_runs_fixed_blocks_in_order_and_is_deterministic() {
    let left = white_noise(1, 1_000, 0.8).unwrap();
    let right = white_noise(2, 1_000, 0.8).unwrap();
    let input: [&[f32]; 2] = [&left, &right];

    let mut seen = Vec::new();
    let out = render_offline(
        |cfg, block| {
            assert!(cfg.is_offline());
            assert_eq!(cfg.max_block_size(), 128);
            assert_eq!(cfg.sample_rate(), 44_100.0);
            seen.push((block.channels(), block.frames(), block.channel(0)[0]));
        },
        &input,
        128,
        44_100.0,
    )
    .unwrap();
    let sizes: Vec<usize> = seen.iter().map(|s| s.1).collect();
    assert_eq!(sizes, [128, 128, 128, 128, 128, 128, 128, 104]);
    assert!(
        seen.iter()
            .enumerate()
            .all(|(i, s)| s.0 == 2 && s.2 == left[i * 128])
    );
    // An identity closure passes the input through untouched, channels in order.
    assert_eq!(out.channel(0), left.as_slice());
    assert_eq!(out.channel(1), right.as_slice());

    let (mut s1, mut s2, mut s3) = ([0.0; 2], [0.0; 2], [0.0; 2]);
    let a = render_offline(one_pole(&mut s1), &input, 64, 48_000.0).unwrap();
    let b = render_offline(one_pole(&mut s2), &input, 64, 48_000.0).unwrap();
    let c = render_offline(one_pole(&mut s3), &input, 7, 48_000.0).unwrap();
    assert_eq!(a, b, "same inputs must give identical renders");
    for ch in 0..2 {
        assert_eq!(
            a.channel(ch),
            c.channel(ch),
            "block size changed the output"
        );
    }
    assert_eq!((a.channels(), a.frames()), (2, 1_000));
}

#[test]
fn b1_render_rejects_bad_input() {
    let err = render_offline(|_, _| {}, &[&[0.0; 4], &[0.0; 5]], 2, 48e3).unwrap_err();
    assert_eq!(err.code().as_str(), "SYRP-TESTKIT-0001");
    let err = render_offline(|_, _| {}, &[&[0.0; 4]], 0, 48e3).unwrap_err();
    assert_eq!(err.code().as_str(), "SYRP-AUDIO-0004");
    let err = render_offline(|_, _| {}, &[&[0.0; 4]], 4, f64::NAN).unwrap_err();
    assert_eq!(err.code().as_str(), "SYRP-AUDIO-0003");

    let mut calls = 0;
    let out = render_offline(|_, _| calls += 1, &[], 4, 48e3).unwrap();
    assert_eq!((out.channels(), out.frames(), calls), (0, 0, 0));
}

/// Reads the fields this crate writes back out of a WAV byte stream.
struct Wav<'a> {
    tag: u16,
    channels: u16,
    rate: u32,
    bits: u16,
    data: &'a [u8],
}

fn parse(bytes: &[u8]) -> Wav<'_> {
    let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(u32_at(4) as usize, bytes.len() - 8, "RIFF size");
    assert_eq!(&bytes[8..16], b"WAVEfmt ");
    let fmt_len = u32_at(16) as usize;
    let (channels, bits) = (u16_at(22), u16_at(34));
    assert_eq!(u32_at(28), u32_at(24) * u32::from(u16_at(32)), "byte rate");
    assert_eq!(u16_at(32), channels * bits / 8, "block align");
    let mut i = 20 + fmt_len;
    if &bytes[i..i + 4] == b"fact" {
        i += 12;
    }
    assert_eq!(&bytes[i..i + 4], b"data");
    let len = u32_at(i + 4) as usize;
    Wav {
        tag: u16_at(20),
        channels,
        rate: u32_at(24),
        bits,
        data: &bytes[i + 8..i + 8 + len],
    }
}

#[test]
fn b2_wav_writer_emits_float_and_pcm() {
    let input: [&[f32]; 2] = [&[0.0, 0.5, -1.0], &[1.0, 2.0, f32::NAN]];
    let out = render_offline(|_, _| {}, &input, 2, 44_100.0).unwrap();

    let mut bytes = Vec::new();
    write_wav(&mut bytes, out.as_view(), 44_100.0, WavFormat::Float32).unwrap();
    let w = parse(&bytes);
    assert_eq!((w.tag, w.channels, w.rate, w.bits), (3, 2, 44_100, 32));
    let floats: Vec<u32> = w
        .data
        .chunks(4)
        .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
        .collect();
    let want: Vec<u32> = [0.0, 1.0, 0.5, 2.0, -1.0, f32::NAN]
        .iter()
        .map(|f: &f32| f.to_bits())
        .collect();
    assert_eq!(floats, want, "float data is interleaved and unchanged");

    bytes.clear();
    write_wav(&mut bytes, out.as_view(), 44_100.0, WavFormat::Pcm16).unwrap();
    let w = parse(&bytes);
    assert_eq!((w.tag, w.bits), (1, 16));
    let pcm: Vec<i16> = w
        .data
        .chunks(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect();
    assert_eq!(
        pcm,
        [0, 32_767, 16_384, 32_767, -32_767, 0],
        "clamped, rounded, NaN -> 0"
    );

    // 24-bit mono with an odd byte count gets a pad byte after the data chunk.
    bytes.clear();
    write_wav(&mut bytes, mono(&[0.25]), 96_000.4, WavFormat::Pcm24).unwrap();
    let w = parse(&bytes);
    assert_eq!((w.tag, w.channels, w.rate, w.bits), (1, 1, 96_000, 24));
    assert_eq!(w.data, &[0x00, 0x00, 0x20]); // round(0.25 * 8388607) = 0x200000
    assert_eq!(bytes.len(), 44 + 3 + 1);

    let path = std::env::temp_dir().join(format!("syrp-testkit-b2-{}.wav", std::process::id()));
    out.write_wav_file(&path, WavFormat::Pcm24).unwrap();
    let file = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(parse(&file).data.len(), 2 * 3 * 3);

    let err = write_wav(Vec::new(), out.as_view(), 0.2, WavFormat::Pcm16).unwrap_err();
    assert_eq!(err.code().as_str(), "SYRP-TESTKIT-0004");
}

#[test]
fn b3_comparators_measure_errors() {
    let c = compare(&[0.0_f32, 0.5, 1.0, 0.0], &[0.0_f32, 0.5, 0.5, 0.0]).unwrap();
    assert_eq!(c.max_abs_error(), 0.5);
    assert_eq!(c.max_abs_error_at(), Some((0, 2)));
    assert_eq!(c.rms_error(), 0.25); // sqrt(0.25 / 4)
    assert!((c.rms_error_dbfs() - 20.0 * 0.25_f64.log10()).abs() < 1e-12);
    assert_eq!(c.peak_dbfs_actual(), 0.0);
    assert!((c.peak_dbfs_difference() - 6.020_599_913).abs() < 1e-6);
    assert_eq!(peak_dbfs(&[0.0_f32; 3]), f64::NEG_INFINITY);

    let nan = compare(&[f32::NAN], &[0.0_f32]).unwrap();
    assert_eq!(nan.max_abs_error(), f64::INFINITY);
    // Identical NaNs are bit-exact but an infinite numeric error: a NaN golden can't lock in NaN.
    let nan_nan = compare(&[0.0_f32, f32::NAN], &[0.0_f32, f32::NAN]).unwrap();
    assert!(nan_nan.is_bit_exact());
    assert_eq!(nan_nan.max_abs_error(), f64::INFINITY);
    assert_eq!(nan_nan.max_abs_error_at(), Some((0, 1)));
    assert!(nan_nan.judge(Tolerance::exact()).passed());
    assert!(!nan_nan.judge(Tolerance::dsp_default()).passed());
    assert!(!nan_nan.judge(Tolerance::dsp_loose()).passed());

    let err = compare(&[0.0_f32; 2], &[0.0_f32; 3]).unwrap_err();
    assert_eq!(err.code().as_str(), "SYRP-TESTKIT-0005");
}

#[test]
fn b3_tolerance_profiles_judge_and_assert_close_panics() {
    let base = white_noise(11, 4_800, 0.5).unwrap();
    let shifted = |d: f32| -> Vec<f32> { base.iter().map(|s| s + d).collect() };
    let judge = |other: &[f32], t: Tolerance| compare(&base, other).unwrap().judge(t).passed();

    assert!(judge(&base, Tolerance::exact()));
    let signed_zero = compare(&[0.0_f32], &[-0.0_f32]).unwrap();
    assert!(!signed_zero.judge(Tolerance::exact()).passed());
    assert!(signed_zero.judge(Tolerance::dsp_default()).passed());
    assert!(!judge(&shifted(1e-3), Tolerance::exact()));
    assert!(judge(&shifted(5e-7), Tolerance::dsp_default()));
    assert!(!judge(&shifted(5e-6), Tolerance::dsp_default()));
    // -96 dBFS is about 1.585e-5 RMS.
    assert!(judge(&shifted(1.0e-5), Tolerance::dsp_loose()));
    assert!(!judge(&shifted(3.0e-5), Tolerance::dsp_loose()));
    assert!(!judge(&shifted(3.0e-5), Tolerance::max_abs(1e-5)));
    assert!(judge(&shifted(3.0e-5), Tolerance::rms_dbfs(-80.0)));
    for name in ["exact", "dsp-default", "dsp-loose"] {
        assert_eq!(Tolerance::by_name(name).unwrap().name(), name);
    }

    let v = assert_close(&base, &shifted(1e-7), Tolerance::dsp_default());
    assert!(v.passed() && v.tolerance() == Tolerance::dsp_default());
    let msg = std::panic::catch_unwind(|| {
        assert_close(&[0.0_f32, 0.5], &[0.0_f32, 0.25], Tolerance::dsp_default());
    })
    .unwrap_err();
    let msg = msg.downcast_ref::<String>().unwrap();
    assert!(
        msg.contains("FAIL under `dsp-default`") && msg.contains("channel 0 frame 1"),
        "{msg}"
    );
    let shape = std::panic::catch_unwind(|| {
        assert_close(&[0.0_f32], &[0.0_f32; 2], Tolerance::exact());
    });
    assert!(shape.is_err());
}

#[test]
fn b4_verdict_serialises_to_the_documented_shape() {
    let v = compare(&[0.5_f32, 0.0], &[0.5_f32, 0.25])
        .unwrap()
        .judge(Tolerance::dsp_loose());
    let json = serde_json::to_value(v).unwrap();
    let want = serde_json::json!({
        "schema": "syrp.testkit.verdict/v1",
        "passed": false,
        "tolerance": { "name": "dsp-loose", "criterion": "rms_dbfs", "limit": -96.0 },
        "comparison": {
            "channels": 1, "frames": 2, "bit_exact": false,
            "max_abs_error": 0.25, "max_abs_error_at": { "channel": 0, "frame": 1 },
            "rms_error": (0.0625_f64 / 2.0).sqrt(),
            "rms_error_dbfs": 20.0 * (0.0625_f64 / 2.0).sqrt().log10(),
            "peak_dbfs_actual": 20.0 * 0.5_f64.log10(),
            "peak_dbfs_expected": 20.0 * 0.5_f64.log10(),
            "peak_dbfs_difference": 0.0
        }
    });
    assert_eq!(json, want);

    let silent = compare(&[0.0_f32], &[0.0_f32])
        .unwrap()
        .judge(Tolerance::exact());
    let json = serde_json::to_value(silent).unwrap();
    assert_eq!(json["passed"], true);
    assert_eq!(json["tolerance"]["limit"], serde_json::Value::Null);
    assert_eq!(json["tolerance"]["criterion"], "bit_exact");
    let c = &json["comparison"];
    for key in [
        "max_abs_error_at",
        "rms_error_dbfs",
        "peak_dbfs_actual",
        "peak_dbfs_expected",
    ] {
        assert_eq!(c[key], serde_json::Value::Null, "{key}");
    }
    assert_eq!(c["peak_dbfs_difference"], 0.0);
}

#[test]
fn b5_seeded_rng_is_reproducible() {
    // Known SplitMix64 outputs for seed 0 pin the sequence across platforms and releases.
    let mut rng = NoiseRng::new(0);
    let first: Vec<u64> = (0..3).map(|_| rng.next_u64()).collect();
    assert_eq!(
        first,
        [
            0xe220_a839_7b1d_cdaf,
            0x6e78_9e6a_a1b9_65f4,
            0x06c4_5d18_8009_454f
        ]
    );

    let a = white_noise(1234, 48_000, 0.5).unwrap();
    assert_eq!(a, white_noise(1234, 48_000, 0.5).unwrap());
    assert_ne!(a, white_noise(1235, 48_000, 0.5).unwrap());
    assert!(a.iter().all(|s| (-0.5..0.5).contains(s)));
    let mean = a.iter().map(|&s| f64::from(s)).sum::<f64>() / a.len() as f64;
    assert!(mean.abs() < 0.01, "mean {mean}");
    let mut buf = [0.0_f32; 4];
    NoiseRng::new(1234).fill_noise(&mut buf, 0.5);
    assert_eq!(buf, a[..4]);
}

#[test]
fn b6_no_alloc_render_runs_rt_safe_closures() {
    let input = white_noise(3, 512, 1.0).unwrap();
    let mut state = [0.0; 2];
    let guarded = render_offline_no_alloc(one_pole(&mut state), &[&input], 32, 48e3).unwrap();
    let mut state = [0.0; 2];
    let plain = render_offline(one_pole(&mut state), &[&input], 32, 48e3).unwrap();
    assert_eq!(guarded, plain);
    let sum = assert_no_alloc(|| input.iter().sum::<f32>());
    assert_eq!(sum, input.iter().sum::<f32>());
}

/// Child half of the next test: only does work when spawned by it.
#[test]
#[ignore = "spawned as a subprocess by b6_allocation_inside_guarded_render_aborts"]
fn b6_child_allocates_in_guarded_render() {
    if std::env::var_os("SYRP_TESTKIT_B6_CHILD").is_none() {
        return;
    }
    let _ = render_offline_no_alloc(
        |_, _| {
            std::hint::black_box(vec![0.0_f32; 16]);
        },
        &[&[0.0; 8]],
        4,
        48e3,
    );
}

#[test]
fn b6_allocation_inside_guarded_render_aborts() {
    if !cfg!(debug_assertions) {
        return; // the guard is a debug-build check
    }
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "b6_child_allocates_in_guarded_render",
            "--ignored",
            "--nocapture",
        ])
        .env("SYRP_TESTKIT_B6_CHILD", "1")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("SYRP-RT-0001"));
}

#[test]
fn every_error_code_is_catalogued() {
    let table: toml::Table = include_str!("../../../../errors/testkit.toml")
        .parse()
        .unwrap();
    let codes: Vec<&str> = table["error"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["code"].as_str().unwrap())
        .collect();
    let mut bytes = Vec::new();
    let ragged = render_offline(|_, _| {}, &[&[0.0], &[]], 1, 48e3).unwrap_err();
    let limit = write_wav(&mut bytes, mono(&[0.0]), 0.0, WavFormat::Pcm16).unwrap_err();
    let io = syrp_testkit::TestkitError::from(std::io::Error::other("x"));
    let shape = compare(&[0.0_f32], &[0.0_f32; 2]).unwrap_err();
    let emitted: Vec<&str> = [ragged, limit, io, shape]
        .iter()
        .map(|e| e.code().as_str())
        .collect();
    assert_eq!(
        emitted,
        [
            "SYRP-TESTKIT-0001",
            "SYRP-TESTKIT-0004",
            "SYRP-TESTKIT-0003",
            "SYRP-TESTKIT-0005"
        ]
    );
    // 0002 (allocation failure) cannot be provoked portably; check it is catalogued too.
    assert_eq!(
        codes,
        [
            "SYRP-TESTKIT-0001",
            "SYRP-TESTKIT-0002",
            "SYRP-TESTKIT-0003",
            "SYRP-TESTKIT-0004",
            "SYRP-TESTKIT-0005"
        ]
    );
}

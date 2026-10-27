// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B1 (the format description) and B2 (reading samples and headers).

mod common;

use std::io::Cursor;

use common::{chunk, fmt, fmt_ext, open, read_all_f32, read_all_f64, riff};
use syrp_audio_api::{AudioBlockMut, ChannelSet};
use syrp_error::Diagnostic;
use syrp_format_api::{AudioFormat, AudioSpec, SampleKind, WriterOptions};
use syrp_format_wav::{WavAudioFormat, WavError, WavReader};

fn code_of(bytes: Vec<u8>) -> String {
    WavReader::open(Box::new(Cursor::new(bytes)))
        .unwrap_err()
        .code()
        .as_str()
        .to_owned()
}

fn le16(v: &[i16]) -> Vec<u8> {
    v.iter().flat_map(|s| s.to_le_bytes()).collect()
}

#[test]
fn b1_name_extensions_rates_and_depths() {
    let wav = WavAudioFormat::new();
    assert_eq!(wav.name(), "WAV file");
    assert_eq!(wav.file_extensions(), ["wav", "bwf"]);
    assert!(wav.matches_extension("/a/b/take.Wav"));
    assert!(wav.sample_rates().is_empty());
    assert_eq!(wav.bit_depths(), [8, 16, 24, 32]);
    assert_eq!(WavAudioFormat::default(), wav);
}

#[test]
fn b1_can_read_riff_rf64_bw64_with_wave_form_only() {
    let wav = WavAudioFormat::new();
    for magic in [b"RIFF", b"RF64", b"BW64"] {
        let mut h = magic.to_vec();
        h.extend_from_slice(b"\0\0\0\0WAVEfmt ");
        assert!(wav.can_read(&h), "{magic:?}");
    }
    assert!(!wav.can_read(b""));
    assert!(!wav.can_read(b"RIFF\0\0\0\0WAV"));
    assert!(!wav.can_read(b"RIFF\0\0\0\0AVI "));
    assert!(!wav.can_read(b"FORM\0\0\0\0AIFF"));
}

#[test]
fn b1_rf64_threshold_defaults_to_the_riff_limit_and_can_be_lowered() {
    let wav = WavAudioFormat::new();
    assert_eq!(wav.rf64_threshold(), u64::from(u32::MAX));
    assert_eq!(wav.with_rf64_threshold(100).rf64_threshold(), 100);
    assert_eq!(
        wav.with_rf64_threshold(u64::MAX).rf64_threshold(),
        u64::from(u32::MAX)
    );
}

#[test]
fn b1_create_reader_returns_a_boxed_reader_and_create_writer_checks_the_spec() {
    let file = riff(&[
        chunk(b"fmt ", &fmt(1, 1, 8000, 16)),
        chunk(b"data", &le16(&[1])),
    ]);
    let r = open(file).expect("valid");
    assert_eq!(r.length(), 1);
    let spec = AudioSpec::new(48_000.0, ChannelSet::mono(), 64, SampleKind::Float).unwrap();
    let out = Box::new(Cursor::new(Vec::new()));
    let err = WavAudioFormat::new()
        .create_writer(out, &WriterOptions::new(spec))
        .err()
        .expect("64-bit float is not a writable depth");
    assert_eq!(err.code().as_str(), "SYRP-FORMAT-0003");
}

#[test]
fn b2_reads_8_bit_unsigned_with_offset_128() {
    let file = riff(&[
        chunk(b"fmt ", &fmt(1, 1, 8000, 8)),
        chunk(b"data", &[0, 64, 128, 255]),
    ]);
    let mut r = open(file).unwrap();
    assert_eq!(r.spec().bits_per_sample(), 8);
    assert_eq!(
        read_all_f64(r.as_mut())[0],
        [-1.0, -0.5, 0.0, 127.0 / 128.0]
    );
}

#[test]
fn b2_reads_16_24_and_32_bit_signed_pcm_exactly() {
    let f16 = riff(&[
        chunk(b"fmt ", &fmt(1, 1, 44_100, 16)),
        chunk(b"data", &le16(&[i16::MIN, -1, 0, i16::MAX])),
    ]);
    assert_eq!(
        read_all_f64(open(f16).unwrap().as_mut())[0],
        [-1.0, -1.0 / 32768.0, 0.0, 32767.0 / 32768.0]
    );
    let d24: Vec<u8> = [-8_388_608i32, -1, 1, 8_388_607]
        .iter()
        .flat_map(|v| v.to_le_bytes()[..3].to_vec())
        .collect();
    let f24 = riff(&[chunk(b"fmt ", &fmt(1, 1, 48_000, 24)), chunk(b"data", &d24)]);
    let s = 8_388_608.0;
    assert_eq!(
        read_all_f64(open(f24).unwrap().as_mut())[0],
        [-1.0, -1.0 / s, 1.0 / s, 8_388_607.0 / s]
    );
    let d32: Vec<u8> = [i32::MIN, 1, i32::MAX]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    let f32_file = riff(&[chunk(b"fmt ", &fmt(1, 1, 48_000, 32)), chunk(b"data", &d32)]);
    let s = 2_147_483_648.0;
    assert_eq!(
        read_all_f64(open(f32_file).unwrap().as_mut())[0],
        [-1.0, 1.0 / s, 2_147_483_647.0 / s]
    );
}

#[test]
fn b2_reads_32_and_64_bit_float_into_both_sample_types() {
    let d: Vec<u8> = [0.25f32, -2.0]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    let mut r = open(riff(&[
        chunk(b"fmt ", &fmt(3, 1, 96_000, 32)),
        chunk(b"data", &d),
    ]))
    .unwrap();
    assert!(r.spec().is_float());
    assert_eq!(read_all_f32(r.as_mut())[0], [0.25, -2.0]);
    let d: Vec<u8> = [0.1f64, -0.3]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    let mut r = open(riff(&[
        chunk(b"fmt ", &fmt(3, 1, 96_000, 64)),
        chunk(b"data", &d),
    ]))
    .unwrap();
    assert_eq!(r.spec().bits_per_sample(), 64);
    assert_eq!(read_all_f64(r.as_mut())[0], [0.1, -0.3]);
}

#[test]
fn b2_extensible_pcm_and_float_subformats() {
    let pcm = riff(&[
        chunk(b"fmt ", &fmt_ext(2, 48_000, 16, 16, 0x3, 1)),
        chunk(b"data", &le16(&[16384, -16384])),
    ]);
    let mut r = open(pcm).unwrap();
    assert_eq!(r.spec().channels(), ChannelSet::stereo());
    assert_eq!(read_all_f64(r.as_mut()), [[0.5], [-0.5]]);
    let d: Vec<u8> = 0.75f32.to_le_bytes().to_vec();
    let float = riff(&[
        chunk(b"fmt ", &fmt_ext(1, 48_000, 32, 32, 0x4, 3)),
        chunk(b"data", &d),
    ]);
    let mut r = open(float).unwrap();
    assert!(r.spec().is_float());
    assert_eq!(r.spec().channels(), ChannelSet::mono());
    assert_eq!(read_all_f32(r.as_mut())[0], [0.75]);
}

#[test]
fn b2_valid_bits_narrower_than_the_container_are_honoured() {
    // 20 valid bits in a 24-bit container: the low 4 bits are padding and ignored.
    let raw: i32 = (0x7FFFF << 4) | 0xF;
    let d: Vec<u8> = raw.to_le_bytes()[..3].to_vec();
    let mut r = open(riff(&[
        chunk(b"fmt ", &fmt_ext(1, 48_000, 24, 20, 0x4, 1)),
        chunk(b"data", &d),
    ]))
    .unwrap();
    assert_eq!(r.spec().bits_per_sample(), 20);
    assert_eq!(
        read_all_f64(r.as_mut())[0],
        [f64::from(0x7FFFF) / f64::from(1 << 19)]
    );
}

#[test]
fn b2_valid_bits_zero_means_the_whole_container_syrp_choice() {
    let r = open(riff(&[
        chunk(b"fmt ", &fmt_ext(1, 48_000, 24, 0, 0x4, 1)),
        chunk(b"data", &[0; 3]),
    ]))
    .unwrap();
    assert_eq!(r.spec().bits_per_sample(), 24);
}

#[test]
fn b2_odd_sized_and_unknown_chunks_are_skipped_with_their_pad_byte() {
    let file = riff(&[
        chunk(b"zzzz", &[1, 2, 3]),
        chunk(b"fmt ", &fmt(1, 1, 8000, 16)),
        chunk(b"odd!", &[9]),
        chunk(b"data", &le16(&[8192])),
        chunk(b"tail", &[7; 5]),
    ]);
    let mut r = open(file).unwrap();
    assert_eq!(read_all_f64(r.as_mut())[0], [0.25]);
}

#[test]
fn b2_rf64_and_bw64_take_sizes_from_ds64() {
    for magic in [b"RF64", b"BW64"] {
        let samples = le16(&[100, 200, 300]);
        let mut ds64 = Vec::new();
        ds64.extend_from_slice(&0u64.to_le_bytes()); // RIFF size, patched below
        ds64.extend_from_slice(&(samples.len() as u64).to_le_bytes());
        ds64.extend_from_slice(&3u64.to_le_bytes());
        ds64.extend_from_slice(&0u32.to_le_bytes());
        let mut data = b"data".to_vec();
        data.extend_from_slice(&u32::MAX.to_le_bytes());
        data.extend_from_slice(&samples);
        let mut file = riff(&[
            chunk(b"ds64", &ds64),
            chunk(b"fmt ", &fmt(1, 1, 8000, 16)),
            data,
        ]);
        file[..4].copy_from_slice(magic);
        file[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        let riff_size = file.len() as u64 - 8;
        file[20..28].copy_from_slice(&riff_size.to_le_bytes());
        let mut r = open(file).unwrap();
        assert_eq!(r.length(), 3);
        let s = 32768.0;
        assert_eq!(
            read_all_f64(r.as_mut())[0],
            [100.0 / s, 200.0 / s, 300.0 / s]
        );
    }
}

#[test]
fn b2_channel_mask_maps_to_a_named_layout_in_canonical_order() {
    // 5.1 with side speakers (Microsoft 0x60F): file order L R C LFE SL SR.
    let d = le16(&[1, 2, 3, 4, 5, 6]);
    let mut r = open(riff(&[
        chunk(b"fmt ", &fmt_ext(6, 48_000, 16, 16, 0x60F, 1)),
        chunk(b"data", &d),
    ]))
    .unwrap();
    assert_eq!(r.spec().channels(), ChannelSet::surround_5_1());
    let got: Vec<f64> = read_all_f64(r.as_mut())
        .into_iter()
        .map(|c| c[0] * 32768.0)
        .collect();
    assert_eq!(got, [1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    // 7.1 (0x63F): file L R C LFE BL BR SL SR; canonical puts the side pair before the rear.
    let d = le16(&[1, 2, 3, 4, 5, 6, 7, 8]);
    let mut r = open(riff(&[
        chunk(b"fmt ", &fmt_ext(8, 48_000, 16, 16, 0x63F, 1)),
        chunk(b"data", &d),
    ]))
    .unwrap();
    assert_eq!(r.spec().channels(), ChannelSet::surround_7_1());
    let got: Vec<f64> = read_all_f64(r.as_mut())
        .into_iter()
        .map(|c| c[0] * 32768.0)
        .collect();
    assert_eq!(got, [1.0, 2.0, 3.0, 4.0, 7.0, 8.0, 5.0, 6.0]);
}

#[test]
fn b2_mask_zero_mismatched_or_inexpressible_gives_discrete() {
    for (mask, ch) in [(0, 2), (0x7, 2), (0xC0, 2), (0x8000_0003, 2)] {
        let file = riff(&[
            chunk(b"fmt ", &fmt_ext(ch, 8000, 16, 16, mask, 1)),
            chunk(b"data", &[0; 4]),
        ]);
        assert_eq!(
            open(file).unwrap().spec().channels(),
            ChannelSet::discrete(2),
            "{mask:#x}"
        );
    }
}

#[test]
fn b2_plain_header_is_mono_stereo_or_discrete_syrp_choice() {
    for (ch, set) in [
        (1, ChannelSet::mono()),
        (2, ChannelSet::stereo()),
        (3, ChannelSet::discrete(3)),
    ] {
        let file = riff(&[chunk(b"fmt ", &fmt(1, ch, 8000, 16)), chunk(b"data", &[])]);
        assert_eq!(open(file).unwrap().spec().channels(), set);
    }
}

#[test]
fn b2_read_zero_fills_outside_the_file_and_honours_channel_subsets() {
    let file = riff(&[
        chunk(b"fmt ", &fmt(1, 2, 8000, 16)),
        chunk(b"data", &le16(&[8192, -8192, 16384, -16384])),
    ]);
    let mut r = open(file).unwrap();
    let mut buf = [9.0f32; 4];
    r.read(AudioBlockMut::new(&mut buf, 1, 4, 4).unwrap().into(), -1)
        .unwrap();
    assert_eq!(buf, [0.0, 0.25, 0.5, 0.0]);
}

#[test]
fn b2_streamed_data_size_runs_to_the_end_of_the_input_syrp_choice() {
    for (riff_size, data_size) in [(u32::MAX, u32::MAX), (0, 0), (100, u32::MAX)] {
        let mut file = riff(&[
            chunk(b"fmt ", &fmt(1, 1, 8000, 16)),
            chunk(b"data", &le16(&[1, 2, 3])),
        ]);
        file[4..8].copy_from_slice(&riff_size.to_le_bytes());
        let at = file.len() - 10;
        file[at..at + 4].copy_from_slice(&data_size.to_le_bytes());
        assert_eq!(open(file).unwrap().length(), 3, "{riff_size} {data_size}");
    }
}

#[test]
fn b2_data_past_the_end_is_cut_to_whole_frames_present_syrp_choice() {
    let mut file = riff(&[
        chunk(b"fmt ", &fmt(1, 2, 8000, 16)),
        chunk(b"data", &le16(&[1, 2, 3, 4, 5])),
    ]);
    let at = file.len() - 14;
    file[at..at + 4].copy_from_slice(&1000u32.to_le_bytes());
    // 10 bytes present, 4 per frame: two whole frames.
    assert_eq!(open(file).unwrap().length(), 2);
}

#[test]
fn b2_missing_fmt_or_data_is_0008_syrp_choice() {
    assert_eq!(
        code_of(riff(&[chunk(b"data", &[0; 4])])),
        "SYRP-FORMAT-0008"
    );
    assert_eq!(
        code_of(riff(&[chunk(b"fmt ", &fmt(1, 1, 8000, 16))])),
        "SYRP-FORMAT-0008"
    );
}

#[test]
fn b2_malformed_headers_have_their_codes() {
    assert_eq!(code_of(b"RIFF\0\0\0\0WAV".to_vec()), "SYRP-FORMAT-0006");
    assert_eq!(code_of(b"RIFX\x04\0\0\0WAVE".to_vec()), "SYRP-FORMAT-0006");
    let mut short_fmt = riff(&[chunk(b"fmt ", &fmt(1, 1, 8000, 16))]);
    short_fmt.extend_from_slice(b"data\0\0\0\0");
    short_fmt.truncate(30);
    assert_eq!(code_of(short_fmt), "SYRP-FORMAT-0008");
    let mut overrun = riff(&[chunk(b"data", &[]), chunk(b"fmt ", &fmt(1, 1, 8000, 16))]);
    overrun.truncate(overrun.len() - 4);
    assert_eq!(code_of(overrun), "SYRP-FORMAT-0007");
    let bad = |f: Vec<u8>| code_of(riff(&[chunk(b"fmt ", &f), chunk(b"data", &[])]));
    assert_eq!(bad(fmt(1, 0, 8000, 16)), "SYRP-FORMAT-0009");
    assert_eq!(bad(fmt(1, 1, 0, 16)), "SYRP-FORMAT-0009");
    assert_eq!(bad(fmt(1, 1, 8000, 0)), "SYRP-FORMAT-0009");
    assert_eq!(bad(fmt(1, 1, 8000, 40)), "SYRP-FORMAT-0009");
    assert_eq!(bad(fmt(3, 1, 8000, 24)), "SYRP-FORMAT-0009");
    assert_eq!(bad(fmt(1, 1, 8000, 16)[..14].to_vec()), "SYRP-FORMAT-0009");
    assert_eq!(bad(fmt_ext(1, 8000, 16, 17, 4, 1)), "SYRP-FORMAT-0009");
    assert_eq!(
        bad(fmt_ext(1, 8000, 16, 16, 4, 1)[..24].to_vec()),
        "SYRP-FORMAT-0009"
    );
    let mut misaligned = fmt(1, 2, 8000, 16);
    misaligned[12] = 3;
    assert_eq!(bad(misaligned), "SYRP-FORMAT-0009");
    assert_eq!(bad(fmt(2, 1, 8000, 4)), "SYRP-FORMAT-0010");
    let mut odd_guid = fmt_ext(1, 8000, 16, 16, 4, 1);
    odd_guid[39] = 0;
    assert_eq!(bad(odd_guid), "SYRP-FORMAT-0010");
}

#[test]
fn b2_rf64_without_a_valid_ds64_is_0012() {
    let mut no_ds64 = riff(&[chunk(b"fmt ", &fmt(1, 1, 8000, 16)), chunk(b"data", &[])]);
    no_ds64[..4].copy_from_slice(b"RF64");
    assert_eq!(code_of(no_ds64), "SYRP-FORMAT-0012");
    let mut ds64 = vec![0u8; 28];
    ds64[24..28].copy_from_slice(&1000u32.to_le_bytes()); // table larger than the chunk
    let mut file = riff(&[
        chunk(b"ds64", &ds64),
        chunk(b"fmt ", &fmt(1, 1, 8000, 16)),
        chunk(b"data", &[]),
    ]);
    file[..4].copy_from_slice(b"BW64");
    assert_eq!(code_of(file), "SYRP-FORMAT-0012");
}

#[test]
fn b2_trait_errors_keep_the_specific_code_in_the_reason() {
    let err = open(b"RIFF\x04\0\0\0WAVE".to_vec())
        .err()
        .expect("no chunks");
    assert_eq!(err.code().as_str(), "SYRP-FORMAT-0002");
    assert!(err.to_string().contains("SYRP-FORMAT-0008"), "{err}");
    let err = open(riff(&[
        chunk(b"fmt ", &fmt(2, 1, 8000, 4)),
        chunk(b"data", &[]),
    ]))
    .err()
    .unwrap();
    assert_eq!(err.code().as_str(), "SYRP-FORMAT-0003");
    assert!(matches!(
        WavReader::open(Box::new(Cursor::new(Vec::new()))),
        Err(WavError::NotWave)
    ));
}

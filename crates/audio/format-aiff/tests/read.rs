// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B2: reading the FORM, COMM and SSND.

mod common;

use common::{chunk, comm, comm_c, form, open, open_code, read_all, ssnd};
use syrp_audio_api::AudioBlockMut;
use syrp_format_api::{MetadataItem, SampleKind};

fn mono16(frames: u32, data: &[u8]) -> Vec<u8> {
    form(
        b"AIFF",
        &[
            chunk(b"COMM", &comm(1, frames, 16, 44_100.0)),
            chunk(b"SSND", &ssnd(0, 0, data)),
        ],
    )
}

#[test]
fn b2_reads_plain_aiff_integer_depths() {
    let cases: [(i16, &[u8], [f64; 2]); 4] = [
        (8, &[0x80, 0x40], [-1.0, 0.5]),
        (16, &[0x40, 0x00, 0xC0, 0x00], [0.5, -0.5]),
        (
            24,
            &[0x7F, 0xFF, 0xFF, 0x00, 0x00, 0x01],
            [8_388_607.0 / 8_388_608.0, 1.0 / 8_388_608.0],
        ),
        (32, &[0x80, 0, 0, 0, 0x20, 0, 0, 0], [-1.0, 0.25]),
    ];
    for (bits, data, want) in cases {
        let file = form(
            b"AIFF",
            &[
                chunk(b"COMM", &comm(1, 2, bits, 48_000.0)),
                chunk(b"SSND", &ssnd(0, 0, data)),
            ],
        );
        let mut r = open(file).unwrap();
        assert_eq!(r.spec().bits_per_sample(), u32::try_from(bits).unwrap());
        assert_eq!(r.spec().sample_rate(), 48_000.0);
        assert_eq!(read_all(&mut *r), want, "{bits}-bit");
    }
}

#[test]
fn b2_reads_aifc_compression_types() {
    let stereo = |kind: &[u8; 4], bits: i16, data: Vec<u8>| {
        let file = form(
            b"AIFC",
            &[
                chunk(b"FVER", &0xA280_5140u32.to_be_bytes()),
                chunk(b"COMM", &comm_c(2, 2, bits, 96_000.0, kind)),
                chunk(b"SSND", &ssnd(0, 0, &data)),
            ],
        );
        let mut r = open(file).unwrap();
        assert!(r.metadata().is_empty(), "FVER is not metadata");
        (r.spec().sample_kind(), read_all(&mut *r))
    };
    // Interleaved L R L R; read back channel-major.
    let none = [0x40, 0, 0xC0, 0, 0x20, 0, 0, 0].to_vec();
    assert_eq!(stereo(b"NONE", 16, none).1, [0.5, 0.25, -0.5, 0.0]);
    let sowt = [0, 0x40, 0, 0xC0, 0, 0x20, 0, 0].to_vec();
    assert_eq!(stereo(b"sowt", 16, sowt).1, [0.5, 0.25, -0.5, 0.0]);
    let f32s: Vec<u8> = [0.5f32, -2.0, 0.125, 1e-3]
        .iter()
        .flat_map(|v| v.to_be_bytes())
        .collect();
    for kind in [b"fl32", b"FL32"] {
        let (k, v) = stereo(kind, 32, f32s.clone());
        assert_eq!(k, SampleKind::Float);
        assert_eq!(v, [0.5, 0.125, -2.0, f64::from(1e-3f32)]);
    }
    let f64s: Vec<u8> = [0.1f64, -0.2, 0.3, 4.0]
        .iter()
        .flat_map(|v| v.to_be_bytes())
        .collect();
    for kind in [b"fl64", b"FL64"] {
        assert_eq!(stereo(kind, 64, f64s.clone()).1, [0.1, 0.3, -0.2, 4.0]);
    }
}

#[test]
fn b2_other_compression_names_the_four_character_code() {
    let file = form(
        b"AIFC",
        &[
            chunk(b"COMM", &comm_c(1, 0, 16, 8_000.0, b"ima4")),
            chunk(b"SSND", &ssnd(0, 0, &[])),
        ],
    );
    let err = open(file).err().unwrap();
    assert!(err.to_string().contains("'ima4'"), "{err}");
    let float_size = form(
        b"AIFC",
        &[chunk(b"COMM", &comm_c(1, 0, 16, 8_000.0, b"fl32"))],
    );
    assert_eq!(open_code(float_size), "SYRP-FORMAT-0002");
}

#[test]
fn b2_honours_ssnd_offset_block_size_odd_chunks_and_unknown_chunks() {
    let file = form(
        b"AIFF",
        &[
            chunk(b"zzzz", &[1, 2, 3]),
            chunk(b"COMM", &comm(1, 2, 8, 22_050.0)),
            chunk(b"SSND", &ssnd(5, 4096, &[0x40, 0xC0, 0x7F])),
        ],
    );
    let mut r = open(file).unwrap();
    // Three data bytes after a 5-byte offset; COMM's two frames win.
    assert_eq!(r.length(), 2);
    let unknown = MetadataItem::Other("zzzz".into(), vec![1, 2, 3]);
    assert_eq!(r.metadata().items(), [unknown]);
    assert_eq!(read_all(&mut *r), [0.5, -0.5]);
}

#[test]
fn b2_reads_into_f32_and_zero_fills_outside_the_file() {
    let mut r = open(mono16(2, &[0x40, 0, 0x20, 0])).unwrap();
    let mut out = [9.0f32; 4];
    r.read(AudioBlockMut::new(&mut out, 1, 4, 4).unwrap().into(), -1)
        .unwrap();
    assert_eq!(out, [0.0, 0.5, 0.25, 0.0]);
}

#[test]
fn b2_long_reads_cross_the_scratch_buffer() {
    let frames = 20_000u32;
    let data: Vec<u8> = (0..frames).flat_map(|i| (i as i16).to_be_bytes()).collect();
    let mut r = open(mono16(frames, &data)).unwrap();
    let all = read_all(&mut *r);
    assert_eq!(all.len(), 20_000);
    assert_eq!(all[19_999], f64::from(19_999i16) / 32_768.0);
}

#[test]
fn b2_frame_count_mismatch_uses_the_smaller_syrp_choice() {
    // COMM says 4 frames, SSND holds 2.
    assert_eq!(open(mono16(4, &[0, 1, 0, 2])).unwrap().length(), 2);
    // COMM says 1 frame, SSND holds 2.
    assert_eq!(open(mono16(1, &[0, 1, 0, 2])).unwrap().length(), 1);
}

#[test]
fn b2_truncated_ssnd_is_cut_to_the_bytes_present_syrp_choice() {
    let mut file = mono16(3, &[0x40, 0, 0x20, 0, 0x10, 0]);
    file.truncate(file.len() - 3);
    let mut r = open(file).unwrap();
    assert_eq!(r.length(), 1);
    assert_eq!(read_all(&mut *r), [0.5]);
    // Bytes after the FORM are ignored.
    let mut longer = mono16(1, &[0x40, 0]);
    longer.extend_from_slice(b"junk after the form");
    assert_eq!(open(longer).unwrap().length(), 1);
}

#[test]
fn b2_malformed_headers_are_coded_errors() {
    let bad = "SYRP-FORMAT-0002";
    let comm_ok = chunk(b"COMM", &comm(1, 0, 16, 44_100.0));
    assert_eq!(open_code(b"FORM".to_vec()), bad);
    assert_eq!(open_code(form(b"WAVE", &[])), bad);
    assert_eq!(open_code(b"FORM\0\0\0\x02AIFF".to_vec()), bad);
    assert_eq!(open_code(form(b"AIFF", &[])), bad, "no COMM");
    assert_eq!(
        open_code(form(b"AIFF", &[comm_ok.clone(), comm_ok.clone()])),
        bad
    );
    assert_eq!(
        open_code(form(b"AIFF", std::slice::from_ref(&comm_ok))),
        "ok",
        "no frames, no SSND"
    );
    let comm_frames = chunk(b"COMM", &comm(1, 5, 16, 44_100.0));
    assert_eq!(
        open_code(form(b"AIFF", &[comm_frames])),
        bad,
        "frames but no SSND"
    );
    assert_eq!(open_code(form(b"AIFF", &[chunk(b"COMM", &[0; 17])])), bad);
    for (ch, bits) in [(0, 16), (-2, 16), (1, 0), (1, 33)] {
        let c = chunk(b"COMM", &comm(ch, 0, bits, 44_100.0));
        assert_eq!(open_code(form(b"AIFF", &[c])), bad, "{ch} ch, {bits} bits");
    }
    let mut zero_rate = comm(1, 0, 16, 1.0);
    zero_rate[8..18].fill(0);
    assert_eq!(open_code(form(b"AIFF", &[chunk(b"COMM", &zero_rate)])), bad);
    let mut nan_rate = comm(1, 0, 16, 1.0);
    nan_rate[8..10].copy_from_slice(&[0x7F, 0xFF]);
    assert_eq!(open_code(form(b"AIFF", &[chunk(b"COMM", &nan_rate)])), bad);
    let mut neg_rate = comm(1, 0, 16, 44_100.0);
    neg_rate[8] |= 0x80;
    assert_eq!(open_code(form(b"AIFF", &[chunk(b"COMM", &neg_rate)])), bad);
    let past_end = ssnd(9, 0, &[0; 4]);
    let mut short_offset = past_end.clone();
    short_offset.truncate(12);
    assert_eq!(
        open_code(form(
            b"AIFF",
            &[comm_ok.clone(), chunk(b"SSND", &short_offset)]
        )),
        bad
    );
    assert_eq!(
        open_code(form(b"AIFF", &[comm_ok.clone(), chunk(b"SSND", &[0; 7])])),
        bad
    );
    let two = [
        chunk(b"SSND", &ssnd(0, 0, &[])),
        chunk(b"SSND", &ssnd(0, 0, &[])),
    ];
    assert_eq!(
        open_code(form(
            b"AIFF",
            &[comm_ok.clone(), two[0].clone(), two[1].clone()]
        )),
        bad
    );
    // A metadata chunk that claims more bytes than the FORM holds.
    let mut lying = chunk(b"ANNO", b"ab");
    lying[4..8].copy_from_slice(&1000u32.to_be_bytes());
    assert_eq!(open_code(form(b"AIFF", &[comm_ok, lying])), bad);
}

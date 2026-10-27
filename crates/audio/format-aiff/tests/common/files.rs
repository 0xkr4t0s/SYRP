// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The B5 test files, built from the public layouts. B6's parity scenarios embed the same
//! bytes (`tests/scenarios.rs` checks they agree).

use super::{chunk, comm, comm_c, form, pstring, ssnd};

/// One test file: a scenario name, what it exercises, and its bytes.
pub struct Case {
    pub name: &'static str,
    pub title: &'static str,
    pub bytes: Vec<u8>,
}

fn be16(v: &[i16]) -> Vec<u8> {
    v.iter().flat_map(|s| s.to_be_bytes()).collect()
}

fn aiff(ch: i16, frames: u32, bits: i16, data: &[u8]) -> Vec<u8> {
    let c = chunk(b"COMM", &comm(ch, frames, bits, 44_100.0));
    form(b"AIFF", &[c, chunk(b"SSND", &ssnd(0, 0, data))])
}

fn aifc(ch: i16, frames: u32, bits: i16, kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let fver = chunk(b"FVER", &0xA280_5140u32.to_be_bytes());
    let c = chunk(b"COMM", &comm_c(ch, frames, bits, 48_000.0, kind));
    form(b"AIFC", &[fver, c, chunk(b"SSND", &ssnd(0, 0, data))])
}

/// `MARK` with markers 1 at 0 ("start"), 2 at 2 ("loop end"), 3 at 3 (no name).
fn mark() -> Vec<u8> {
    let mut b = 3u16.to_be_bytes().to_vec();
    for (id, pos, name) in [
        (1i16, 0u32, &b"start"[..]),
        (2, 2, b"loop end"),
        (3, 3, b""),
    ] {
        b.extend_from_slice(&id.to_be_bytes());
        b.extend_from_slice(&pos.to_be_bytes());
        b.extend_from_slice(&pstring(name));
    }
    chunk(b"MARK", &b)
}

/// `INST`: note 60, detune 0, keys 0..127, velocities 1..127, gain 0, sustain forward 1 to 2,
/// release ping-pong 2 to 3.
fn inst() -> Vec<u8> {
    let mut b = vec![60, 0, 0, 127, 1, 127, 0, 0];
    b.extend_from_slice(&be16(&[1, 1, 2, 2, 2, 3]));
    chunk(b"INST", &b)
}

fn c(name: &'static str, title: &'static str, bytes: Vec<u8>) -> Case {
    Case { name, title, bytes }
}

/// The valid files, then the malformed ones (whose names start with `bad_` or `choice_`).
pub fn catalogue() -> Vec<Case> {
    let s16 = be16(&[16_384, -16_384, 32_767, -32_768]);
    let s16le: Vec<u8> = [16_384i16, -16_384, 32_767, -32_768]
        .iter()
        .flat_map(|s| s.to_le_bytes())
        .collect();
    let f32s: Vec<u8> = [0.5f32, -0.25, 1.5, -1.0e-3]
        .iter()
        .flat_map(|v| v.to_be_bytes())
        .collect();
    let f64s: Vec<u8> = [0.1f64, -0.75, 2.0]
        .iter()
        .flat_map(|v| v.to_be_bytes())
        .collect();
    let comm16 = chunk(b"COMM", &comm(1, 4, 16, 44_100.0));
    let data16 = chunk(b"SSND", &ssnd(0, 0, &s16));
    let mut cases = vec![
        c(
            "int8_mono",
            "8-bit signed mono AIFF",
            aiff(1, 4, 8, &[0x80, 0xC0, 0x40, 0x7F]),
        ),
        c("int16_stereo", "16-bit stereo AIFF", aiff(2, 2, 16, &s16)),
        c(
            "int24_mono",
            "24-bit mono AIFF",
            aiff(1, 2, 24, &[0x80, 0, 0, 0x00, 0x00, 0x01]),
        ),
        c(
            "int32_mono",
            "32-bit mono AIFF",
            aiff(1, 2, 32, &[0x7F, 0xFF, 0xFF, 0xFF, 0xC0, 0, 0, 0]),
        ),
        c(
            "int12_mono",
            "12-bit samples left-justified in two bytes",
            aiff(1, 2, 12, &[0x80, 0x00, 0x7F, 0xF0]),
        ),
        c(
            "aifc_none_stereo",
            "AIFF-C NONE 16-bit stereo",
            aifc(2, 2, 16, b"NONE", &s16),
        ),
        c(
            "aifc_sowt_stereo",
            "AIFF-C sowt (little-endian) 16-bit stereo",
            aifc(2, 2, 16, b"sowt", &s16le),
        ),
        c(
            "aifc_fl32_stereo",
            "AIFF-C fl32 stereo",
            aifc(2, 2, 32, b"fl32", &f32s),
        ),
        c(
            "aifc_fl64_mono",
            "AIFF-C FL64 (upper case) mono",
            aifc(1, 3, 64, b"FL64", &f64s),
        ),
        c(
            "odd_and_unknown_chunks",
            "An odd-sized unknown chunk before COMM and an odd-sized SSND with its pad byte",
            form(
                b"AIFF",
                &[
                    chunk(b"zzzz", b"odd"),
                    chunk(b"COMM", &comm(1, 3, 8, 22_050.0)),
                    chunk(b"SSND", &ssnd(0, 0, &[1, 2, 3])),
                ],
            ),
        ),
        c(
            "ssnd_offset_and_block",
            "SSND with a 6-byte offset and a block size of 8",
            form(
                b"AIFF",
                &[comm16.clone(), chunk(b"SSND", &ssnd(6, 8, &s16))],
            ),
        ),
        c(
            "metadata",
            "MARK, INST loops, NAME, AUTH, (c), two ANNO, COMT and APPL chunks",
            form(
                b"AIFF",
                &[
                    comm16.clone(),
                    mark(),
                    inst(),
                    chunk(b"NAME", b"Take 3"),
                    chunk(b"AUTH", b"Someone"),
                    chunk(b"(c) ", b"2026 Someone"),
                    chunk(b"ANNO", b"first"),
                    chunk(b"ANNO", b"second"),
                    chunk(b"COMT", &[0, 0]),
                    chunk(b"APPL", b"syrp"),
                    data16.clone(),
                ],
            ),
        ),
    ];
    let mut truncated = aiff(1, 4, 16, &s16);
    truncated.truncate(truncated.len() - 3);
    let mut nan_rate = comm(1, 4, 16, 44_100.0);
    nan_rate[8..10].copy_from_slice(&[0x7F, 0xFF]);
    let mut unnormal = comm(1, 4, 16, 44_100.0);
    unnormal[10] &= 0x7F;
    let mut lying = chunk(b"ANNO", b"ab");
    lying[4..8].copy_from_slice(&1000u32.to_be_bytes());
    cases.extend([
        c(
            "choice_truncated_ssnd",
            "SSND cut 3 bytes short by the end of the file",
            truncated,
        ),
        c(
            "choice_comm_frames_exceed_ssnd",
            "COMM counts 6 frames, SSND holds 4",
            aiff(1, 6, 16, &s16),
        ),
        c(
            "bad_compression_ima4",
            "AIFF-C compression ima4, which is not supported",
            aifc(1, 0, 16, b"ima4", &[]),
        ),
        c(
            "bad_nan_rate",
            "COMM sample rate is a NaN",
            form(b"AIFF", &[chunk(b"COMM", &nan_rate), data16.clone()]),
        ),
        c(
            "bad_unnormal_rate",
            "COMM sample rate lacks the integer bit",
            form(b"AIFF", &[chunk(b"COMM", &unnormal), data16.clone()]),
        ),
        c(
            "bad_chunk_past_end",
            "An ANNO chunk claims 1,000 bytes",
            form(b"AIFF", &[comm16.clone(), lying, data16.clone()]),
        ),
        c(
            "bad_no_comm",
            "SSND without COMM",
            form(b"AIFF", std::slice::from_ref(&data16)),
        ),
        c(
            "bad_mark_count",
            "MARK claims 65,535 markers in 8 bytes",
            form(
                b"AIFF",
                &[
                    comm16,
                    chunk(b"MARK", &[0xFF, 0xFF, 0, 1, 0, 0, 0, 0, 0, 0]),
                    data16,
                ],
            ),
        ),
    ]);
    cases
}

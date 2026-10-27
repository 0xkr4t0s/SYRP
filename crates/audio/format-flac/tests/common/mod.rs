// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Test streams: audio frames from `flacenc`, metadata blocks built from RFC 9639's layout.
#![allow(dead_code)]

use flacenc::component::BitRepr;
use flacenc::error::Verify;
use syrp_error::Diagnostic;
use syrp_format_api::{AudioFormat, AudioReader, FormatError};
use syrp_format_flac::FlacAudioFormat;

/// Deterministic test signal on the `bits` grid, interleaved, with both extremes in it.
pub fn signal(frames: usize, channels: usize, bits: u32, seed: u64) -> Vec<i32> {
    let max = (1i64 << (bits - 1)) - 1;
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut out: Vec<i32> = (0..frames * channels)
        .map(|i| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let wave = ((i / channels) as f64 * 0.01).sin() * max as f64 * 0.7;
            let noise = (x % 64) as i64 - 32;
            (wave as i64 + noise).clamp(-max - 1, max) as i32
        })
        .collect();
    out[0] = (-max - 1) as i32;
    let last = channels.min(out.len() - 1);
    out[last] = max as i32;
    out
}

/// Encodes interleaved `samples`; returns the file and each frame's byte length.
pub fn encode(
    samples: &[i32],
    channels: usize,
    bits: u32,
    rate: usize,
    block: usize,
) -> (Vec<u8>, Vec<u64>) {
    let config = flacenc::config::Encoder::default()
        .into_verified()
        .expect("config");
    let src = flacenc::source::MemSource::from_samples(samples, channels, bits as usize, rate);
    let stream = flacenc::encode_with_fixed_block_size(&config, src, block).expect("encode");
    let sizes = (0..stream.frame_count())
        .map(|n| (stream.frame(n).expect("frame").count_bits() / 8) as u64)
        .collect();
    let mut sink = flacenc::bitsink::ByteSink::new();
    stream.write(&mut sink).expect("write");
    (sink.into_inner(), sizes)
}

/// Inserts `blocks` (type, body) after STREAMINFO, keeping blocks already there after them.
pub fn with_blocks(flac: &[u8], blocks: &[(u8, Vec<u8>)]) -> Vec<u8> {
    let mut all: Vec<(u8, Vec<u8>)> = Vec::new();
    let mut at = 4;
    loop {
        let last = flac[at] & 0x80 != 0;
        let len = u32::from_be_bytes([0, flac[at + 1], flac[at + 2], flac[at + 3]]) as usize;
        all.push((flac[at] & 0x7F, flac[at + 4..at + 4 + len].to_vec()));
        at += 4 + len;
        if last {
            break;
        }
    }
    let rest = all.split_off(1);
    all.extend(blocks.iter().cloned());
    all.extend(rest);
    let mut out = b"fLaC".to_vec();
    for (i, (kind, body)) in all.iter().enumerate() {
        let last = if i + 1 == all.len() { 0x80 } else { 0 };
        out.push(kind | last);
        out.extend_from_slice(&(body.len() as u32).to_be_bytes()[1..]);
        out.extend_from_slice(body);
    }
    out.extend_from_slice(&flac[at..]);
    out
}

/// Offset of the first frame in a file from [`with_blocks`] or [`encode`].
pub fn frames_start(flac: &[u8]) -> usize {
    let mut at = 4;
    loop {
        let len = u32::from_be_bytes([0, flac[at + 1], flac[at + 2], flac[at + 3]]) as usize;
        let last = flac[at] & 0x80 != 0;
        at += 4 + len;
        if last {
            return at;
        }
    }
}

/// Rewrites the packed rate/channels/bits/length word of STREAMINFO.
pub fn patch_streaminfo(flac: &mut [u8], f: impl FnOnce(u64) -> u64) {
    let word = u64::from_be_bytes(flac[18..26].try_into().expect("8 bytes"));
    flac[18..26].copy_from_slice(&f(word).to_be_bytes());
}

/// A SEEKTABLE body from (sample, offset from the first frame) points.
pub fn seektable(points: &[(u64, u64)]) -> Vec<u8> {
    let mut out = Vec::new();
    for (sample, offset) in points {
        out.extend_from_slice(&sample.to_be_bytes());
        out.extend_from_slice(&offset.to_be_bytes());
        out.extend_from_slice(&4096u16.to_be_bytes());
    }
    out
}

pub fn open(bytes: Vec<u8>) -> Result<Box<dyn AudioReader>, FormatError> {
    FlacAudioFormat::new().create_reader(Box::new(std::io::Cursor::new(bytes)))
}

/// The code of a failed result.
pub fn code<T>(r: Result<T, FormatError>) -> &'static str {
    match r {
        Ok(_) => panic!("expected an error"),
        Err(e) => e.code().as_str(),
    }
}

/// Reads `frames` frames from `start` as `f64`, channel after channel.
pub fn read_f64(
    r: &mut dyn AudioReader,
    start: i64,
    frames: usize,
) -> Result<Vec<f64>, FormatError> {
    let channels = r.spec().channel_count();
    let mut out = vec![f64::NAN; frames * channels];
    let block =
        syrp_audio_api::AudioBlockMut::new(&mut out, channels, frames, frames).expect("block");
    r.read(block.into(), start)?;
    Ok(out)
}

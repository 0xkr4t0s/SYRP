// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Builds WAV files byte by byte from the public RIFF layout.
#![allow(dead_code)] // Each test binary uses a different subset.

use std::io::Cursor;

use syrp_format_api::{AudioFormat, AudioReader, FormatError};
use syrp_format_wav::WavAudioFormat;

/// One chunk with its pad byte when `body` has an odd length.
pub fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut v = id.to_vec();
    v.extend_from_slice(&u32::try_from(body.len()).unwrap().to_le_bytes());
    v.extend_from_slice(body);
    if body.len() % 2 == 1 {
        v.push(0);
    }
    v
}

/// A RIFF WAVE file around `chunks`, with a correct RIFF size.
pub fn riff(chunks: &[Vec<u8>]) -> Vec<u8> {
    let body: Vec<u8> = chunks.concat();
    let mut v = b"RIFF".to_vec();
    v.extend_from_slice(&u32::try_from(body.len() + 4).unwrap().to_le_bytes());
    v.extend_from_slice(b"WAVE");
    v.extend_from_slice(&body);
    v
}

/// A 16-byte `fmt ` body.
pub fn fmt(tag: u16, channels: u16, rate: u32, bits: u16) -> Vec<u8> {
    let container = bits.div_ceil(8);
    let align = channels * container;
    let mut v = Vec::new();
    v.extend_from_slice(&tag.to_le_bytes());
    v.extend_from_slice(&channels.to_le_bytes());
    v.extend_from_slice(&rate.to_le_bytes());
    v.extend_from_slice(&(rate * u32::from(align)).to_le_bytes());
    v.extend_from_slice(&align.to_le_bytes());
    v.extend_from_slice(&bits.to_le_bytes());
    v
}

/// Opens `bytes` through the `AudioFormat` trait.
pub fn open(bytes: Vec<u8>) -> Result<Box<dyn AudioReader>, FormatError> {
    WavAudioFormat::new().create_reader(Box::new(Cursor::new(bytes)))
}

/// Reads every frame of every channel as `f64`, channel-major.
pub fn read_all_f64(r: &mut dyn AudioReader) -> Vec<Vec<f64>> {
    let ch = r.spec().channel_count();
    let n = usize::try_from(r.length()).unwrap();
    let mut buf = vec![0.0f64; ch * n.max(1)];
    let block = syrp_audio_api::AudioBlockMut::new(&mut buf, ch, n, n.max(1)).unwrap();
    r.read(block.into(), 0).unwrap();
    buf.chunks(n.max(1)).map(|c| c[..n].to_vec()).collect()
}

/// Reads every frame of every channel as `f32`, channel-major.
pub fn read_all_f32(r: &mut dyn AudioReader) -> Vec<Vec<f32>> {
    let ch = r.spec().channel_count();
    let n = usize::try_from(r.length()).unwrap();
    let mut buf = vec![0.0f32; ch * n.max(1)];
    let block = syrp_audio_api::AudioBlockMut::new(&mut buf, ch, n, n.max(1)).unwrap();
    r.read(block.into(), 0).unwrap();
    buf.chunks(n.max(1)).map(|c| c[..n].to_vec()).collect()
}

// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Hand-built AIFF files from the public AIFF 1.3 / AIFF-C layouts.
#![allow(dead_code)]

use std::io::Cursor;

use syrp_audio_api::AudioBlockMut;
use syrp_error::Diagnostic;
use syrp_format_aiff::{AiffAudioFormat, f64_to_extended};
use syrp_format_api::{AudioFormat, AudioReader};

/// A chunk: id, big-endian size, body, and a pad byte when the size is odd.
pub fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = id.to_vec();
    out.extend_from_slice(&u32::try_from(body.len()).unwrap().to_be_bytes());
    out.extend_from_slice(body);
    if body.len() % 2 == 1 {
        out.push(0);
    }
    out
}

/// A FORM of `kind` around `chunks`, with a correct size.
pub fn form(kind: &[u8; 4], chunks: &[Vec<u8>]) -> Vec<u8> {
    let mut body = kind.to_vec();
    for c in chunks {
        body.extend_from_slice(c);
    }
    chunk(b"FORM", &body)
}

/// An AIFF `COMM` body.
pub fn comm(channels: i16, frames: u32, bits: i16, rate: f64) -> Vec<u8> {
    let mut b = channels.to_be_bytes().to_vec();
    b.extend_from_slice(&frames.to_be_bytes());
    b.extend_from_slice(&bits.to_be_bytes());
    b.extend_from_slice(&f64_to_extended(rate).unwrap());
    b
}

/// An AIFF-C `COMM` body with compression `kind` and a Pascal-string name.
pub fn comm_c(channels: i16, frames: u32, bits: i16, rate: f64, kind: &[u8; 4]) -> Vec<u8> {
    let mut b = comm(channels, frames, bits, rate);
    b.extend_from_slice(kind);
    b.extend_from_slice(&pstring(b"x"));
    b
}

/// A Pascal string: count byte, text, padded to an even length.
pub fn pstring(text: &[u8]) -> Vec<u8> {
    let mut b = vec![u8::try_from(text.len()).unwrap()];
    b.extend_from_slice(text);
    if b.len() % 2 == 1 {
        b.push(0);
    }
    b
}

/// An `SSND` body with `offset` junk bytes before `data`.
pub fn ssnd(offset: u32, block: u32, data: &[u8]) -> Vec<u8> {
    let mut b = offset.to_be_bytes().to_vec();
    b.extend_from_slice(&block.to_be_bytes());
    b.extend(std::iter::repeat_n(0xEE, offset as usize));
    b.extend_from_slice(data);
    b
}

pub fn open(bytes: Vec<u8>) -> Result<Box<dyn AudioReader>, syrp_format_api::FormatError> {
    AiffAudioFormat::new().create_reader(Box::new(Cursor::new(bytes)))
}

/// The error code of opening `bytes`, or "ok".
pub fn open_code(bytes: Vec<u8>) -> String {
    match open(bytes) {
        Ok(_) => "ok".into(),
        Err(e) => e.code().as_str().to_owned(),
    }
}

/// Every frame of every channel, as f64, channel-major.
pub fn read_all(r: &mut dyn AudioReader) -> Vec<f64> {
    let ch = r.spec().channel_count();
    let n = usize::try_from(r.length()).unwrap();
    let mut out = vec![0.0; ch * n.max(1)];
    let block = AudioBlockMut::new(&mut out, ch, n, n.max(1)).unwrap();
    r.read(block.into(), 0).unwrap();
    out.truncate(ch * n);
    out
}

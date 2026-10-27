// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The WAV writer: header, metadata, sample encoding and the RIFF or RF64 finish.

use std::io::SeekFrom;

use syrp_audio_api::{AudioBlock, AudioSample};
use syrp_format_api::{AnyBlock, AudioSpec, AudioWriter, FormatError, WriteSeek, WriterOptions};

use crate::layout;
use crate::meta_write::{encode, push_chunk};
use crate::riff::{GUID_TAIL, TAG_EXTENSIBLE, TAG_FLOAT, TAG_PCM};

/// Payload of the `JUNK` chunk reserved for `ds64`: RIFF size, data size and sample count
/// (8 bytes each) and an empty table (4 bytes), as EBU Tech 3306 lays it out.
const DS64_BODY: usize = 28;
/// Where the `JUNK`/`ds64` chunk starts: right after `RIFF`, size and `WAVE`.
const DS64_AT: u64 = 12;
/// Bytes encoded per `write_all`, rounded down to whole frames (at least one frame).
const SCRATCH_BYTES: usize = 16 * 1024;
/// The largest size a plain RIFF may write: `0xFFFF_FFFF` means "unknown" to readers.
const MAX_PLAIN_SIZE: u64 = 0xFFFF_FFFE;

/// Whether a file with this RIFF size must be written as RF64.
fn needs_rf64(riff_size: u64, threshold: u64) -> bool {
    riff_size > threshold.min(MAX_PLAIN_SIZE)
}

/// Encodes one sample of `bytes` bytes (a float when `float`) from `v`.
///
/// Integers follow the WP-2.1-02 B2 rule: multiply by 2^(bits - 1), round to nearest with
/// ties away from zero, clip to the integer range, NaN becomes 0; 8-bit adds 128.
fn encode_sample(out: &mut [u8], v: f64, bytes: usize, float: bool) {
    if float {
        #[allow(clippy::cast_possible_truncation)] // Rounding to the nearest f32 is the intent.
        out.copy_from_slice(&(v as f32).to_le_bytes());
        return;
    }
    let full = f64::from(1u32 << (bytes * 8 - 1));
    // `as` saturates and maps NaN to 0, which is the clip and NaN part of the rule.
    #[allow(clippy::cast_possible_truncation)]
    let q = ((v * full).round().clamp(-full, full - 1.0)) as i32;
    let le = q.to_le_bytes();
    if bytes == 1 {
        out[0] = (q + 128) as u8;
    } else {
        out.copy_from_slice(&le[..bytes]);
    }
}

/// Writes a WAV file; returned boxed by [`WavAudioFormat`](crate::WavAudioFormat).
pub(crate) struct WavWriter {
    out: Box<dyn WriteSeek>,
    spec: AudioSpec,
    bytes: usize,
    float: bool,
    /// For each file channel, the canonical channel of the written block it holds.
    canonical: Vec<usize>,
    rf64_threshold: u64,
    /// Stream position where the file starts; every patch offset is relative to it.
    start: u64,
    data_size_at: u64,
    data_bytes: u64,
    frames: u64,
    scratch: Vec<u8>,
    finished: bool,
}

/// The `fmt ` body for `spec`: the plain layout for up to two channels of up to 16 bits,
/// `WAVE_FORMAT_EXTENSIBLE` with `mask` otherwise.
fn fmt_body(spec: &AudioSpec, rate: u32, mask: u32) -> Vec<u8> {
    let bits = spec.bits_per_sample() as u16;
    let channels = spec.channel_count() as u16;
    let align = channels * (bits / 8);
    let extensible = channels > 2 || bits > 16;
    let tag = match (extensible, spec.is_float()) {
        (true, _) => TAG_EXTENSIBLE,
        (false, true) => TAG_FLOAT,
        (false, false) => TAG_PCM,
    };
    let mut v = Vec::with_capacity(40);
    v.extend_from_slice(&tag.to_le_bytes());
    v.extend_from_slice(&channels.to_le_bytes());
    v.extend_from_slice(&rate.to_le_bytes());
    v.extend_from_slice(&(rate.wrapping_mul(u32::from(align))).to_le_bytes());
    v.extend_from_slice(&align.to_le_bytes());
    v.extend_from_slice(&bits.to_le_bytes());
    if extensible {
        let sub = if spec.is_float() { TAG_FLOAT } else { TAG_PCM };
        v.extend_from_slice(&22u16.to_le_bytes());
        v.extend_from_slice(&bits.to_le_bytes());
        v.extend_from_slice(&mask.to_le_bytes());
        v.extend_from_slice(&sub.to_le_bytes());
        v.extend_from_slice(&GUID_TAIL);
    }
    v
}

impl WavWriter {
    pub(crate) fn create(
        mut out: Box<dyn WriteSeek>,
        options: &WriterOptions,
        rf64_threshold: u64,
    ) -> Result<Self, FormatError> {
        let spec = *options.spec();
        let rate = spec.sample_rate();
        if rate.fract() != 0.0 || rate > f64::from(u32::MAX) {
            return Err(FormatError::unsupported(
                "WAV needs a whole sample rate in Hz",
            ));
        }
        let rate = rate as u32;
        let channels = spec.channel_count();
        let bytes = spec.bits_per_sample() as usize / 8;
        if channels > usize::from(u16::MAX) / bytes {
            return Err(FormatError::unsupported(
                "more channels than a WAV frame holds",
            ));
        }
        let (mask, canonical) = layout::to_mask(spec.channels())?;
        let mut head = b"RIFF\0\0\0\0WAVE".to_vec();
        push_chunk(&mut head, b"JUNK", &[0; DS64_BODY])?;
        push_chunk(&mut head, b"fmt ", &fmt_body(&spec, rate, mask))?;
        head.extend(encode(options.metadata(), rate)?);
        head.extend_from_slice(b"data\0\0\0\0");
        let start = out.stream_position()?;
        out.write_all(&head)?;
        let block_align = channels * bytes;
        Ok(Self {
            out,
            spec,
            bytes,
            float: spec.is_float(),
            canonical,
            rf64_threshold,
            start,
            data_size_at: head.len() as u64 - 4,
            data_bytes: 0,
            frames: 0,
            scratch: vec![0; (SCRATCH_BYTES / block_align).max(1) * block_align],
            finished: false,
        })
    }

    fn put<T>(&mut self, block: AudioBlock<'_, T>) -> Result<(), FormatError>
    where
        T: AudioSample + Into<f64>,
    {
        let align = self.canonical.len() * self.bytes;
        let per_write = self.scratch.len() / align;
        let mut done = 0;
        while done < block.frames() {
            let n = per_write.min(block.frames() - done);
            for (slot, &ch) in self.canonical.iter().enumerate() {
                let src = &block.channel(ch)[done..done + n];
                for (i, &s) in src.iter().enumerate() {
                    let at = i * align + slot * self.bytes;
                    let out = &mut self.scratch[at..at + self.bytes];
                    encode_sample(out, s.into(), self.bytes, self.float);
                }
            }
            self.out.write_all(&self.scratch[..n * align])?;
            done += n;
        }
        self.data_bytes += (block.frames() * align) as u64;
        self.frames += block.frames() as u64;
        Ok(())
    }

    /// Pads the data, then patches sizes as RIFF or rewrites the header as RF64.
    fn complete(&mut self) -> Result<(), FormatError> {
        if self.finished {
            return Ok(());
        }
        self.finished = true;
        if self.data_bytes % 2 == 1 {
            self.out.write_all(&[0])?;
        }
        let riff_size = self.data_size_at + 4 + self.data_bytes + (self.data_bytes & 1) - 8;
        if needs_rf64(riff_size, self.rf64_threshold) {
            let mut ds64 = Vec::with_capacity(DS64_BODY);
            for v in [riff_size, self.data_bytes, self.frames] {
                ds64.extend_from_slice(&v.to_le_bytes());
            }
            ds64.extend_from_slice(&0u32.to_le_bytes());
            self.patch(0, b"RF64\xFF\xFF\xFF\xFF")?;
            let mut chunk = Vec::new();
            push_chunk(&mut chunk, b"ds64", &ds64)?;
            self.patch(DS64_AT, &chunk)?;
            self.patch(self.data_size_at, &u32::MAX.to_le_bytes())?;
        } else {
            self.patch(4, &(riff_size as u32).to_le_bytes())?;
            self.patch(self.data_size_at, &(self.data_bytes as u32).to_le_bytes())?;
        }
        self.out.seek(SeekFrom::Start(
            self.start + self.data_size_at + 4 + self.padded_data(),
        ))?;
        self.out.flush()?;
        Ok(())
    }

    fn padded_data(&self) -> u64 {
        self.data_bytes + (self.data_bytes & 1)
    }

    /// Writes the plain RIFF and `data` sizes for the data so far (both `0xFFFF_FFFF`,
    /// "unknown", once the file would need RF64) and returns to the end of the data.
    fn patch_sizes_so_far(&mut self) -> Result<(), FormatError> {
        let riff_size = self.data_size_at + 4 + self.data_bytes - 8;
        let (riff, data) = if needs_rf64(riff_size, self.rf64_threshold) {
            (u32::MAX, u32::MAX)
        } else {
            (riff_size as u32, self.data_bytes as u32)
        };
        self.patch(4, &riff.to_le_bytes())?;
        self.patch(self.data_size_at, &data.to_le_bytes())?;
        self.out.seek(SeekFrom::Start(
            self.start + self.data_size_at + 4 + self.data_bytes,
        ))?;
        self.out.flush()?;
        Ok(())
    }

    fn patch(&mut self, at: u64, bytes: &[u8]) -> Result<(), FormatError> {
        self.out.seek(SeekFrom::Start(self.start + at))?;
        self.out.write_all(bytes)?;
        Ok(())
    }
}

impl AudioWriter for WavWriter {
    fn spec(&self) -> &AudioSpec {
        &self.spec
    }

    fn write_samples(&mut self, block: AnyBlock<'_>) -> Result<(), FormatError> {
        match block {
            AnyBlock::F32(b) => self.put(b),
            AnyBlock::F64(b) => self.put(b),
            _ => Err(FormatError::unsupported("sample type")),
        }
    }

    fn flush(&mut self) -> Result<(), FormatError> {
        if self.finished {
            return Ok(());
        }
        self.patch_sizes_so_far()
    }

    fn finish(mut self: Box<Self>) -> Result<(), FormatError> {
        self.complete()
    }
}

impl Drop for WavWriter {
    fn drop(&mut self) {
        // Best effort, as the contract asks; `finish` reports the same failure.
        let _ = self.complete();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b4_rf64_threshold_never_leaves_an_unknown_size_in_plain_riff() {
        assert!(!needs_rf64(0xFFFF_FFFE, DEFAULT));
        assert!(needs_rf64(0xFFFF_FFFF, DEFAULT));
        assert!(needs_rf64(0x1_0000_0000, DEFAULT));
        assert!(!needs_rf64(100, 100));
        assert!(needs_rf64(101, 100));
    }

    const DEFAULT: u64 = crate::WavAudioFormat::DEFAULT_RF64_THRESHOLD;
}

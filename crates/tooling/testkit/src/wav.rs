// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! A minimal RIFF/WAVE writer (spec B2), so renders can be inspected and attached to reviews.

use std::io::Write;

use syrp_audio_api::AudioBlock;

use crate::TestkitError;

/// Sample format of a written WAV file.
/// ```
/// assert_eq!(syrp_testkit::WavFormat::Pcm24.bytes_per_sample(), 3);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum WavFormat {
    /// 32-bit IEEE float, written unchanged (format tag 3).
    Float32,
    /// 16-bit signed integer PCM (format tag 1).
    Pcm16,
    /// 24-bit signed integer PCM (format tag 1).
    Pcm24,
}

impl WavFormat {
    /// Bytes per sample of one channel.
    /// ```
    /// assert_eq!(syrp_testkit::WavFormat::Float32.bytes_per_sample(), 4);
    /// ```
    #[must_use]
    pub fn bytes_per_sample(self) -> usize {
        match self {
            WavFormat::Float32 => 4,
            WavFormat::Pcm16 => 2,
            WavFormat::Pcm24 => 3,
        }
    }
}

/// Quantises `x` to a signed integer of `bits` bits: NaN becomes 0, values are clamped to
/// [-1, 1], scaled by `2^(bits-1) - 1` and rounded to nearest (ties away from zero).
fn quantise(x: f32, bits: u32) -> i32 {
    if x.is_nan() {
        return 0;
    }
    let full = f64::from((1_i32 << (bits - 1)) - 1);
    // In range by construction: |x| <= 1, so the product fits the target width.
    (f64::from(x.clamp(-1.0, 1.0)) * full).round() as i32
}

/// Writes `audio` to `out` as a little-endian WAV stream of interleaved frames in `format`,
/// tagged with `sample_rate` rounded to whole Hz (spec B2).
///
/// Float files carry a `fact` chunk, as the format requires for non-PCM data. PCM conversion
/// clamps to [-1, 1] and rounds (see [`WavFormat`]). Fails with `SYRP-TESTKIT-0004` when the
/// data exceeds the 4 GiB RIFF limit, the channel count exceeds 65,535, or the rounded rate is
/// outside 1..=`u32::MAX`, and with `SYRP-TESTKIT-0003` on a write error.
/// ```
/// use syrp_testkit::{WavFormat, write_wav};
/// let mono = syrp_audio_api::AudioBlock::new(&[0.5_f32, -1.0], 1, 2, 2)?;
/// let mut bytes = Vec::new();
/// write_wav(&mut bytes, mono, 48_000.0, WavFormat::Pcm16)?;
/// assert_eq!(&bytes[..4], b"RIFF");
/// assert_eq!(bytes.len(), 44 + 4);
/// assert_eq!(&bytes[44..], &[0x00, 0x40, 0x01, 0x80]); // 16384, -32767
/// # Ok::<(), syrp_testkit::TestkitError>(())
/// ```
pub fn write_wav<W: Write>(
    mut out: W,
    audio: AudioBlock<'_, f32>,
    sample_rate: f64,
    format: WavFormat,
) -> Result<(), TestkitError> {
    let rate = sample_rate.round();
    if !(1.0..=f64::from(u32::MAX)).contains(&rate) {
        return Err(TestkitError::WavLimit);
    }
    let rate = rate as u32;
    let channels = u16::try_from(audio.channels()).map_err(|_| TestkitError::WavLimit)?;
    let width = format.bytes_per_sample();
    let block_align = usize::from(channels) * width;
    let data_len = block_align
        .checked_mul(audio.frames())
        .and_then(|n| u32::try_from(n).ok())
        .ok_or(TestkitError::WavLimit)?;
    let float = format == WavFormat::Float32;
    let pad = data_len % 2;
    // "WAVE" + fmt chunk (16 or 18 bytes) + optional fact chunk + data chunk header and body.
    let fmt_len: u32 = if float { 18 } else { 16 };
    let fact_len: u32 = if float { 12 } else { 0 };
    let riff_len = (4 + 8 + fmt_len + fact_len + 8)
        .checked_add(data_len)
        .and_then(|n| n.checked_add(pad))
        .ok_or(TestkitError::WavLimit)?;
    let frames = u32::try_from(audio.frames()).map_err(|_| TestkitError::WavLimit)?;

    out.write_all(b"RIFF")?;
    out.write_all(&riff_len.to_le_bytes())?;
    out.write_all(b"WAVEfmt ")?;
    out.write_all(&fmt_len.to_le_bytes())?;
    out.write_all(&(if float { 3_u16 } else { 1 }).to_le_bytes())?;
    out.write_all(&channels.to_le_bytes())?;
    out.write_all(&rate.to_le_bytes())?;
    // Checked: many channels at a high rate can overflow the u32 byte-rate field.
    let byte_rate = u32::try_from(block_align)
        .ok()
        .and_then(|b| b.checked_mul(rate))
        .ok_or(TestkitError::WavLimit)?;
    out.write_all(&byte_rate.to_le_bytes())?;
    let align = u16::try_from(block_align).map_err(|_| TestkitError::WavLimit)?;
    out.write_all(&align.to_le_bytes())?;
    out.write_all(&(width as u16 * 8).to_le_bytes())?;
    if float {
        out.write_all(&0_u16.to_le_bytes())?;
        out.write_all(b"fact")?;
        out.write_all(&4_u32.to_le_bytes())?;
        out.write_all(&frames.to_le_bytes())?;
    }
    out.write_all(b"data")?;
    out.write_all(&data_len.to_le_bytes())?;

    let mut frame_bytes = Vec::with_capacity(block_align);
    for i in 0..audio.frames() {
        frame_bytes.clear();
        for ch in audio.iter() {
            let x = ch[i];
            match format {
                WavFormat::Float32 => frame_bytes.extend_from_slice(&x.to_le_bytes()),
                WavFormat::Pcm16 => {
                    frame_bytes.extend_from_slice(&(quantise(x, 16) as i16).to_le_bytes());
                }
                WavFormat::Pcm24 => {
                    frame_bytes.extend_from_slice(&quantise(x, 24).to_le_bytes()[..3]);
                }
            }
        }
        out.write_all(&frame_bytes)?;
    }
    if pad == 1 {
        out.write_all(&[0])?;
    }
    Ok(())
}

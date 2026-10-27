// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Packed sample formats and the converters between them and planar `f32`/`f64` blocks, in the
//! role of JUCE's `AudioData` (WP-2.1-02 B1–B3).
//!
//! Audio devices and file formats hand over samples as bytes: integers of 8 to 32 bits or
//! floats, little- or big-endian, with the channels interleaved frame by frame or stored one
//! after another. [`read_into`] decodes such bytes into an [`AudioBlockMut`], [`write_from`]
//! encodes an [`AudioBlock`] into bytes, and [`convert_bytes`] goes from one packed format to
//! another.
//!
//! # The conversion rule
//! - An integer sample `n` of `bits` bits becomes the float `n / 2^(bits - 1)`, rounded once to
//!   the destination type (to nearest, ties to even). `UInt8` subtracts 128 first. `Int24In32`
//!   uses `bits = 24`. Every integer therefore lands in `-1.0..1.0`.
//! - A float `x` becomes the integer `x * 2^(bits - 1)`, rounded to nearest with ties away from
//!   zero, then clipped to the integer range, so `1.0` gives the largest integer and values
//!   outside `-1.0..=1.0` saturate. NaN gives 0. `UInt8` adds 128 after clipping. The
//!   `AudioData` documentation describes the scaling and clipping; the rounding and NaN parts
//!   are Syrp's choice, and the format crates follow the same rule.
//! - `Float32` and `Float64` are stored as they are: no scaling and no clipping. Narrowing
//!   `f64` to `Float32` rounds to nearest. `Float32` read into `f32` (and `Float64` into `f64`)
//!   keeps every bit, NaN payloads included.
//!
//! The converters never allocate, lock or do I/O, so they are safe on the audio thread. A slice
//! whose size does not match the block is a [`ConvertError`] and nothing is written.
//!
//! ```
//! use syrp_audio_api::AudioBlockMut;
//! use syrp_audio_basics::convert::{self, Endianness, SampleFormat};
//! // Two frames of stereo 16-bit little-endian, interleaved: L0 R0 L1 R1.
//! let bytes = [0x00, 0x40, 0x00, 0xC0, 0xFF, 0x7F, 0x00, 0x80];
//! let mut planar = [0.0_f32; 4];
//! let mut block = AudioBlockMut::new(&mut planar, 2, 2, 2)?;
//! convert::read_into(&bytes, SampleFormat::Int16, Endianness::Little, true, 2, &mut block)?;
//! assert_eq!(block.channel(0), &[0.5, 32767.0 / 32768.0]);
//! assert_eq!(block.channel(1), &[-0.5, -1.0]);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::fmt;

use syrp_audio_api::{AudioBlock, AudioBlockMut};
use syrp_error::{Diagnostic, ErrorCode};

use crate::BufferSample;

mod codec;

use codec::Codec;

/// The packed sample formats the converters read and write.
///
/// The integer formats and `Float32` are the ones the `AudioData` documentation lists;
/// `Float64` is added for file formats that store doubles.
///
/// ```
/// use syrp_audio_basics::convert::SampleFormat;
/// assert_eq!(SampleFormat::Int24.bytes_per_sample(), 3);
/// assert_eq!(SampleFormat::Int24In32.bytes_per_sample(), 4);
/// assert!(SampleFormat::Float32.is_float());
/// ```
#[doc(alias = "AudioData")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SampleFormat {
    /// Signed 8-bit integer.
    #[doc(alias = "AudioData::Int8")]
    Int8,
    /// Unsigned 8-bit integer with 128 as silence (as in 8-bit WAV).
    #[doc(alias = "AudioData::UInt8")]
    UInt8,
    /// Signed 16-bit integer.
    #[doc(alias = "AudioData::Int16")]
    Int16,
    /// Signed 24-bit integer packed in 3 bytes.
    #[doc(alias = "AudioData::Int24")]
    Int24,
    /// Signed 24-bit integer in the low 24 bits of a 32-bit word. Reading ignores the top
    /// byte; writing fills it with the sign (Syrp's choice), so the word is also a valid
    /// 32-bit integer of the same value.
    #[doc(alias = "AudioData::Int24in32")]
    Int24In32,
    /// Signed 32-bit integer.
    #[doc(alias = "AudioData::Int32")]
    Int32,
    /// IEEE 754 single-precision float.
    #[doc(alias = "AudioData::Float32")]
    Float32,
    /// IEEE 754 double-precision float.
    Float64,
}

impl SampleFormat {
    /// Every format, in declaration order.
    ///
    /// ```
    /// use syrp_audio_basics::convert::SampleFormat;
    /// assert_eq!(SampleFormat::ALL.len(), 8);
    /// ```
    pub const ALL: &'static [SampleFormat] = &[
        SampleFormat::Int8,
        SampleFormat::UInt8,
        SampleFormat::Int16,
        SampleFormat::Int24,
        SampleFormat::Int24In32,
        SampleFormat::Int32,
        SampleFormat::Float32,
        SampleFormat::Float64,
    ];

    /// Bytes one sample takes.
    ///
    /// ```
    /// use syrp_audio_basics::convert::SampleFormat;
    /// assert_eq!(SampleFormat::UInt8.bytes_per_sample(), 1);
    /// assert_eq!(SampleFormat::Float64.bytes_per_sample(), 8);
    /// ```
    #[doc(alias = "AudioData::Int16::bytesPerSample")]
    #[must_use]
    pub const fn bytes_per_sample(self) -> usize {
        match self {
            SampleFormat::Int8 | SampleFormat::UInt8 => 1,
            SampleFormat::Int16 => 2,
            SampleFormat::Int24 => 3,
            SampleFormat::Int24In32 | SampleFormat::Int32 | SampleFormat::Float32 => 4,
            SampleFormat::Float64 => 8,
        }
    }

    /// True for `Float32` and `Float64`, which are stored without scaling or clipping.
    ///
    /// ```
    /// use syrp_audio_basics::convert::SampleFormat;
    /// assert!(!SampleFormat::Int32.is_float());
    /// ```
    #[must_use]
    pub const fn is_float(self) -> bool {
        matches!(self, SampleFormat::Float32 | SampleFormat::Float64)
    }
}

/// The byte order of multi-byte samples. Single-byte formats ignore it.
///
/// ```
/// use syrp_audio_basics::convert::Endianness;
/// assert_ne!(Endianness::Native.resolve(), Endianness::Native);
/// ```
#[doc(alias = "AudioData::LittleEndian")]
#[doc(alias = "AudioData::BigEndian")]
#[doc(alias = "AudioData::NativeEndian")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Endianness {
    /// Least significant byte first (WAV, most devices).
    Little,
    /// Most significant byte first (AIFF).
    Big,
    /// The byte order of the CPU running the code.
    Native,
}

impl Endianness {
    /// `Little` or `Big`: `Native` becomes the target's byte order.
    ///
    /// ```
    /// use syrp_audio_basics::convert::Endianness;
    /// assert_eq!(Endianness::Big.resolve(), Endianness::Big);
    /// # #[cfg(target_endian = "little")]
    /// assert_eq!(Endianness::Native.resolve(), Endianness::Little);
    /// ```
    #[must_use]
    pub const fn resolve(self) -> Endianness {
        match self {
            Endianness::Native if cfg!(target_endian = "big") => Endianness::Big,
            Endianness::Native => Endianness::Little,
            other => other,
        }
    }
}

/// Errors from the converters, catalogued in `errors/audio.toml`. On any error the
/// destination is left untouched.
///
/// ```
/// use syrp_audio_api::AudioBlockMut;
/// use syrp_audio_basics::convert::{self, ConvertError, Endianness, SampleFormat};
/// use syrp_error::Diagnostic;
/// let mut planar = [0.0_f32; 4];
/// let mut block = AudioBlockMut::new(&mut planar, 1, 4, 4)?;
/// let err = convert::read_into(&[0; 7], SampleFormat::Int16, Endianness::Little, false, 1, &mut block)
///     .unwrap_err();
/// assert!(matches!(err, ConvertError::ByteLengthMismatch { expected: 8, actual: 7, .. }), "{err:?}");
/// assert_eq!(err.code().as_str(), "SYRP-AUDIO-0007");
/// # Ok::<(), syrp_audio_api::AudioError>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ConvertError {
    /// The byte slice does not hold exactly the samples the block shape and format need
    /// (`SYRP-AUDIO-0007`). `expected` saturates at `usize::MAX` when the size overflows.
    #[non_exhaustive]
    ByteLengthMismatch {
        /// Bytes the shape needs.
        expected: usize,
        /// Bytes the slice holds.
        actual: usize,
    },
    /// [`read_into`] was given a block with more channels than the bytes hold
    /// (`SYRP-AUDIO-0008`).
    #[non_exhaustive]
    TooFewChannels {
        /// Channels in the destination block.
        block: usize,
        /// Channels in the byte data.
        data: usize,
    },
    /// [`convert_bytes`] was given a source that is not a whole number of samples
    /// (`SYRP-AUDIO-0009`).
    #[non_exhaustive]
    PartialSample {
        /// Bytes in the source slice.
        len: usize,
        /// Bytes per sample of the source format.
        bytes_per_sample: usize,
    },
}

impl fmt::Display for ConvertError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            ConvertError::ByteLengthMismatch { expected, actual } => write!(
                f,
                "the sample data is {actual} bytes but the block shape needs {expected}"
            ),
            ConvertError::TooFewChannels { block, data } => write!(
                f,
                "the block has {block} channels but the sample data holds only {data}"
            ),
            ConvertError::PartialSample {
                len,
                bytes_per_sample,
            } => write!(
                f,
                "{len} bytes is not a whole number of {bytes_per_sample}-byte samples"
            ),
        }
    }
}

impl std::error::Error for ConvertError {}

impl Diagnostic for ConvertError {
    fn code(&self) -> ErrorCode {
        ErrorCode::new(match self {
            ConvertError::ByteLengthMismatch { .. } => "SYRP-AUDIO-0007",
            ConvertError::TooFewChannels { .. } => "SYRP-AUDIO-0008",
            ConvertError::PartialSample { .. } => "SYRP-AUDIO-0009",
        })
    }
}

/// Checks that `len` bytes are exactly `channels * frames` samples of `format`.
fn check_bytes(
    len: usize,
    channels: usize,
    frames: usize,
    format: SampleFormat,
) -> Result<(), ConvertError> {
    let expected = channels
        .saturating_mul(frames)
        .saturating_mul(format.bytes_per_sample());
    if len == expected {
        Ok(())
    } else {
        Err(ConvertError::ByteLengthMismatch {
            expected,
            actual: len,
        })
    }
}

/// Decodes packed samples into `dest`.
///
/// `src` holds `channels` channels of `dest.frames()` samples of `format`: frame by frame when
/// `interleaved`, otherwise channel after channel. `dest` channel `c` receives data channel
/// `c`; when `dest` has fewer channels than the data, the remaining data channels are skipped
/// (Syrp's choice, for devices that deliver more channels than are in use). See the
/// [module documentation](self) for the conversion rule.
///
/// # Errors
/// [`ConvertError::TooFewChannels`] when `dest.channels() > channels`;
/// [`ConvertError::ByteLengthMismatch`] when `src.len()` is not
/// `channels * dest.frames() * format.bytes_per_sample()`. `dest` is untouched on error.
///
/// ```
/// use syrp_audio_api::AudioBlockMut;
/// use syrp_audio_basics::convert::{self, Endianness, SampleFormat};
/// // One channel of two 24-bit big-endian samples, read into f64.
/// let mut planar = [0.0_f64; 2];
/// let mut block = AudioBlockMut::new(&mut planar, 1, 2, 2)?;
/// let bytes = [0x40, 0x00, 0x00, 0x80, 0x00, 0x00];
/// convert::read_into(&bytes, SampleFormat::Int24, Endianness::Big, false, 1, &mut block)?;
/// assert_eq!(block.channel(0), &[0.5, -1.0]);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[doc(alias = "AudioData::ConverterInstance")]
#[doc(alias = "AudioData::convertSamples")]
pub fn read_into<T: BufferSample>(
    src: &[u8],
    format: SampleFormat,
    endianness: Endianness,
    interleaved: bool,
    channels: usize,
    dest: &mut AudioBlockMut<'_, T>,
) -> Result<(), ConvertError> {
    let (block_channels, frames) = (dest.channels(), dest.frames());
    if block_channels > channels {
        return Err(ConvertError::TooFewChannels {
            block: block_channels,
            data: channels,
        });
    }
    check_bytes(src.len(), channels, frames, format)?;
    if block_channels == 0 || frames == 0 {
        return Ok(());
    }
    let codec = Codec::new(format, endianness);
    let size = format.bytes_per_sample();
    for c in 0..block_channels {
        let out = dest.channel_mut(c);
        if interleaved {
            let frames_in = src.chunks_exact(channels * size);
            for (o, frame) in out.iter_mut().zip(frames_in) {
                *o = codec.decode(&frame[c * size..]);
            }
        } else {
            let start = c * frames * size;
            let samples = src[start..start + frames * size].chunks_exact(size);
            for (o, bytes) in out.iter_mut().zip(samples) {
                *o = codec.decode(bytes);
            }
        }
    }
    Ok(())
}

/// Encodes `src` into packed samples.
///
/// `dest` receives `src.channels()` channels of `src.frames()` samples of `format`: frame by
/// frame when `interleaved`, otherwise channel after channel. See the
/// [module documentation](self) for the conversion rule.
///
/// # Errors
/// [`ConvertError::ByteLengthMismatch`] when `dest.len()` is not
/// `src.channels() * src.frames() * format.bytes_per_sample()`. `dest` is untouched on error.
///
/// ```
/// use syrp_audio_api::AudioBlock;
/// use syrp_audio_basics::convert::{self, Endianness, SampleFormat};
/// let planar = [0.5_f32, -2.0, 0.0, 1.0];
/// let block = AudioBlock::new(&planar, 2, 2, 2)?;
/// let mut bytes = [0_u8; 4];
/// convert::write_from(&block, &mut bytes, SampleFormat::UInt8, Endianness::Native, true)?;
/// // Interleaved L0 R0 L1 R1; -2.0 clips to 0 and 1.0 to 255.
/// assert_eq!(bytes, [192, 128, 0, 255]);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[doc(alias = "AudioData::ConverterInstance")]
#[doc(alias = "AudioData::convertSamples")]
pub fn write_from<T: BufferSample>(
    src: &AudioBlock<'_, T>,
    dest: &mut [u8],
    format: SampleFormat,
    endianness: Endianness,
    interleaved: bool,
) -> Result<(), ConvertError> {
    let (channels, frames) = (src.channels(), src.frames());
    check_bytes(dest.len(), channels, frames, format)?;
    if channels == 0 || frames == 0 {
        return Ok(());
    }
    let codec = Codec::new(format, endianness);
    let size = format.bytes_per_sample();
    for c in 0..channels {
        let input = src.channel(c);
        if interleaved {
            let frames_out = dest.chunks_exact_mut(channels * size);
            for (&x, frame) in input.iter().zip(frames_out) {
                codec.encode(x, &mut frame[c * size..]);
            }
        } else {
            let start = c * frames * size;
            let samples = dest[start..start + frames * size].chunks_exact_mut(size);
            for (&x, bytes) in input.iter().zip(samples) {
                codec.encode(x, bytes);
            }
        }
    }
    Ok(())
}

/// Converts packed samples from one format to another, sample for sample, and returns the
/// number of samples converted.
///
/// The result is the same as reading `src` into `f64` with [`read_into`] and writing that with
/// [`write_from`]: widening integer conversions are exact, and narrowing ones follow the
/// conversion rule (round to nearest with ties away from zero, then clip). When both formats
/// are the same the bytes are copied, swapped if the byte orders differ. Interleaving does not
/// matter here: each sample keeps its position.
///
/// # Errors
/// [`ConvertError::PartialSample`] when `src` is not a whole number of samples;
/// [`ConvertError::ByteLengthMismatch`] when `dest` does not hold exactly as many samples of
/// `dest_format`. `dest` is untouched on error.
///
/// ```
/// use syrp_audio_basics::convert::{self, Endianness::Little, SampleFormat};
/// // Two Int16 samples to Int24.
/// let mut out = [0_u8; 6];
/// let n = convert::convert_bytes(&[0x01, 0x00, 0xFF, 0xFF], SampleFormat::Int16, Little,
///     &mut out, SampleFormat::Int24, Little)?;
/// assert_eq!(n, 2);
/// assert_eq!(out, [0x00, 0x01, 0x00, 0x00, 0xFF, 0xFF]);
/// # Ok::<(), syrp_audio_basics::convert::ConvertError>(())
/// ```
#[doc(alias = "AudioData::ConverterInstance")]
pub fn convert_bytes(
    src: &[u8],
    src_format: SampleFormat,
    src_endianness: Endianness,
    dest: &mut [u8],
    dest_format: SampleFormat,
    dest_endianness: Endianness,
) -> Result<usize, ConvertError> {
    let size = src_format.bytes_per_sample();
    if !src.len().is_multiple_of(size) {
        return Err(ConvertError::PartialSample {
            len: src.len(),
            bytes_per_sample: size,
        });
    }
    let count = src.len() / size;
    check_bytes(dest.len(), 1, count, dest_format)?;
    let (from, to) = (
        Codec::new(src_format, src_endianness),
        Codec::new(dest_format, dest_endianness),
    );
    let out_size = dest_format.bytes_per_sample();
    let pairs = src.chunks_exact(size).zip(dest.chunks_exact_mut(out_size));
    if src_format == dest_format {
        let swap = src_endianness.resolve() != dest_endianness.resolve();
        for (i, o) in pairs {
            o.copy_from_slice(i);
            if swap {
                o.reverse();
            }
        }
    } else {
        for (i, o) in pairs {
            to.encode(from.decode::<f64>(i), o);
        }
    }
    Ok(count)
}

// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The WAV reader: integer PCM and IEEE float sample decoding.

use std::io::{Read, SeekFrom};

use syrp_audio_api::AudioBlockMut;
use syrp_format_api::{
    AnyBlockMut, AudioReader, AudioSpec, FormatError, Metadata, ReadSeek, SampleKind,
};

use crate::WavError;
use crate::riff::{self, Fmt};
use crate::{layout, meta};

/// Bytes decoded per read call, rounded down to whole frames (at least one frame).
const SCRATCH_BYTES: usize = 16 * 1024;

/// How one stored sample becomes a number.
#[derive(Debug, Clone, Copy)]
enum Encoding {
    /// Integer PCM in `bytes` bytes (8-bit is unsigned), keeping the top `valid` bits.
    Int {
        bytes: usize,
        shift: u32,
        scale: f64,
    },
    F32,
    F64,
}

impl Encoding {
    fn of(fmt: &Fmt) -> Self {
        match (fmt.float, fmt.container_bytes) {
            (true, 4) => Self::F32,
            (true, _) => Self::F64,
            (false, bytes) => {
                let valid = u32::from(fmt.valid_bits);
                Self::Int {
                    bytes: usize::from(bytes),
                    shift: u32::from(bytes) * 8 - valid,
                    // 2^-(valid - 1): exact in an f64 for every valid width 1..=32.
                    scale: 1.0 / f64::from(1u32 << (valid - 1)),
                }
            }
        }
    }

    /// Decodes the sample starting at `b[0]`.
    fn decode<T: OutSample>(self, b: &[u8]) -> T {
        match self {
            Self::Int {
                bytes,
                shift,
                scale,
            } => {
                let raw: i32 = match bytes {
                    1 => i32::from(b[0]) - 128,
                    2 => i32::from(i16::from_le_bytes([b[0], b[1]])),
                    // Put the 24 bits at the top of an i32, then shift back to sign-extend.
                    3 => i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8,
                    _ => i32::from_le_bytes([b[0], b[1], b[2], b[3]]),
                };
                T::from_f64(f64::from(raw >> shift) * scale)
            }
            Self::F32 => T::from_f32(f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
            Self::F64 => {
                let mut le = [0; 8];
                le.copy_from_slice(&b[..8]);
                T::from_f64(f64::from_le_bytes(le))
            }
        }
    }
}

/// The two sample types a reader fills.
trait OutSample: syrp_audio_api::AudioSample {
    fn from_f32(v: f32) -> Self;
    fn from_f64(v: f64) -> Self;
}

impl OutSample for f32 {
    fn from_f32(v: f32) -> Self {
        v
    }
    #[allow(clippy::cast_possible_truncation)] // Rounding to the nearest f32 is the intent.
    fn from_f64(v: f64) -> Self {
        v as f32
    }
}

impl OutSample for f64 {
    fn from_f32(v: f32) -> Self {
        f64::from(v)
    }
    fn from_f64(v: f64) -> Self {
        v
    }
}

/// A reader for RIFF, RF64 and BW64 WAVE files holding integer PCM or IEEE float samples.
///
/// Integer samples become `value / 2^(bits - 1)` (8-bit samples are offset by 128 first), so
/// every stored value is exact in `f64` and, up to 24 bits, in `f32`. Channels come out in
/// the canonical order of [`AudioSpec::channels`], whatever order the file's channel mask
/// gives them.
///
/// ```
/// use syrp_format_api::AudioReader;
/// use syrp_format_wav::WavReader;
/// // A 1-frame mono 16-bit file holding the sample 0x4000 (0.5).
/// let mut f = b"RIFF\x26\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\x44\xac\0\0\x88\x58\x01\0\x02\0\x10\0".to_vec();
/// f.extend_from_slice(b"data\x02\0\0\0\x00\x40");
/// let reader = WavReader::open(Box::new(std::io::Cursor::new(f)))?;
/// assert_eq!(reader.length(), 1);
/// assert_eq!(reader.spec().bits_per_sample(), 16);
/// # Ok::<(), syrp_format_wav::WavError>(())
/// ```
#[doc(alias = "WavAudioFormatReader")]
pub struct WavReader {
    input: Box<dyn ReadSeek>,
    spec: AudioSpec,
    length: u64,
    metadata: Metadata,
    data_offset: u64,
    block_align: usize,
    sample_bytes: usize,
    encoding: Encoding,
    file_index: Vec<usize>,
    scratch: Vec<u8>,
}

impl std::fmt::Debug for WavReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WavReader")
            .field("spec", &self.spec)
            .field("length", &self.length)
            .finish_non_exhaustive()
    }
}

impl WavReader {
    /// Parses the header of `input` and returns a reader, or the precise [`WavError`].
    ///
    /// ```
    /// use syrp_error::Diagnostic;
    /// use syrp_format_wav::WavReader;
    /// let err = WavReader::open(Box::new(std::io::Cursor::new(b"FORM".to_vec()))).unwrap_err();
    /// assert_eq!(err.code().as_str(), "SYRP-FORMAT-0006");
    /// ```
    pub fn open(mut input: Box<dyn ReadSeek>) -> Result<Self, WavError> {
        let riff = riff::walk(input.as_mut())?;
        let fmt = riff.fmt;
        let layout = layout::from_mask(fmt.mask, fmt.channels)?;
        let kind = if fmt.float {
            SampleKind::Float
        } else {
            SampleKind::Int
        };
        let spec = AudioSpec::new(
            f64::from(fmt.sample_rate),
            layout.channels,
            u32::from(fmt.valid_bits),
            kind,
        )
        .map_err(|_| WavError::bad_format("no stream matches these values"))?;
        let metadata = meta::read(input.as_mut(), &riff.chunks, riff.len)?;
        let block_align = usize::from(fmt.block_align);
        let frames_per_read = (SCRATCH_BYTES / block_align).max(1);
        let mut scratch = Vec::new();
        scratch
            .try_reserve_exact(frames_per_read * block_align)
            .map_err(|_| WavError::OutOfMemory)?;
        scratch.resize(frames_per_read * block_align, 0);
        Ok(Self {
            input,
            spec,
            length: riff.data.size / u64::from(fmt.block_align),
            metadata,
            data_offset: riff.data.offset,
            block_align,
            sample_bytes: usize::from(fmt.container_bytes),
            encoding: Encoding::of(&fmt),
            file_index: layout.file_index,
            scratch,
        })
    }

    fn fill<T: OutSample>(
        &mut self,
        dest: &mut AudioBlockMut<'_, T>,
        start: u64,
    ) -> Result<(), FormatError> {
        let at = start
            .checked_mul(self.block_align as u64)
            .and_then(|o| o.checked_add(self.data_offset))
            .ok_or_else(|| FormatError::invalid_data("read position overflows"))?;
        self.input.seek(SeekFrom::Start(at))?;
        let frames = dest.frames();
        let per_read = self.scratch.len() / self.block_align;
        let mut done = 0;
        while done < frames {
            let n = per_read.min(frames - done);
            let bytes = &mut self.scratch[..n * self.block_align];
            self.input.read_exact(bytes)?;
            for (c, &file_ch) in self.file_index.iter().enumerate().take(dest.channels()) {
                let out = &mut dest.channel_mut(c)[done..done + n];
                let first = file_ch * self.sample_bytes;
                for (i, o) in out.iter_mut().enumerate() {
                    *o = self.encoding.decode(&bytes[first + i * self.block_align..]);
                }
            }
            done += n;
        }
        Ok(())
    }
}

impl AudioReader for WavReader {
    fn spec(&self) -> &AudioSpec {
        &self.spec
    }

    fn length(&self) -> u64 {
        self.length
    }

    fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    fn read_samples(&mut self, dest: AnyBlockMut<'_>, start: u64) -> Result<(), FormatError> {
        match dest {
            AnyBlockMut::F32(mut b) => self.fill(&mut b, start),
            AnyBlockMut::F64(mut b) => self.fill(&mut b, start),
            _ => Err(FormatError::unsupported("sample type")),
        }
    }
}

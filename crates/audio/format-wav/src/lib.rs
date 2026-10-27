// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Syrp's WAV codec (master plan §4.2): RIFF WAVE, `WAVE_FORMAT_EXTENSIBLE` and RF64/BW64, as
//! an [`AudioFormat`] of the `syrp-format-api` contract.
//!
//! - [`WavAudioFormat`]: the format description and the factory for readers and writers.
//! - [`WavReader`]: decodes integer PCM (8-bit unsigned, 16, 24 and 32-bit signed, and
//!   narrower `valid_bits_per_sample`) and 32/64-bit IEEE float.
//! - Metadata: `bext`, `cue ` with `LIST`/`adtl` labels, `smpl` loops and `LIST`/`INFO` text
//!   become typed [`MetadataItem`](syrp_format_api::MetadataItem)s; `acid`, `inst`, `iXML`,
//!   `axml`, `Trkn` and `cart` are kept byte for byte as `Other`.
//! - [`WavError`]: the codec's coded errors (`SYRP-FORMAT-0006` onwards).
//!
//! WP-2.1-14 lands in increments: robustness tests and parity scenarios come last.
//!
//! The layouts come from public specifications only: Microsoft's RIFF and
//! `WAVEFORMATEXTENSIBLE` documentation, EBU Tech 3285 (BWF), EBU Tech 3306 (RF64) and
//! ITU-R BS.2088 (BW64).
//!
//! ```
//! use syrp_format_api::AudioFormat;
//! use syrp_format_wav::WavAudioFormat;
//! let wav = WavAudioFormat::new();
//! assert!(wav.can_read(b"RIFF\0\0\0\0WAVE"));
//! assert!(wav.matches_extension("take.BWF"));
//! ```

mod error;
mod layout;
mod meta;
mod meta_write;
mod reader;
mod riff;
mod writer;

pub use error::WavError;
pub use reader::WavReader;

use syrp_format_api::{
    AudioFormat, AudioReader, AudioWriter, FormatError, ReadSeek, WriteSeek, WriterOptions,
};

/// The WAV format: RIFF WAVE files, with RF64/BW64 for files past the 32-bit size limit.
///
/// It is a small `Copy` description; readers and writers carry the per-file state.
///
/// ```
/// use syrp_format_api::AudioFormat;
/// use syrp_format_wav::WavAudioFormat;
/// let wav = WavAudioFormat::new();
/// assert_eq!(wav.name(), "WAV file");
/// assert_eq!(wav.file_extensions(), ["wav", "bwf"]);
/// assert_eq!(wav.bit_depths(), [8, 16, 24, 32]);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WavAudioFormat {
    rf64_threshold: u64,
}

impl WavAudioFormat {
    /// The default RF64 threshold: the largest size a 32-bit RIFF size field can hold.
    ///
    /// ```
    /// assert_eq!(syrp_format_wav::WavAudioFormat::DEFAULT_RF64_THRESHOLD, 0xFFFF_FFFF);
    /// ```
    pub const DEFAULT_RF64_THRESHOLD: u64 = 0xFFFF_FFFF;

    /// The WAV format with the default RF64 threshold.
    ///
    /// ```
    /// let wav = syrp_format_wav::WavAudioFormat::new();
    /// assert_eq!(wav.rf64_threshold(), syrp_format_wav::WavAudioFormat::DEFAULT_RF64_THRESHOLD);
    /// ```
    #[must_use]
    pub const fn new() -> Self {
        Self {
            rf64_threshold: Self::DEFAULT_RF64_THRESHOLD,
        }
    }

    /// Lowers (or raises) the RIFF size, in bytes, above which writers switch the file to
    /// RF64 when they finish. `0` makes every written file RF64. Values above
    /// [`DEFAULT_RF64_THRESHOLD`](Self::DEFAULT_RF64_THRESHOLD) are clamped to it, since a
    /// larger file cannot be plain RIFF.
    ///
    /// ```
    /// let always = syrp_format_wav::WavAudioFormat::new().with_rf64_threshold(0);
    /// assert_eq!(always.rf64_threshold(), 0);
    /// ```
    #[must_use]
    pub const fn with_rf64_threshold(mut self, bytes: u64) -> Self {
        self.rf64_threshold = if bytes < Self::DEFAULT_RF64_THRESHOLD {
            bytes
        } else {
            Self::DEFAULT_RF64_THRESHOLD
        };
        self
    }

    /// The RIFF size above which writers produce RF64.
    ///
    /// ```
    /// let wav = syrp_format_wav::WavAudioFormat::new().with_rf64_threshold(1 << 20);
    /// assert_eq!(wav.rf64_threshold(), 1 << 20);
    /// ```
    #[must_use]
    pub const fn rf64_threshold(&self) -> u64 {
        self.rf64_threshold
    }
}

impl Default for WavAudioFormat {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioFormat for WavAudioFormat {
    fn name(&self) -> &str {
        "WAV file"
    }

    fn file_extensions(&self) -> &[&str] {
        &["wav", "bwf"]
    }

    /// Empty: a WAV header stores any whole rate from 1 Hz to `u32::MAX` Hz (Syrp's choice;
    /// the writer rejects fractional rates).
    fn sample_rates(&self) -> &[u32] {
        &[]
    }

    /// 8, 16, 24 and 32-bit integer, and (with 32) 32-bit float.
    fn bit_depths(&self) -> &[u32] {
        &[8, 16, 24, 32]
    }

    fn can_read(&self, header: &[u8]) -> bool {
        header.len() >= 12
            && matches!(
                header[..4],
                [b'R', b'I', b'F', b'F'] | [b'R', b'F', b'6', b'4'] | [b'B', b'W', b'6', b'4']
            )
            && header[8..12] == *b"WAVE"
    }

    fn create_reader(&self, input: Box<dyn ReadSeek>) -> Result<Box<dyn AudioReader>, FormatError> {
        Ok(Box::new(WavReader::open(input)?))
    }

    /// A writer for 8, 16, 24 or 32-bit integer or 32-bit float samples at a whole sample
    /// rate. Up to two channels of up to 16 bits get the plain `fmt ` layout, anything else
    /// `WAVE_FORMAT_EXTENSIBLE` with a channel mask. Metadata chunks go between `fmt ` and
    /// `data` (Syrp's choice); `finish` patches the sizes, or rewrites the header as RF64
    /// when the RIFF size passes [`rf64_threshold`](WavAudioFormat::rf64_threshold).
    fn create_writer(
        &self,
        output: Box<dyn WriteSeek>,
        options: &WriterOptions,
    ) -> Result<Box<dyn AudioWriter>, FormatError> {
        self.check_spec(options.spec())?;
        Ok(Box::new(writer::WavWriter::create(
            output,
            options,
            self.rf64_threshold,
        )?))
    }
}

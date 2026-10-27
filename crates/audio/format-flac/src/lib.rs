// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! FLAC files for Syrp, behind the `syrp-format-api` contract (master plan §4.2).
//!
//! [`FlacAudioFormat`] reads native FLAC streams through [`claxon`] 0.4 and parses the
//! metadata blocks itself, from the layout in RFC 9639. Both crates stay behind this crate's
//! own types, so either can be replaced without an API change.
//!
//! # Reading
//! - The stream shape and length come from `STREAMINFO`. A stream whose total length is
//!   unknown (0) is refused with `SYRP-FORMAT-0003`, because random access needs it (Syrp's
//!   choice).
//! - Integer samples are divided by 2^(bits − 1), the converters' rule (WP-2.1-02 B2). `claxon`
//!   decodes frames whose header names its bit depth: 8, 12, 16, 20 and 24 bits. Other depths
//!   are refused with `SYRP-FORMAT-0003` when the reader is created.
//! - Metadata blocks other than `STREAMINFO` and `SEEKTABLE` are checked for length and
//!   skipped; mapping them to tags and cue points is the next increment (B3).
//! - Damaged or truncated streams give `SYRP-FORMAT-0002`, never a panic.
//!
//! # Random access and its cost
//! The reader keeps the last decoded frame and the byte offset of the frame after it.
//! A read that starts in the kept frame or after it decodes forward, so a forward read never
//! decodes a frame twice. Any other start (backwards, or far ahead) seeks to the nearest
//! earlier frame the reader knows and decodes forward from there. It knows the first frame,
//! every `SEEKTABLE` point, and one frame it has already decoded per 65,536 samples.
//!
//! So a backward seek into audio already read costs at most about 65,536 samples plus one
//! block of decoding (about 1.4 s of audio at 48 kHz). A seek into audio not yet read, in a
//! file without a `SEEKTABLE`, decodes everything between the nearest known frame and the
//! target. Memory is one decoded block (at most 8 channels × 65,535 samples × 4 bytes) plus
//! 16 bytes per remembered frame.
//!
//! ```
//! use syrp_format_api::AudioFormat;
//! use syrp_format_flac::FlacAudioFormat;
//! let flac = FlacAudioFormat::new();
//! assert!(flac.can_read(b"fLaC\0\0\0\x22"));
//! assert!(flac.matches_extension("take.FLAC"));
//! ```

use syrp_format_api::WriterOptions;
use syrp_format_api::{AudioFormat, AudioReader, AudioWriter, FormatError, ReadSeek, WriteSeek};

mod meta;
mod reader;
mod source;

/// Sample rates the writer accepts, in Hz (Syrp's choice: the public `FlacAudioFormat`
/// documentation does not list them).
const SAMPLE_RATES: [u32; 15] = [
    8_000, 11_025, 12_000, 16_000, 22_050, 24_000, 32_000, 44_100, 48_000, 88_200, 96_000, 176_400,
    192_000, 352_800, 384_000,
];

/// Bit depths the writer accepts (Syrp's choice: the documentation does not list them).
const BIT_DEPTHS: [u32; 2] = [16, 24];

/// The FLAC lossless audio format: `.flac` files.
///
/// It reads native FLAC streams, optionally after a leading ID3v2 tag (Syrp's choice, see
/// [`can_read`](AudioFormat::can_read)). Readers are created with
/// [`create_reader`](AudioFormat::create_reader); see the crate documentation for how they
/// seek and what that costs.
///
/// ```
/// use syrp_format_api::AudioFormat;
/// use syrp_format_flac::FlacAudioFormat;
/// let flac = FlacAudioFormat::new();
/// assert_eq!(flac.name(), "FLAC file");
/// assert_eq!(flac.file_extensions(), &["flac"]);
/// assert_eq!(flac.bit_depths(), &[16, 24]);
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FlacAudioFormat {
    _private: (),
}

impl FlacAudioFormat {
    /// The FLAC format.
    ///
    /// ```
    /// let flac = syrp_format_flac::FlacAudioFormat::new();
    /// # let _ = flac;
    /// ```
    #[must_use]
    pub const fn new() -> Self {
        Self { _private: () }
    }
}

impl AudioFormat for FlacAudioFormat {
    fn name(&self) -> &str {
        "FLAC file"
    }

    fn file_extensions(&self) -> &[&str] {
        &["flac"]
    }

    fn sample_rates(&self) -> &[u32] {
        &SAMPLE_RATES
    }

    fn bit_depths(&self) -> &[u32] {
        &BIT_DEPTHS
    }

    /// True for a header starting with `fLaC`, or with a well-formed ID3v2 tag followed by
    /// `fLaC` inside `header` (Syrp's choice: an ID3v2 tag alone is not enough, because other
    /// formats use it too).
    fn can_read(&self, header: &[u8]) -> bool {
        if header.starts_with(meta::MAGIC) {
            return true;
        }
        meta::id3v2_len(header)
            .and_then(|n| usize::try_from(n).ok())
            .and_then(|n| header.get(n..))
            .is_some_and(|rest| rest.starts_with(meta::MAGIC))
    }

    fn create_reader(&self, input: Box<dyn ReadSeek>) -> Result<Box<dyn AudioReader>, FormatError> {
        Ok(Box::new(reader::FlacReader::open(input)?))
    }

    fn create_writer(
        &self,
        _output: Box<dyn WriteSeek>,
        options: &WriterOptions,
    ) -> Result<Box<dyn AudioWriter>, FormatError> {
        self.check_spec(options.spec())?;
        Err(FormatError::unsupported(
            "FLAC writing (arrives with WP-2.1-15 B4)",
        ))
    }
}

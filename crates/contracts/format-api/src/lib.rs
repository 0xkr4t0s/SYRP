// SPDX-License-Identifier: MIT OR Apache-2.0
//! The Syrp audio file format contract (master plan §4, L1).
//!
//! - [`AudioFormat`]: a codec's description (name, extensions, rates, bit depths), header
//!   sniffing with [`AudioFormat::can_read`], and factories for readers and writers.
//! - [`AudioReader`]: random-access reads into `syrp-audio-api` views, zero-filled outside the
//!   file, plus the file's [`Metadata`].
//! - [`AudioWriter`]: sequential writes from views, `flush` and `finish`, all reporting I/O
//!   failure as [`FormatError`] instead of panicking.
//! - [`Metadata`] and [`MetadataItem`]: typed BWF `bext`, cue points, loops and text tags, with
//!   an [`MetadataItem::Other`] escape for anything else.
//!
//! Sample data only moves through [`syrp_audio_api::AudioBlock`] and
//! [`syrp_audio_api::AudioBlockMut`]; [`AnyBlock`] and [`AnyBlockMut`] only say which of the two
//! sample types a call carries.
//!
//! ```
//! use syrp_audio_api::ChannelSet;
//! use syrp_format_api::{AudioSpec, SampleKind, WriterOptions};
//! let spec = AudioSpec::new(48_000.0, ChannelSet::stereo(), 24, SampleKind::Int)?;
//! let options = WriterOptions::new(spec);
//! assert_eq!(options.spec().channel_count(), 2);
//! # Ok::<(), syrp_format_api::FormatError>(())
//! ```

use std::fmt;
use std::io;

use syrp_error::{Diagnostic, ErrorCode};

mod block;
mod format;
mod metadata;
mod spec;

pub use block::{AnyBlock, AnyBlockMut};
pub use format::{AudioFormat, AudioReader, AudioWriter, ReadSeek, WriteSeek};
pub use metadata::{Bext, CuePoint, LoopKind, Metadata, MetadataItem, SampleLoop, Tag};
pub use spec::{AudioSpec, SampleKind, WriterOptions};

/// Errors from formats, readers and writers, catalogued in `errors/format.toml`.
///
/// ```
/// use syrp_error::Diagnostic;
/// use syrp_format_api::FormatError;
/// let err = FormatError::from(std::io::Error::other("disk full"));
/// assert_eq!(err.code().as_str(), "SYRP-FORMAT-0001");
/// ```
#[derive(Debug)]
#[non_exhaustive]
pub enum FormatError {
    /// The underlying stream failed (`SYRP-FORMAT-0001`).
    Io(io::Error),
    /// The bytes are not a valid file of this format (`SYRP-FORMAT-0002`).
    #[non_exhaustive]
    InvalidData {
        /// What is wrong, in a few words.
        reason: String,
    },
    /// The format cannot handle the requested stream or feature (`SYRP-FORMAT-0003`).
    #[non_exhaustive]
    Unsupported {
        /// What is not supported, in a few words.
        reason: String,
    },
    /// [`AudioSpec::new`] was given an impossible description (`SYRP-FORMAT-0004`).
    #[non_exhaustive]
    InvalidSpec {
        /// Which value is wrong.
        reason: &'static str,
    },
    /// A written block has the wrong number of channels (`SYRP-FORMAT-0005`).
    #[non_exhaustive]
    ChannelMismatch {
        /// The writer's channel count.
        expected: usize,
        /// The block's channel count.
        actual: usize,
    },
}

impl FormatError {
    /// A [`FormatError::InvalidData`] with `reason`.
    ///
    /// ```
    /// let err = syrp_format_api::FormatError::invalid_data("bad magic");
    /// assert_eq!(err.to_string(), "invalid audio data: bad magic");
    /// ```
    pub fn invalid_data(reason: impl Into<String>) -> Self {
        Self::InvalidData {
            reason: reason.into(),
        }
    }

    /// A [`FormatError::Unsupported`] with `reason`.
    ///
    /// ```
    /// let err = syrp_format_api::FormatError::unsupported("12-bit float");
    /// assert_eq!(err.to_string(), "unsupported by this format: 12-bit float");
    /// ```
    pub fn unsupported(reason: impl Into<String>) -> Self {
        Self::Unsupported {
            reason: reason.into(),
        }
    }
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "audio file I/O failed: {e}"),
            Self::InvalidData { reason } => write!(f, "invalid audio data: {reason}"),
            Self::Unsupported { reason } => write!(f, "unsupported by this format: {reason}"),
            Self::InvalidSpec { reason } => write!(f, "invalid audio spec: {reason}"),
            Self::ChannelMismatch { expected, actual } => {
                write!(f, "block has {actual} channels, writer expects {expected}")
            }
        }
    }
}

impl std::error::Error for FormatError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for FormatError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl Diagnostic for FormatError {
    fn code(&self) -> ErrorCode {
        ErrorCode::new(match self {
            Self::Io(_) => "SYRP-FORMAT-0001",
            Self::InvalidData { .. } => "SYRP-FORMAT-0002",
            Self::Unsupported { .. } => "SYRP-FORMAT-0003",
            Self::InvalidSpec { .. } => "SYRP-FORMAT-0004",
            Self::ChannelMismatch { .. } => "SYRP-FORMAT-0005",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b5_channel_mismatch_has_its_code() {
        let err = FormatError::ChannelMismatch {
            expected: 2,
            actual: 3,
        };
        assert_eq!(err.code().as_str(), "SYRP-FORMAT-0005");
        assert_eq!(err.to_string(), "block has 3 channels, writer expects 2");
    }
}

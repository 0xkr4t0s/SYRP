// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The WAV codec's own error codes (`errors/format.toml`, `SYRP-FORMAT-0006` onwards).

use std::fmt;
use std::io;

use syrp_error::{Diagnostic, ErrorCode};
use syrp_format_api::FormatError;

/// Why a WAV file could not be opened or decoded.
///
/// The [`AudioFormat`](syrp_format_api::AudioFormat) trait returns
/// [`FormatError`]; converting a `WavError` into one keeps this error's code at the start of
/// the reason text (for example `"SYRP-FORMAT-0007: ..."`). [`WavReader::open`](crate::WavReader::open)
/// returns the typed error directly.
///
/// ```
/// use syrp_error::Diagnostic;
/// use syrp_format_wav::WavError;
/// let err = WavError::NotWave;
/// assert_eq!(err.code().as_str(), "SYRP-FORMAT-0006");
/// let generic = syrp_format_api::FormatError::from(err);
/// assert!(generic.to_string().contains("SYRP-FORMAT-0006"));
/// ```
#[derive(Debug)]
#[non_exhaustive]
pub enum WavError {
    /// The stream failed (`SYRP-FORMAT-0001`).
    Io(io::Error),
    /// The input does not start with a `RIFF`, `RF64` or `BW64` header of form `WAVE`
    /// (`SYRP-FORMAT-0006`).
    NotWave,
    /// A chunk the reader needs claims more bytes than the input holds (`SYRP-FORMAT-0007`).
    #[non_exhaustive]
    Truncated {
        /// The chunk's four-character id.
        chunk: [u8; 4],
    },
    /// No `fmt ` chunk, or no `data` chunk, was found (`SYRP-FORMAT-0008`).
    #[non_exhaustive]
    MissingChunk {
        /// The four-character id that is missing.
        chunk: [u8; 4],
    },
    /// The `fmt ` chunk holds impossible values (`SYRP-FORMAT-0009`).
    #[non_exhaustive]
    BadFormat {
        /// Which value is wrong.
        reason: &'static str,
    },
    /// The format tag or sub-format is a codec this crate does not decode (`SYRP-FORMAT-0010`).
    #[non_exhaustive]
    UnsupportedEncoding {
        /// The format tag (for `WAVE_FORMAT_EXTENSIBLE`, the sub-format's first two bytes).
        tag: u16,
    },
    /// A size from the input needs more memory than could be reserved (`SYRP-FORMAT-0011`).
    OutOfMemory,
    /// An `RF64`/`BW64` file has no valid `ds64` chunk first (`SYRP-FORMAT-0012`).
    BadDs64,
}

impl WavError {
    pub(crate) fn truncated(chunk: [u8; 4]) -> Self {
        Self::Truncated { chunk }
    }

    pub(crate) fn bad_format(reason: &'static str) -> Self {
        Self::BadFormat { reason }
    }
}

fn id(chunk: &[u8; 4]) -> String {
    chunk.iter().map(|&b| char::from(b)).collect()
}

impl fmt::Display for WavError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "WAV I/O failed: {e}"),
            Self::NotWave => f.write_str("not a RIFF, RF64 or BW64 WAVE file"),
            Self::Truncated { chunk } => {
                write!(f, "chunk '{}' runs past the end of the input", id(chunk))
            }
            Self::MissingChunk { chunk } => write!(f, "no '{}' chunk", id(chunk)),
            Self::BadFormat { reason } => write!(f, "bad 'fmt ' chunk: {reason}"),
            Self::UnsupportedEncoding { tag } => {
                write!(f, "WAV encoding 0x{tag:04X} is not supported")
            }
            Self::BadDs64 => f.write_str("RF64/BW64 file without a valid 'ds64' chunk"),
            Self::OutOfMemory => f.write_str("not enough memory for a size read from the file"),
        }
    }
}

impl std::error::Error for WavError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for WavError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl Diagnostic for WavError {
    fn code(&self) -> ErrorCode {
        ErrorCode::new(match self {
            Self::Io(_) => "SYRP-FORMAT-0001",
            Self::NotWave => "SYRP-FORMAT-0006",
            Self::Truncated { .. } => "SYRP-FORMAT-0007",
            Self::MissingChunk { .. } => "SYRP-FORMAT-0008",
            Self::BadFormat { .. } => "SYRP-FORMAT-0009",
            Self::UnsupportedEncoding { .. } => "SYRP-FORMAT-0010",
            Self::OutOfMemory => "SYRP-FORMAT-0011",
            Self::BadDs64 => "SYRP-FORMAT-0012",
        })
    }
}

impl From<WavError> for FormatError {
    fn from(e: WavError) -> Self {
        let reason = format!("{}: {e}", e.code().as_str());
        match e {
            WavError::Io(io) => FormatError::Io(io),
            WavError::UnsupportedEncoding { .. } => FormatError::unsupported(reason),
            _ => FormatError::invalid_data(reason),
        }
    }
}

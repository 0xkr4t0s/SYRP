// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`DspError`], catalogued in `errors/dsp.toml`.

use std::fmt;

use syrp_error::{Diagnostic, ErrorCode};

/// The error type of this crate. Each variant carries a code from `errors/dsp.toml`.
///
/// ```
/// use syrp_dsp_core::DspError;
/// use syrp_error::Diagnostic;
/// /// What a host shows for a failed `prepare`.
/// fn report(err: &DspError) -> String {
///     format!("{}: {err}", err.code().as_str())
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DspError {
    /// A buffer sized from the spec could not be allocated (`SYRP-DSP-0001`).
    #[non_exhaustive]
    CannotAllocate {
        /// What the buffer is for.
        what: &'static str,
        /// The number of elements requested.
        len: usize,
    },
}

impl fmt::Display for DspError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::CannotAllocate { what, len } => {
                write!(f, "cannot allocate the {what} ({len} elements)")
            }
        }
    }
}

impl std::error::Error for DspError {}

impl Diagnostic for DspError {
    fn code(&self) -> ErrorCode {
        match self {
            Self::CannotAllocate { .. } => ErrorCode::new("SYRP-DSP-0001"),
        }
    }
}

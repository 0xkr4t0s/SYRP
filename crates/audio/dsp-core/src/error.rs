// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`DspError`], catalogued in `errors/dsp.toml`.

use std::fmt;

use syrp_error::{Diagnostic, ErrorCode};

/// The error type of this crate. Each variant carries a code from `errors/dsp.toml`.
///
/// ```
/// use syrp_audio_api::ProcessConfig;
/// use syrp_dsp_core::{DspError, DspProcessor, DspSpec, LaneProcessor, ProcessorDuplicator};
/// use syrp_error::Diagnostic;
/// # struct Nop; impl LaneProcessor<()> for Nop { type Lanes = syrp_simd::F32x8; fn prepare(&mut self, _: &DspSpec, _: &()) -> Result<(), DspError> { Ok(()) } fn reset(&mut self) {} fn process_lanes(&mut self, _: &(), _: &mut [syrp_simd::F32x8], _: bool) {} }
/// let mut dup = ProcessorDuplicator::new((), || Nop);
/// let err = dup.prepare(&DspSpec::new(ProcessConfig::new(48_000.0, usize::MAX)?, 1)).unwrap_err();
/// assert!(matches!(err, DspError::CannotAllocate { .. }));
/// assert_eq!(err.code().as_str(), "SYRP-DSP-0001");
/// assert!(err.to_string().contains("scratch"));
/// # Ok::<(), syrp_audio_api::AudioError>(())
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
    /// [`Fft::new`](crate::Fft::new) was asked for an order above
    /// [`Fft::MAX_ORDER`](crate::Fft::MAX_ORDER) (`SYRP-DSP-0002`).
    #[non_exhaustive]
    OrderTooLarge {
        /// The requested order.
        order: u32,
        /// The largest supported order.
        max: u32,
    },
}

impl fmt::Display for DspError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::CannotAllocate { what, len } => {
                write!(f, "cannot allocate the {what} ({len} elements)")
            }
            Self::OrderTooLarge { order, max } => {
                write!(
                    f,
                    "FFT order {order} is above the largest supported order {max}"
                )
            }
        }
    }
}

impl std::error::Error for DspError {}

impl Diagnostic for DspError {
    fn code(&self) -> ErrorCode {
        match self {
            Self::CannotAllocate { .. } => ErrorCode::new("SYRP-DSP-0001"),
            Self::OrderTooLarge { .. } => ErrorCode::new("SYRP-DSP-0002"),
        }
    }
}

/// An empty vector with room for exactly `len` elements, or `CannotAllocate`.
pub(crate) fn vec_with_capacity<T>(what: &'static str, len: usize) -> Result<Vec<T>, DspError> {
    let mut v = Vec::new();
    v.try_reserve_exact(len)
        .map_err(|_| DspError::CannotAllocate { what, len })?;
    Ok(v)
}

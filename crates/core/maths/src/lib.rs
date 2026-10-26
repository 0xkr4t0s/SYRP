// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Numeric types for Syrp (master plan §5.2, L2 Core).
//!
//! - [`Range`]: a half-open `start..end` range for integers and floats.
//! - [`NormalisableRange`]: a value range mapped to and from `0..=1` with skew, symmetric skew,
//!   interval snapping and optional custom mappings. Parameters and sliders map through it.
//!
//! Both are plain `Copy` data with pure methods: none of them allocate. The numeric helpers,
//! `Random` and `StatisticsAccumulator` arrive in later increments of WP-1.1-11.
//!
//! ```
//! use syrp_maths::{NormalisableRange, Range};
//! let r = Range::new(0, 10).get_intersection_with(Range::new(5, 20));
//! assert_eq!((r.start(), r.end()), (5, 10));
//! let gain = NormalisableRange::new(-60.0_f32, 6.0)?.with_interval(0.5)?;
//! assert_eq!(gain.snap_to_legal_value(gain.convert_from_0to1(0.5)), -27.0);
//! # Ok::<(), syrp_maths::MathsError>(())
//! ```

use std::fmt;

use syrp_error::{Diagnostic, ErrorCode};

mod float;
mod normalisable;
mod range;

pub use float::Float;
pub use normalisable::{MapFn, NormalisableRange};
pub use range::Range;

/// Errors from building the numeric types in this crate.
///
/// ```
/// use syrp_error::Diagnostic;
/// let err = syrp_maths::NormalisableRange::new(2.0_f32, 1.0).unwrap_err();
/// assert_eq!(err.code().as_str(), "SYRP-MATHS-0001");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MathsError {
    /// A `NormalisableRange` argument is out of its legal domain (`SYRP-MATHS-0001`).
    InvalidRange,
}

impl fmt::Display for MathsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            MathsError::InvalidRange => "invalid NormalisableRange parameters",
        })
    }
}

impl std::error::Error for MathsError {}

impl Diagnostic for MathsError {
    fn code(&self) -> ErrorCode {
        ErrorCode::new(match self {
            MathsError::InvalidRange => "SYRP-MATHS-0001",
        })
    }
}

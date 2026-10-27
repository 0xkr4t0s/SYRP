// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Numeric types for Syrp (master plan §5.2, L2 Core).
//!
//! - [`Range`]: a half-open `start..end` range for integers and floats.
//! - [`NormalisableRange`]: a value range mapped to and from `0..=1` with skew, symmetric skew,
//!   interval snapping and optional custom mappings. Parameters and sliders map through it.
//! - Helpers: [`map_range`], [`map_to_0to1`], [`map_from_log10`], [`map_to_log10`], [`limit`],
//!   [`is_within`], [`approximately_equal`], [`next_power_of_two`], [`is_power_of_two`],
//!   [`round_to_int`] and the [`MathConstants`] for `f32` and `f64`.
//! - [`Random`]: a seedable SplitMix64 generator, and the shared [`system_random`].
//! - [`StatisticsAccumulator`]: running mean, variance, minimum and maximum (Welford).
//! - [`BigInteger`]: an arbitrary-size signed integer with arithmetic and bit operations.
//!
//! Nothing here allocates except [`BigInteger`], [`Random::with_entropy`] and
//! [`system_random`]. `Range`,
//! `NormalisableRange`, `Random` and `StatisticsAccumulator` implement [`syrp_rt::RtSafe`].
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

mod big_integer;
mod float;
mod helpers;
mod normalisable;
mod random;
mod range;
mod statistics;

pub use big_integer::BigInteger;
pub use float::Float;
pub use helpers::{
    MathConstants, PowerOfTwo, Tolerance, approximately_equal, approximately_equal_with,
    is_power_of_two, is_within, limit, map_from_log10, map_range, map_to_0to1, map_to_log10,
    next_power_of_two, round_to_int,
};
pub use normalisable::{MapFn, NormalisableRange};
pub use random::{Random, system_random};
pub use range::Range;
pub use statistics::StatisticsAccumulator;

// SAFETY: two `T` values inline, no destructor beyond `T`'s; every method is arithmetic on
// `T`, which is itself `RtSafe`.
unsafe impl<T: syrp_rt::RtSafe> syrp_rt::RtSafe for Range<T> {}
// SAFETY: `Copy` scalars and `fn` pointers, no destructor; the built-in mappings are
// arithmetic only. Custom `MapFn`s run the caller's code, which must itself be RT-safe for
// the range to be used on an audio thread (documented on `MapFn`).
unsafe impl<T: Float + syrp_rt::RtSafe> syrp_rt::RtSafe for NormalisableRange<T> {}
// SAFETY: one `u64` of state; every method except `with_entropy` (documented as not for the
// audio thread) is integer arithmetic or writes into a caller-provided slice.
unsafe impl syrp_rt::RtSafe for Random {}
// SAFETY: a count, two `f64` and two `Option<T>` inline, no destructor; `add` and the
// accessors are arithmetic only.
unsafe impl<T: Float + syrp_rt::RtSafe> syrp_rt::RtSafe for StatisticsAccumulator<T> {}

/// Errors from building or parsing the numeric types in this crate.
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
    /// A `BigInteger` text base other than 2, 8, 10 or 16, or text that is not a number in
    /// that base (`SYRP-MATHS-0002`).
    InvalidBaseOrDigit,
}

impl fmt::Display for MathsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            MathsError::InvalidRange => "invalid NormalisableRange parameters",
            MathsError::InvalidBaseOrDigit => {
                "unsupported base or invalid digit in BigInteger text"
            }
        })
    }
}

impl std::error::Error for MathsError {}

impl Diagnostic for MathsError {
    fn code(&self) -> ErrorCode {
        ErrorCode::new(match self {
            MathsError::InvalidRange => "SYRP-MATHS-0001",
            MathsError::InvalidBaseOrDigit => "SYRP-MATHS-0002",
        })
    }
}

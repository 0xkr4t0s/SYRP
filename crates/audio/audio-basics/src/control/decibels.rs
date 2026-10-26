// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Gain and decibel conversion (spec B1), in the role of JUCE's `Decibels`.
//!
//! A gain of `1.0` is 0 dB. A floor, `minus_infinity_db`, stands for silence: gains at or
//! below the floor's gain (and zero, negative or NaN gains) convert to the floor, and decibel
//! values at or below the floor convert back to a gain of exactly `0.0`. JUCE's documented
//! default floor is −100 dB ([`DecibelFloat::DEFAULT_MINUS_INFINITY_DB`]).
//!
//! The functions take `f32` or `f64`. [`Sample`](syrp_simd::Sample) has no logarithm, so lane
//! packs convert lane by lane through `to_array` and `from_array`.
//!
//! ```
//! use syrp_audio_basics::control::decibels::{decibels_to_gain, gain_to_decibels};
//! use syrp_simd::F32x4;
//! let gains = F32x4::from_array([1.0, 0.5, 0.0, 2.0]);
//! let db = F32x4::from_array(gains.to_array().map(|g| gain_to_decibels(g, -100.0)));
//! assert_eq!(db.lane(0), 0.0);
//! assert_eq!(db.lane(2), -100.0);
//! assert_eq!(decibels_to_gain(db.lane(2), -100.0), 0.0);
//! ```

use core::fmt::Display;

/// The text [`to_string`] callers pass to get JUCE's documented default for silence.
///
/// ```
/// use syrp_audio_basics::control::decibels::{MINUS_INFINITY_TEXT, to_string};
/// assert_eq!(to_string(-200.0_f32, 1, MINUS_INFINITY_TEXT), "-INF");
/// ```
pub const MINUS_INFINITY_TEXT: &str = "-INF";

/// The float types the decibel functions take: `f32` and `f64`. The trait is sealed.
///
/// ```
/// use syrp_audio_basics::control::decibels::{DecibelFloat, gain_to_decibels};
/// fn to_db<F: DecibelFloat>(gain: F) -> F {
///     gain_to_decibels(gain, F::DEFAULT_MINUS_INFINITY_DB)
/// }
/// assert_eq!(to_db(0.0_f64), -100.0);
/// ```
pub trait DecibelFloat: Copy + PartialOrd + Display + private::Float {
    /// The default silence floor, −100 dB.
    const DEFAULT_MINUS_INFINITY_DB: Self;
}

impl DecibelFloat for f32 {
    const DEFAULT_MINUS_INFINITY_DB: Self = -100.0;
}

impl DecibelFloat for f64 {
    const DEFAULT_MINUS_INFINITY_DB: Self = -100.0;
}

mod private {
    /// The arithmetic behind [`DecibelFloat`](super::DecibelFloat).
    pub trait Float: Copy {
        const ZERO: Self;
        /// `20 * log10(self)`.
        fn to_db(self) -> Self;
        /// `10^(self / 20)`.
        fn to_gain(self) -> Self;
    }

    impl Float for f32 {
        const ZERO: Self = 0.0;
        #[inline(always)]
        fn to_db(self) -> Self {
            20.0 * self.log10()
        }
        #[inline(always)]
        fn to_gain(self) -> Self {
            10.0_f32.powf(self / 20.0)
        }
    }

    impl Float for f64 {
        const ZERO: Self = 0.0;
        #[inline(always)]
        fn to_db(self) -> Self {
            20.0 * self.log10()
        }
        #[inline(always)]
        fn to_gain(self) -> Self {
            10.0_f64.powf(self / 20.0)
        }
    }
}

/// Converts a gain to decibels: `20 * log10(gain)`, but never below `minus_infinity_db`.
/// Zero, negative and NaN gains give `minus_infinity_db`.
///
/// ```
/// use syrp_audio_basics::control::decibels::gain_to_decibels;
/// assert_eq!(gain_to_decibels(1.0_f32, -100.0), 0.0);
/// assert!((gain_to_decibels(0.5_f64, -100.0) + 6.0206).abs() < 1e-4);
/// assert_eq!(gain_to_decibels(1e-9_f64, -100.0), -100.0);
/// ```
#[doc(alias = "Decibels::gainToDecibels")]
#[inline(always)]
pub fn gain_to_decibels<F: DecibelFloat>(gain: F, minus_infinity_db: F) -> F {
    if gain > F::ZERO {
        let db = gain.to_db();
        if db > minus_infinity_db {
            return db;
        }
    }
    minus_infinity_db
}

/// Converts decibels to a gain: `10^(db / 20)`, or exactly `0.0` when `db` is at or below
/// `minus_infinity_db` (or NaN).
///
/// ```
/// use syrp_audio_basics::control::decibels::decibels_to_gain;
/// assert_eq!(decibels_to_gain(0.0_f32, -100.0), 1.0);
/// assert_eq!(decibels_to_gain(20.0_f64, -100.0), 10.0);
/// assert_eq!(decibels_to_gain(-100.0_f32, -100.0), 0.0);
/// ```
#[doc(alias = "Decibels::decibelsToGain")]
#[inline(always)]
pub fn decibels_to_gain<F: DecibelFloat>(db: F, minus_infinity_db: F) -> F {
    if db > minus_infinity_db {
        db.to_gain()
    } else {
        F::ZERO
    }
}

/// Formats decibels for display with `decimal_places` digits after the point and a `" dB"`
/// suffix. Values at or below the default floor (−100 dB), and NaN, give
/// `minus_infinity_text` instead; pass [`MINUS_INFINITY_TEXT`] for JUCE's default, `"-INF"`.
///
/// The cutoff is always [`DecibelFloat::DEFAULT_MINUS_INFINITY_DB`], as the spec's signature
/// has no floor parameter. A caller that converts with another floor and wants that floor
/// shown as silence checks `db <= floor` itself before formatting.
///
/// This allocates the returned `String`; do not call it on the audio thread.
///
/// ```
/// use syrp_audio_basics::control::decibels::{MINUS_INFINITY_TEXT, to_string};
/// assert_eq!(to_string(-6.0206_f32, 2, MINUS_INFINITY_TEXT), "-6.02 dB");
/// assert_eq!(to_string(3.0_f64, 0, MINUS_INFINITY_TEXT), "3 dB");
/// assert_eq!(to_string(-100.0_f64, 2, "silent"), "silent");
/// assert_eq!(to_string(f32::NAN, 2, MINUS_INFINITY_TEXT), "-INF");
/// ```
#[doc(alias = "Decibels::toString")]
pub fn to_string<F: DecibelFloat>(
    db: F,
    decimal_places: usize,
    minus_infinity_text: &str,
) -> String {
    // NaN compares as unordered, so it takes this branch too.
    if db.partial_cmp(&F::DEFAULT_MINUS_INFINITY_DB) != Some(core::cmp::Ordering::Greater) {
        return minus_infinity_text.to_owned();
    }
    format!("{db:.decimal_places$} dB")
}

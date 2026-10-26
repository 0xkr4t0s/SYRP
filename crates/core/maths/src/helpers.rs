//! Numeric helpers and constants (spec B3).

use crate::Float;

mod sealed {
    pub trait Sealed {}
    impl Sealed for u32 {}
    impl Sealed for u64 {}
    impl Sealed for usize {}
    impl Sealed for i32 {}
    impl Sealed for i64 {}
}

/// Mathematical constants for `f32` and `f64`.
///
/// ```
/// use syrp_maths::MathConstants;
/// assert_eq!(<f64 as MathConstants>::TWO_PI, std::f64::consts::TAU);
/// assert_eq!(<f32 as MathConstants>::SQRT2, std::f32::consts::SQRT_2);
/// ```
pub trait MathConstants: Sized {
    /// π.
    ///
    /// ```
    /// use syrp_maths::MathConstants;
    /// assert!((<f64 as MathConstants>::PI - 3.14159265).abs() < 1e-8);
    /// ```
    const PI: Self;
    /// 2π.
    ///
    /// ```
    /// use syrp_maths::MathConstants;
    /// assert_eq!(<f32 as MathConstants>::TWO_PI, 2.0 * std::f32::consts::PI);
    /// ```
    const TWO_PI: Self;
    /// π / 2.
    ///
    /// ```
    /// use syrp_maths::MathConstants;
    /// assert_eq!(<f64 as MathConstants>::HALF_PI, std::f64::consts::PI / 2.0);
    /// ```
    const HALF_PI: Self;
    /// Euler's number e, the base of natural logarithms.
    ///
    /// ```
    /// use syrp_maths::MathConstants;
    /// assert!((<f64 as MathConstants>::EULER.ln() - 1.0).abs() < 1e-15);
    /// ```
    const EULER: Self;
    /// The square root of 2.
    ///
    /// ```
    /// use syrp_maths::MathConstants;
    /// let s = <f64 as MathConstants>::SQRT2;
    /// assert!((s * s - 2.0).abs() < 1e-15);
    /// ```
    const SQRT2: Self;
}

impl MathConstants for f32 {
    const PI: Self = std::f32::consts::PI;
    const TWO_PI: Self = std::f32::consts::TAU;
    const HALF_PI: Self = std::f32::consts::FRAC_PI_2;
    const EULER: Self = std::f32::consts::E;
    const SQRT2: Self = std::f32::consts::SQRT_2;
}

impl MathConstants for f64 {
    const PI: Self = std::f64::consts::PI;
    const TWO_PI: Self = std::f64::consts::TAU;
    const HALF_PI: Self = std::f64::consts::FRAC_PI_2;
    const EULER: Self = std::f64::consts::E;
    const SQRT2: Self = std::f64::consts::SQRT_2;
}

/// Maps `value` linearly from `source_min..source_max` to `target_min..target_max`.
///
/// Values outside the source range extrapolate; nothing is clamped. A zero-width source range
/// divides by zero (infinity or NaN), as the arithmetic implies.
///
/// ```
/// assert_eq!(syrp_maths::map_range(5.0, 0.0, 10.0, 100.0, 200.0), 150.0);
/// assert_eq!(syrp_maths::map_range(20.0_f32, 0.0, 10.0, 0.0, 1.0), 2.0);
/// ```
#[doc(alias = "jmap")]
pub fn map_range<T: Float>(
    value: T,
    source_min: T,
    source_max: T,
    target_min: T,
    target_max: T,
) -> T {
    target_min + (target_max - target_min) * (value - source_min) / (source_max - source_min)
}

/// Maps `value` from `min..max` to `0..1` (unclamped).
///
/// ```
/// assert_eq!(syrp_maths::map_to_0to1(15.0, 10.0, 20.0), 0.5);
/// ```
#[doc(alias = "jmap")]
pub fn map_to_0to1<T: Float>(value: T, min: T, max: T) -> T {
    (value - min) / (max - min)
}

/// Maps a normalised `value` in `0..1` onto the logarithmic range `log_min..log_max`.
///
/// 0 gives `log_min`, 1 gives `log_max`, and equal steps of `value` multiply the result by
/// equal factors. Both bounds must be positive.
///
/// ```
/// let v = syrp_maths::map_from_log10(0.5_f64, 10.0, 1000.0);
/// assert!((v - 100.0).abs() < 1e-9);
/// ```
#[doc(alias = "mapFromLog10")]
pub fn map_from_log10<T: Float>(value: T, log_min: T, log_max: T) -> T {
    let (lo, hi) = (log_min.to_f64().log10(), log_max.to_f64().log10());
    T::from_f64(10_f64.powf(lo + (hi - lo) * value.to_f64()))
}

/// Maps `value` in the logarithmic range `log_min..log_max` to `0..1`; the inverse of
/// [`map_from_log10`]. All three arguments must be positive.
///
/// ```
/// let p = syrp_maths::map_to_log10(100.0_f64, 10.0, 1000.0);
/// assert!((p - 0.5).abs() < 1e-12);
/// ```
#[doc(alias = "mapToLog10")]
pub fn map_to_log10<T: Float>(value: T, log_min: T, log_max: T) -> T {
    let (lo, hi) = (log_min.to_f64().log10(), log_max.to_f64().log10());
    T::from_f64((value.to_f64().log10() - lo) / (hi - lo))
}

/// Clamps `value` to `lower..=upper`. `lower` must not be greater than `upper`.
///
/// Argument order follows JUCE's `jlimit` so ported calls keep their shape.
///
/// ```
/// assert_eq!(syrp_maths::limit(0, 10, 15), 10);
/// assert_eq!(syrp_maths::limit(0.0, 1.0, -0.5), 0.0);
/// assert_eq!(syrp_maths::limit(0, 10, 4), 4);
/// ```
#[doc(alias = "jlimit")]
pub fn limit<T: PartialOrd>(lower: T, upper: T, value: T) -> T {
    debug_assert!(lower <= upper, "limit: lower bound is above upper bound");
    if value < lower {
        lower
    } else if value > upper {
        upper
    } else {
        value
    }
}

/// True when `a` and `b` differ by at most `tolerance`.
///
/// ```
/// assert!(syrp_maths::is_within(1.0, 1.05, 0.1));
/// assert!(!syrp_maths::is_within(1.0, 1.2, 0.1));
/// ```
#[doc(alias = "isWithin")]
pub fn is_within<T: Float>(a: T, b: T, tolerance: T) -> bool {
    let diff = if a > b { a - b } else { b - a };
    diff <= tolerance
}

/// Absolute and relative tolerances for [`approximately_equal_with`].
///
/// The default is an absolute tolerance of the smallest positive normal value and a relative
/// tolerance of machine epsilon.
///
/// ```
/// use syrp_maths::Tolerance;
/// let t = Tolerance::<f64>::default().with_absolute(1e-6).with_relative(1e-3);
/// assert_eq!((t.absolute(), t.relative()), (1e-6, 1e-3));
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tolerance<T> {
    absolute: T,
    relative: T,
}

impl<T: Float> Default for Tolerance<T> {
    fn default() -> Self {
        Self {
            absolute: T::MIN_POSITIVE,
            relative: T::EPSILON,
        }
    }
}

impl<T: Float> Tolerance<T> {
    /// Returns a copy with the absolute tolerance set to `absolute`.
    ///
    /// ```
    /// let t = syrp_maths::Tolerance::<f32>::default().with_absolute(0.01);
    /// assert_eq!(t.absolute(), 0.01);
    /// ```
    #[must_use]
    pub fn with_absolute(self, absolute: T) -> Self {
        Self { absolute, ..self }
    }

    /// Returns a copy with the relative tolerance set to `relative`.
    ///
    /// ```
    /// let t = syrp_maths::Tolerance::<f32>::default().with_relative(0.5);
    /// assert_eq!(t.relative(), 0.5);
    /// ```
    #[must_use]
    pub fn with_relative(self, relative: T) -> Self {
        Self { relative, ..self }
    }

    /// The absolute tolerance.
    ///
    /// ```
    /// assert_eq!(syrp_maths::Tolerance::<f64>::default().absolute(), f64::MIN_POSITIVE);
    /// ```
    pub fn absolute(self) -> T {
        self.absolute
    }

    /// The relative tolerance, a fraction of the larger magnitude.
    ///
    /// ```
    /// assert_eq!(syrp_maths::Tolerance::<f64>::default().relative(), f64::EPSILON);
    /// ```
    pub fn relative(self) -> T {
        self.relative
    }
}

/// [`approximately_equal_with`] using [`Tolerance::default`].
///
/// ```
/// assert!(syrp_maths::approximately_equal(0.1 + 0.2, 0.3));
/// assert!(!syrp_maths::approximately_equal(1.0, 1.0001));
/// ```
#[doc(alias = "approximatelyEqual")]
pub fn approximately_equal<T: Float>(a: T, b: T) -> bool {
    approximately_equal_with(a, b, Tolerance::default())
}

/// True when `a` and `b` are equal within `tolerance`.
///
/// Non-finite values compare with `==` (so two infinities of the same sign are equal and NaN
/// equals nothing). Otherwise the difference must be at most the absolute tolerance, or at most
/// the relative tolerance times the larger of `|a|` and `|b|`.
///
/// ```
/// use syrp_maths::{approximately_equal_with, Tolerance};
/// let t = Tolerance::default().with_relative(0.01);
/// assert!(approximately_equal_with(100.0, 100.5, t));
/// assert!(!approximately_equal_with(100.0, 102.0, t));
/// let t = Tolerance::default().with_absolute(0.1);
/// assert!(approximately_equal_with(0.0, 0.05, t));
/// ```
pub fn approximately_equal_with<T: Float>(a: T, b: T, tolerance: Tolerance<T>) -> bool {
    let (a, b) = (a.to_f64(), b.to_f64());
    if !a.is_finite() || !b.is_finite() {
        return a == b;
    }
    let diff = (a - b).abs();
    diff <= tolerance.absolute.to_f64()
        || diff <= tolerance.relative.to_f64() * a.abs().max(b.abs())
}

/// Integer power-of-two queries for `u32`, `u64`, `usize`, `i32` and `i64`.
///
/// ```
/// use syrp_maths::PowerOfTwo;
/// assert_eq!(5_u32.next_power_of_two_checked(), Some(8));
/// assert!(64_i64.is_power_of_two());
/// ```
pub trait PowerOfTwo: sealed::Sealed + Sized {
    /// True when the value is a positive power of two (1, 2, 4, ...).
    ///
    /// ```
    /// use syrp_maths::PowerOfTwo;
    /// assert!(!0_u32.is_power_of_two());
    /// assert!(!(-4_i32).is_power_of_two());
    /// ```
    // Implemented only for `Copy` integers, where taking `self` by value is the norm.
    #[allow(clippy::wrong_self_convention)]
    fn is_power_of_two(self) -> bool;
    /// The smallest power of two that is at least the value (1 for zero or negatives), or
    /// `None` if that does not fit in the type.
    ///
    /// ```
    /// use syrp_maths::PowerOfTwo;
    /// assert_eq!(0_i32.next_power_of_two_checked(), Some(1));
    /// assert_eq!(i32::MAX.next_power_of_two_checked(), None);
    /// ```
    fn next_power_of_two_checked(self) -> Option<Self>;
}

macro_rules! power_of_two_unsigned {
    ($($t:ty),*) => {$(
        impl PowerOfTwo for $t {
            fn is_power_of_two(self) -> bool { <$t>::is_power_of_two(self) }
            fn next_power_of_two_checked(self) -> Option<Self> { self.checked_next_power_of_two() }
        }
    )*};
}
// A macro because the unsigned impls are identical apart from the type.
power_of_two_unsigned!(u32, u64, usize);

macro_rules! power_of_two_signed {
    ($($t:ty),*) => {$(
        impl PowerOfTwo for $t {
            fn is_power_of_two(self) -> bool { self > 0 && self.cast_unsigned().is_power_of_two() }
            fn next_power_of_two_checked(self) -> Option<Self> {
                let n = self.max(1).cast_unsigned().checked_next_power_of_two()?;
                <$t>::try_from(n).ok()
            }
        }
    )*};
}
// A macro for the same reason, for the signed types.
power_of_two_signed!(i32, i64);

/// The smallest power of two that is at least `n` (1 for zero or negatives), or `None` on
/// overflow. See [`PowerOfTwo`].
///
/// ```
/// assert_eq!(syrp_maths::next_power_of_two(1000_u32), Some(1024));
/// assert_eq!(syrp_maths::next_power_of_two(1024_i32), Some(1024));
/// ```
#[doc(alias = "nextPowerOfTwo")]
pub fn next_power_of_two<T: PowerOfTwo>(n: T) -> Option<T> {
    n.next_power_of_two_checked()
}

/// True when `n` is a positive power of two. See [`PowerOfTwo`].
///
/// ```
/// assert!(syrp_maths::is_power_of_two(512_usize));
/// assert!(!syrp_maths::is_power_of_two(12_u64));
/// ```
#[doc(alias = "isPowerOfTwo")]
pub fn is_power_of_two<T: PowerOfTwo>(n: T) -> bool {
    n.is_power_of_two()
}

/// Rounds to the nearest `i32`, halves away from zero; saturates at the `i32` limits and maps
/// NaN to 0.
///
/// ```
/// assert_eq!(syrp_maths::round_to_int(2.5_f32), 3);
/// assert_eq!(syrp_maths::round_to_int(-2.5_f64), -3);
/// assert_eq!(syrp_maths::round_to_int(1e20_f64), i32::MAX);
/// ```
#[doc(alias = "roundToInt")]
#[allow(clippy::cast_possible_truncation)]
pub fn round_to_int<T: Float>(value: T) -> i32 {
    value.to_f64().round() as i32
}

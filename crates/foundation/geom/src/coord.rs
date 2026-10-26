//! The [`Coord`] trait: the scalar types geometry can be built from.

use core::fmt::Debug;
use core::ops::{Add, Div, Mul, Neg, Sub};

mod sealed {
    pub trait Sealed {}
    impl Sealed for i32 {}
    impl Sealed for f32 {}
    impl Sealed for f64 {}
}

/// A scalar coordinate type: `i32`, `f32` or `f64`.
///
/// The trait is sealed so that new methods can be added without breaking anyone.
/// Conversions from `f64` saturate for `i32` (NaN becomes 0).
/// ```
/// use syrp_geom::Coord;
/// assert_eq!(<i32 as Coord>::from_f64(2.5), 3);
/// assert_eq!(<f32 as Coord>::from_f64(2.5), 2.5);
/// assert_eq!(7_i32.half(), 3);
/// ```
pub trait Coord:
    sealed::Sealed
    + Copy
    + PartialOrd
    + Default
    + Debug
    + Add<Output = Self>
    + Sub<Output = Self>
    + Mul<Output = Self>
    + Div<Output = Self>
    + Neg<Output = Self>
    + 'static
{
    /// The additive identity.
    const ZERO: Self;

    /// `true` for integer coordinate types, whose conversions from `f64` round.
    const INTEGER: bool;

    /// Widens the value to `f64` (exact for every supported type except very large `f64`s, which pass through).
    fn to_f64(self) -> f64;

    /// Converts from `f64`, rounding to nearest (halves away from zero) for integers.
    fn from_f64(v: f64) -> Self;

    /// Converts from `f64`, rounding towards negative infinity for integers.
    fn floor_from_f64(v: f64) -> Self {
        Self::from_f64(v.floor())
    }

    /// Converts from `f64`, rounding towards positive infinity for integers.
    fn ceil_from_f64(v: f64) -> Self {
        Self::from_f64(v.ceil())
    }

    /// Half of the value; integers divide towards zero.
    fn half(self) -> Self;

    /// The smaller of two values (the first if they compare equal or unordered).
    fn min_of(self, other: Self) -> Self {
        if other < self { other } else { self }
    }

    /// The larger of two values (the first if they compare equal or unordered).
    fn max_of(self, other: Self) -> Self {
        if other > self { other } else { self }
    }
}

impl Coord for i32 {
    const ZERO: Self = 0;
    const INTEGER: bool = true;
    fn to_f64(self) -> f64 {
        f64::from(self)
    }
    fn from_f64(v: f64) -> Self {
        // `as` saturates and maps NaN to 0, which is the documented behaviour.
        v.round() as i32
    }
    fn half(self) -> Self {
        self / 2
    }
}

impl Coord for f32 {
    const ZERO: Self = 0.0;
    const INTEGER: bool = false;
    fn to_f64(self) -> f64 {
        f64::from(self)
    }
    fn from_f64(v: f64) -> Self {
        v as f32
    }
    fn floor_from_f64(v: f64) -> Self {
        Self::from_f64(v)
    }
    fn ceil_from_f64(v: f64) -> Self {
        Self::from_f64(v)
    }
    fn half(self) -> Self {
        self * 0.5
    }
}

impl Coord for f64 {
    const ZERO: Self = 0.0;
    const INTEGER: bool = false;
    fn to_f64(self) -> f64 {
        self
    }
    fn from_f64(v: f64) -> Self {
        v
    }
    fn floor_from_f64(v: f64) -> Self {
        Self::from_f64(v)
    }
    fn ceil_from_f64(v: f64) -> Self {
        Self::from_f64(v)
    }
    fn half(self) -> Self {
        self * 0.5
    }
}

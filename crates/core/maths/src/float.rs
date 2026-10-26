//! The `Float` scalar trait shared by the float APIs.

use std::ops::{Add, Div, Mul, Sub};

mod sealed {
    pub trait Sealed {}
    impl Sealed for f32 {}
    impl Sealed for f64 {}
}

/// The floating-point types this crate's float APIs accept: `f32` and `f64`.
///
/// The trait is sealed. Mappings with powers and logarithms are computed in `f64` and
/// converted back, so `f32` results are rounded once, at the end.
///
/// ```
/// use syrp_maths::Float;
/// fn halve<T: Float>(x: T) -> T { T::from_f64(x.to_f64() / 2.0) }
/// assert_eq!(halve(3.0_f32), 1.5);
/// ```
pub trait Float:
    sealed::Sealed
    + Copy
    + PartialOrd
    + Default
    + std::fmt::Debug
    + Add<Output = Self>
    + Sub<Output = Self>
    + Mul<Output = Self>
    + Div<Output = Self>
{
    /// The difference between 1 and the next larger representable value.
    ///
    /// ```
    /// use syrp_maths::Float;
    /// assert_eq!(<f32 as Float>::EPSILON, f32::EPSILON);
    /// ```
    const EPSILON: Self;

    /// The smallest positive normal value.
    ///
    /// ```
    /// use syrp_maths::Float;
    /// assert_eq!(<f64 as Float>::MIN_POSITIVE, f64::MIN_POSITIVE);
    /// ```
    const MIN_POSITIVE: Self;

    /// Converts to `f64` exactly.
    ///
    /// ```
    /// use syrp_maths::Float;
    /// assert_eq!(0.5_f32.to_f64(), 0.5);
    /// ```
    fn to_f64(self) -> f64;

    /// Converts from `f64`, rounding to nearest for `f32`.
    ///
    /// ```
    /// use syrp_maths::Float;
    /// assert_eq!(f32::from_f64(0.25), 0.25_f32);
    /// ```
    fn from_f64(value: f64) -> Self;
}

impl Float for f32 {
    const EPSILON: Self = f32::EPSILON;
    const MIN_POSITIVE: Self = f32::MIN_POSITIVE;
    fn to_f64(self) -> f64 {
        f64::from(self)
    }
    #[allow(clippy::cast_possible_truncation)]
    fn from_f64(value: f64) -> Self {
        value as f32
    }
}

impl Float for f64 {
    const EPSILON: Self = f64::EPSILON;
    const MIN_POSITIVE: Self = f64::MIN_POSITIVE;
    fn to_f64(self) -> f64 {
        self
    }
    fn from_f64(value: f64) -> Self {
        value
    }
}

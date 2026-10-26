// SPDX-License-Identifier: MIT OR Apache-2.0
//! The [`Sample`] trait, its [`Mask`] companion, and the lane packs.
//!
//! Every operation is lane-wise. Packs keep their lanes private (ADR 0003 D3) and are moved in
//! and out with `from_array`, `to_array` and `lane`. Pack methods are `#[inline(always)]`; a
//! lane-generic function is compiled for the level selected by [`dispatch!`](crate::dispatch)
//! only when it, and every function between it and the `dispatch!` body, is
//! `#[inline(always)]` too (see the crate docs, "The inlining rule").

use core::fmt::Debug;
use core::ops::{
    Add, AddAssign, BitAnd, BitOr, Div, DivAssign, Mul, MulAssign, Neg, Not, Sub, SubAssign,
};

mod sealed {
    pub trait Sealed {}
}
pub(crate) use sealed::Sealed;

/// A lane mask produced by [`Sample`] comparisons and consumed by [`Sample::select`].
///
/// `bool` is the mask of the scalar samples; each pack has its own mask type.
///
/// ```
/// use syrp_simd::{F32x4, Mask, Sample};
/// let m = F32x4::from_array([1.0, 5.0, 2.0, 7.0]).simd_gt(F32x4::splat(3.0));
/// assert!(m.any() && !m.all());
/// assert!((m & !m).none());
/// ```
pub trait Mask:
    Copy
    + Send
    + Sync
    + 'static
    + Debug
    + PartialEq
    + BitAnd<Output = Self>
    + BitOr<Output = Self>
    + Not<Output = Self>
    + Sealed
{
    /// A mask with every lane set to `value`.
    fn splat(value: bool) -> Self;
    /// True when at least one lane is set.
    fn any(self) -> bool;
    /// True when every lane is set.
    fn all(self) -> bool;
    /// True when no lane is set.
    #[inline(always)]
    fn none(self) -> bool {
        !self.any()
    }
}

impl Sealed for bool {}
impl Mask for bool {
    #[inline(always)]
    fn splat(value: bool) -> Self {
        value
    }
    #[inline(always)]
    fn any(self) -> bool {
        self
    }
    #[inline(always)]
    fn all(self) -> bool {
        self
    }
}

/// A sample type that DSP code is written against once: `f32`, `f64` or a lane pack.
///
/// A struct generic over `S: Sample` is its own lane pack: `Voice<F32x8>` is eight voices in
/// struct-of-arrays form. Branches are written with masks and [`select`](Sample::select).
/// Mark lane-generic functions `#[inline(always)]` and call them from a
/// [`dispatch!`](crate::dispatch) body; otherwise they run at the baseline level (see the
/// crate docs, "The inlining rule").
/// The trait is sealed; the implementors are `f32`, `f64`, [`F32x4`], [`F32x8`], [`F64x2`]
/// and [`F64x4`].
///
/// ```
/// use syrp_simd::{F32x8, Sample};
/// /// Hard clip written once for every sample type.
/// #[inline(always)]
/// fn clip<S: Sample>(x: S, limit: S) -> S {
///     S::select(x.abs().simd_gt(limit), limit.min(x.max(-limit)), x)
/// }
/// assert_eq!(clip(2.0_f32, 1.0), 1.0);
/// let y = clip(F32x8::from_array([-3.0, 0.5, 9.0, 0.0, 1.0, -1.0, 0.25, 4.0]), F32x8::one());
/// assert_eq!(y.to_array(), [-1.0, 0.5, 1.0, 0.0, 1.0, -1.0, 0.25, 1.0]);
/// ```
pub trait Sample:
    Copy
    + Send
    + Sync
    + 'static
    + Debug
    + Default
    + PartialEq
    + Add<Output = Self>
    + Sub<Output = Self>
    + Mul<Output = Self>
    + Div<Output = Self>
    + Neg<Output = Self>
    + AddAssign
    + SubAssign
    + MulAssign
    + DivAssign
    + Sealed
{
    /// The scalar type of one lane (`f32` or `f64`).
    type Element: Sample<Element = Self::Element>;
    /// The mask type returned by comparisons.
    type Mask: Mask;
    /// Number of lanes: 1 for `f32` and `f64`.
    const LANES: usize;

    /// Every lane set to `value`.
    fn splat(value: Self::Element) -> Self;
    /// Every lane `0.0`.
    #[inline(always)]
    fn zero() -> Self {
        Self::default()
    }
    /// Every lane `1.0`.
    fn one() -> Self;
    /// `self * a + b` with a single rounding (fused), the same result as `f32::mul_add`
    /// on every dispatch level, whether or not the CPU has an FMA instruction. Without hardware
    /// FMA (`Sse2`, `Sse4_2`, or code not inlined into a `dispatch!` body) this is a software
    /// `fma` call per lane and slow; write `self * a + b` where fusion is not needed.
    fn mul_add(self, a: Self, b: Self) -> Self;
    /// Lane-wise minimum; a NaN lane yields the other operand (as `f32::min`).
    fn min(self, other: Self) -> Self;
    /// Lane-wise maximum; a NaN lane yields the other operand (as `f32::max`).
    fn max(self, other: Self) -> Self;
    /// Lane-wise absolute value.
    fn abs(self) -> Self;
    /// Lane-wise square root (correctly rounded).
    fn sqrt(self) -> Self;
    /// Lanes where `self == other`.
    fn simd_eq(self, other: Self) -> Self::Mask;
    /// Lanes where `self < other`.
    fn simd_lt(self, other: Self) -> Self::Mask;
    /// Lanes where `self <= other`.
    fn simd_le(self, other: Self) -> Self::Mask;
    /// Lanes where `self > other`.
    fn simd_gt(self, other: Self) -> Self::Mask;
    /// Lanes where `self >= other`.
    fn simd_ge(self, other: Self) -> Self::Mask;
    /// Per lane, `if_true` where `mask` is set and `if_false` elsewhere.
    fn select(mask: Self::Mask, if_true: Self, if_false: Self) -> Self;
}

// Macro justification: the scalar and pack implementations differ only in the element type,
// lane count and alignment; generating them keeps the six types in lockstep. Bodies are kept
// one line per method.
#[rustfmt::skip]
macro_rules! scalar_sample {
    ($t:ty) => {
        impl Sealed for $t {}
        impl Sample for $t {
            type Element = $t;
            type Mask = bool;
            const LANES: usize = 1;
            #[inline(always)] fn splat(value: $t) -> Self { value }
            #[inline(always)] fn one() -> Self { 1.0 }
            #[inline(always)] fn mul_add(self, a: Self, b: Self) -> Self { <$t>::mul_add(self, a, b) }
            #[inline(always)] fn min(self, other: Self) -> Self { <$t>::min(self, other) }
            #[inline(always)] fn max(self, other: Self) -> Self { <$t>::max(self, other) }
            #[inline(always)] fn abs(self) -> Self { <$t>::abs(self) }
            #[inline(always)] fn sqrt(self) -> Self { <$t>::sqrt(self) }
            #[inline(always)] fn simd_eq(self, other: Self) -> bool { self == other }
            #[inline(always)] fn simd_lt(self, other: Self) -> bool { self < other }
            #[inline(always)] fn simd_le(self, other: Self) -> bool { self <= other }
            #[inline(always)] fn simd_gt(self, other: Self) -> bool { self > other }
            #[inline(always)] fn simd_ge(self, other: Self) -> bool { self >= other }
            #[inline(always)] fn select(mask: bool, if_true: Self, if_false: Self) -> Self { if mask { if_true } else { if_false } }
        }
    };
}
scalar_sample!(f32);
scalar_sample!(f64);

/// Applies `$f` lane by lane to the arrays of `$a` and `$b`.
#[rustfmt::skip]
macro_rules! zip {
    ($a:expr, $b:expr, $f:expr) => {{
        let (a, b) = ($a.0, $b.0);
        core::array::from_fn(|i| $f(a[i], b[i]))
    }};
}

#[rustfmt::skip]
macro_rules! binop {
    ($pack:ident, $tr:ident, $f:ident, $atr:ident, $af:ident, $op:tt) => {
        impl $tr for $pack {
            type Output = Self;
            #[inline(always)] fn $f(self, rhs: Self) -> Self { Self(zip!(self, rhs, |a, b| a $op b)) }
        }
        impl $atr for $pack {
            #[inline(always)] fn $af(&mut self, rhs: Self) { *self = *self $op rhs; }
        }
    };
}

#[rustfmt::skip]
macro_rules! pack {
    ($(#[$doc:meta])* $pack:ident, $mask:ident, $t:ty, $n:literal, $align:literal) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, Default, PartialEq)]
        #[repr(C, align($align))]
        pub struct $pack([$t; $n]);

        #[doc = concat!("The lane mask of [`", stringify!($pack), "`]: one flag per lane.")]
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
        pub struct $mask([bool; $n]);

        impl $pack {
            #[doc = concat!("A pack from its ", stringify!($n), " lanes.")]
            #[inline(always)] pub const fn from_array(lanes: [$t; $n]) -> Self { Self(lanes) }
            /// The lanes as an array.
            #[inline(always)] pub const fn to_array(self) -> [$t; $n] { self.0 }
            /// The value of lane `i`; panics when `i >= LANES`.
            #[inline(always)] pub const fn lane(self, i: usize) -> $t { self.0[i] }
        }

        impl $mask {
            /// A mask from one flag per lane.
            #[inline(always)] pub const fn from_array(lanes: [bool; $n]) -> Self { Self(lanes) }
            /// The flags as an array.
            #[inline(always)] pub const fn to_array(self) -> [bool; $n] { self.0 }
            /// Whether lane `i` is set; panics when `i >= LANES`.
            #[inline(always)] pub const fn lane(self, i: usize) -> bool { self.0[i] }
        }

        binop!($pack, Add, add, AddAssign, add_assign, +);
        binop!($pack, Sub, sub, SubAssign, sub_assign, -);
        binop!($pack, Mul, mul, MulAssign, mul_assign, *);
        binop!($pack, Div, div, DivAssign, div_assign, /);
        impl Neg for $pack {
            type Output = Self;
            #[inline(always)] fn neg(self) -> Self { Self(self.0.map(|x| -x)) }
        }
        impl BitAnd for $mask {
            type Output = Self;
            #[inline(always)] fn bitand(self, rhs: Self) -> Self { Self(zip!(self, rhs, |a, b| a & b)) }
        }
        impl BitOr for $mask {
            type Output = Self;
            #[inline(always)] fn bitor(self, rhs: Self) -> Self { Self(zip!(self, rhs, |a, b| a | b)) }
        }
        impl Not for $mask {
            type Output = Self;
            #[inline(always)] fn not(self) -> Self { Self(self.0.map(|x| !x)) }
        }

        impl Sealed for $mask {}
        impl Mask for $mask {
            #[inline(always)] fn splat(value: bool) -> Self { Self([value; $n]) }
            #[inline(always)] fn any(self) -> bool { self.0.iter().any(|&x| x) }
            #[inline(always)] fn all(self) -> bool { self.0.iter().all(|&x| x) }
        }

        impl Sealed for $pack {}
        impl Sample for $pack {
            type Element = $t;
            type Mask = $mask;
            const LANES: usize = $n;
            #[inline(always)] fn splat(value: $t) -> Self { Self([value; $n]) }
            #[inline(always)] fn one() -> Self { Self([1.0; $n]) }
            #[inline(always)] fn mul_add(self, a: Self, b: Self) -> Self {
                let (x, a, b) = (self.0, a.0, b.0);
                Self(core::array::from_fn(|i| x[i].mul_add(a[i], b[i])))
            }
            #[inline(always)] fn min(self, other: Self) -> Self { Self(zip!(self, other, <$t>::min)) }
            #[inline(always)] fn max(self, other: Self) -> Self { Self(zip!(self, other, <$t>::max)) }
            #[inline(always)] fn abs(self) -> Self { Self(self.0.map(<$t>::abs)) }
            #[inline(always)] fn sqrt(self) -> Self { Self(self.0.map(<$t>::sqrt)) }
            #[inline(always)] fn simd_eq(self, o: Self) -> $mask { $mask(zip!(self, o, |a, b| a == b)) }
            #[inline(always)] fn simd_lt(self, o: Self) -> $mask { $mask(zip!(self, o, |a, b| a < b)) }
            #[inline(always)] fn simd_le(self, o: Self) -> $mask { $mask(zip!(self, o, |a, b| a <= b)) }
            #[inline(always)] fn simd_gt(self, o: Self) -> $mask { $mask(zip!(self, o, |a, b| a > b)) }
            #[inline(always)] fn simd_ge(self, o: Self) -> $mask { $mask(zip!(self, o, |a, b| a >= b)) }
            #[inline(always)] fn select(mask: $mask, if_true: Self, if_false: Self) -> Self {
                let (m, t, f) = (mask.0, if_true.0, if_false.0);
                Self(core::array::from_fn(|i| if m[i] { t[i] } else { f[i] }))
            }
        }
    };
}

pack!(
    /// Four `f32` lanes, 16-byte aligned.
    ///
    /// ```
    /// use syrp_simd::{F32x4, Sample};
    /// let p = F32x4::from_array([1.0, 2.0, 3.0, 4.0]) * F32x4::splat(2.0);
    /// assert_eq!(p.lane(3), 8.0);
    /// ```
    F32x4, Mask32x4, f32, 4, 16
);
pack!(
    /// Eight `f32` lanes, 32-byte aligned.
    ///
    /// ```
    /// use syrp_simd::{F32x8, Sample};
    /// assert_eq!(F32x8::one().to_array(), [1.0; 8]);
    /// ```
    F32x8, Mask32x8, f32, 8, 32
);
pack!(
    /// Two `f64` lanes, 16-byte aligned.
    ///
    /// ```
    /// use syrp_simd::{F64x2, Sample};
    /// assert_eq!(F64x2::from_array([4.0, 9.0]).sqrt().to_array(), [2.0, 3.0]);
    /// ```
    F64x2, Mask64x2, f64, 2, 16
);
pack!(
    /// Four `f64` lanes, 32-byte aligned.
    ///
    /// ```
    /// use syrp_simd::{F64x4, Sample};
    /// assert_eq!(F64x4::splat(-1.5).abs().lane(0), 1.5);
    /// ```
    F64x4, Mask64x4, f64, 4, 32
);

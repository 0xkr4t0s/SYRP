// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`BigInteger`]: an arbitrary-size signed integer stored as sign and magnitude.

use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};

// `ops` defines the `binary_op!` macro that `bits` also uses, so it comes first.
#[macro_use]
mod ops;
mod bits;
mod mag;
mod text;

/// An arbitrary-size signed integer.
///
/// The value is a sign and a magnitude, not two's complement, as JUCE's `BigInteger`
/// documentation describes; bit operations act on the magnitude. Zero is never negative.
/// Arithmetic allocates, so `BigInteger` is not [`syrp_rt::RtSafe`]. Division by zero does
/// not panic: see [`checked_div_rem`](Self::checked_div_rem).
///
/// ```
/// use syrp_maths::BigInteger;
/// let a = BigInteger::from(u64::MAX);
/// let square = &a * &a;
/// assert_eq!(square, BigInteger::from(u128::MAX) - BigInteger::from(u128::from(u64::MAX) * 2));
/// assert_eq!((&square / &a), a);
/// assert!(BigInteger::from(-3) < BigInteger::from(2));
/// ```
#[doc(alias = "juce::BigInteger")]
#[derive(Clone, Default, PartialEq, Eq)]
pub struct BigInteger {
    /// Little-endian limbs with no zero limb at the top; empty for zero.
    mag: Vec<u64>,
    /// True only for a non-zero negative value.
    negative: bool,
}

impl BigInteger {
    /// Builds a value from a magnitude (normalised here) and a sign.
    fn from_parts(mut mag: Vec<u64>, negative: bool) -> Self {
        mag::normalise(&mut mag);
        let negative = negative && !mag.is_empty();
        Self { mag, negative }
    }

    /// True when the value is zero.
    ///
    /// ```
    /// assert!(syrp_maths::BigInteger::default().is_zero());
    /// ```
    #[doc(alias = "isZero")]
    pub fn is_zero(&self) -> bool {
        self.mag.is_empty()
    }

    /// True when the value is exactly `1` (not `-1`).
    ///
    /// ```
    /// use syrp_maths::BigInteger;
    /// assert!(BigInteger::from(1).is_one());
    /// assert!(!BigInteger::from(-1).is_one());
    /// ```
    #[doc(alias = "isOne")]
    pub fn is_one(&self) -> bool {
        !self.negative && self.mag == [1]
    }

    /// True when the value is below zero.
    ///
    /// ```
    /// assert!(syrp_maths::BigInteger::from(-7).is_negative());
    /// ```
    #[doc(alias = "isNegative")]
    pub fn is_negative(&self) -> bool {
        self.negative
    }

    /// Sets the sign, keeping the magnitude. Has no effect on zero, which is never negative.
    ///
    /// ```
    /// use syrp_maths::BigInteger;
    /// let mut v = BigInteger::from(5);
    /// v.set_negative(true);
    /// assert_eq!(v, BigInteger::from(-5));
    /// let mut z = BigInteger::default();
    /// z.set_negative(true);
    /// assert!(!z.is_negative());
    /// ```
    #[doc(alias = "setNegative")]
    pub fn set_negative(&mut self, negative: bool) {
        self.negative = negative && !self.is_zero();
    }

    /// Flips the sign in place (zero stays zero).
    ///
    /// ```
    /// use syrp_maths::BigInteger;
    /// let mut v = BigInteger::from(9);
    /// v.negate();
    /// assert_eq!(v, BigInteger::from(-9));
    /// ```
    pub fn negate(&mut self) {
        self.negative = !self.negative && !self.is_zero();
    }

    /// The absolute value.
    ///
    /// ```
    /// use syrp_maths::BigInteger;
    /// assert_eq!(BigInteger::from(-4).abs(), BigInteger::from(4));
    /// ```
    pub fn abs(&self) -> BigInteger {
        Self {
            mag: self.mag.clone(),
            negative: false,
        }
    }

    /// The value as `i64`, or `None` when it does not fit.
    ///
    /// Syrp's choice: checked, where JUCE's `toInt64` keeps only the low bits.
    ///
    /// ```
    /// use syrp_maths::BigInteger;
    /// assert_eq!(BigInteger::from(i64::MIN).to_i64(), Some(i64::MIN));
    /// assert_eq!(BigInteger::from(u64::MAX).to_i64(), None);
    /// ```
    #[doc(alias = "toInt64")]
    pub fn to_i64(&self) -> Option<i64> {
        self.to_i128().and_then(|v| i64::try_from(v).ok())
    }

    /// The value as `u64`, or `None` when it is negative or does not fit.
    ///
    /// ```
    /// use syrp_maths::BigInteger;
    /// assert_eq!(BigInteger::from(u64::MAX).to_u64(), Some(u64::MAX));
    /// assert_eq!(BigInteger::from(-1).to_u64(), None);
    /// ```
    pub fn to_u64(&self) -> Option<u64> {
        self.to_u128().and_then(|v| u64::try_from(v).ok())
    }

    /// The value as `i128`, or `None` when it does not fit. A Syrp addition.
    ///
    /// ```
    /// use syrp_maths::BigInteger;
    /// assert_eq!(BigInteger::from(i128::MIN).to_i128(), Some(i128::MIN));
    /// ```
    pub fn to_i128(&self) -> Option<i128> {
        let m = self.magnitude_u128()?;
        if self.negative {
            0_i128.checked_sub_unsigned(m)
        } else {
            i128::try_from(m).ok()
        }
    }

    /// The value as `u128`, or `None` when it is negative or does not fit. A Syrp addition.
    ///
    /// ```
    /// use syrp_maths::BigInteger;
    /// assert_eq!(BigInteger::from(u128::MAX).to_u128(), Some(u128::MAX));
    /// ```
    pub fn to_u128(&self) -> Option<u128> {
        if self.negative {
            None
        } else {
            self.magnitude_u128()
        }
    }

    fn magnitude_u128(&self) -> Option<u128> {
        match self.mag.as_slice() {
            [] => Some(0),
            [lo] => Some(u128::from(*lo)),
            [lo, hi] => Some((u128::from(*hi) << 64) | u128::from(*lo)),
            _ => None,
        }
    }
}

impl From<u128> for BigInteger {
    /// ```
    /// assert_eq!(syrp_maths::BigInteger::from(1_u128 << 100).to_u128(), Some(1 << 100));
    /// ```
    fn from(v: u128) -> Self {
        Self::from_parts(vec![v as u64, (v >> 64) as u64], false)
    }
}

impl From<i128> for BigInteger {
    /// ```
    /// assert!(syrp_maths::BigInteger::from(-1_i128).is_negative());
    /// ```
    fn from(v: i128) -> Self {
        let mut out = Self::from(v.unsigned_abs());
        out.set_negative(v < 0);
        out
    }
}

impl From<u64> for BigInteger {
    /// ```
    /// assert_eq!(syrp_maths::BigInteger::from(7_u64).to_u64(), Some(7));
    /// ```
    fn from(v: u64) -> Self {
        Self::from(u128::from(v))
    }
}

impl From<i64> for BigInteger {
    /// ```
    /// assert_eq!(syrp_maths::BigInteger::from(-7_i64).to_i64(), Some(-7));
    /// ```
    fn from(v: i64) -> Self {
        Self::from(i128::from(v))
    }
}

impl From<u32> for BigInteger {
    /// ```
    /// assert_eq!(syrp_maths::BigInteger::from(7_u32).to_u64(), Some(7));
    /// ```
    fn from(v: u32) -> Self {
        Self::from(u128::from(v))
    }
}

impl From<i32> for BigInteger {
    /// ```
    /// assert_eq!(syrp_maths::BigInteger::from(-7_i32).to_i64(), Some(-7));
    /// ```
    fn from(v: i32) -> Self {
        Self::from(i128::from(v))
    }
}

impl Ord for BigInteger {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.negative, other.negative) {
            (false, true) => Ordering::Greater,
            (true, false) => Ordering::Less,
            (false, false) => mag::cmp(&self.mag, &other.mag),
            (true, true) => mag::cmp(&other.mag, &self.mag),
        }
    }
}

impl PartialOrd for BigInteger {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Hash for BigInteger {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let Self { mag, negative } = self;
        negative.hash(state);
        mag.hash(state);
    }
}

impl BigInteger {
    /// Writes the magnitude in base 16 (no prefix, no sign), `0` for zero.
    fn write_hex_magnitude(&self, f: &mut fmt::Formatter<'_>, upper: bool) -> fmt::Result {
        let Some((top, rest)) = self.mag.split_last() else {
            return f.write_str("0");
        };
        if upper {
            write!(f, "{top:X}")?;
            rest.iter().rev().try_for_each(|l| write!(f, "{l:016X}"))
        } else {
            write!(f, "{top:x}")?;
            rest.iter().rev().try_for_each(|l| write!(f, "{l:016x}"))
        }
    }
}

impl fmt::Debug for BigInteger {
    /// Prints `BigInteger(0x…)` with a leading `-` for negative values.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("BigInteger(")?;
        if self.negative {
            f.write_str("-")?;
        }
        f.write_str("0x")?;
        self.write_hex_magnitude(f, false)?;
        f.write_str(")")
    }
}

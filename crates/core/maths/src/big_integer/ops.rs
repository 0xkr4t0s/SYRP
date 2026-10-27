// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Arithmetic (B2) for [`BigInteger`].

use std::cmp::Ordering;
use std::ops::{
    Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Rem, RemAssign, Sub, SubAssign,
};

use super::{BigInteger, mag};

/// `a + b` where `b`'s sign is replaced by `b_negative`.
fn add_signed(a: &BigInteger, b: &BigInteger, b_negative: bool) -> BigInteger {
    if a.negative == b_negative {
        return BigInteger::from_parts(mag::add(&a.mag, &b.mag), a.negative);
    }
    match mag::cmp(&a.mag, &b.mag) {
        Ordering::Equal => BigInteger::default(),
        Ordering::Greater => BigInteger::from_parts(mag::sub(&a.mag, &b.mag), a.negative),
        Ordering::Less => BigInteger::from_parts(mag::sub(&b.mag, &a.mag), b_negative),
    }
}

fn add(a: &BigInteger, b: &BigInteger) -> BigInteger {
    add_signed(a, b, b.negative)
}

fn sub(a: &BigInteger, b: &BigInteger) -> BigInteger {
    add_signed(a, b, !b.negative)
}

fn mul(a: &BigInteger, b: &BigInteger) -> BigInteger {
    BigInteger::from_parts(mag::mul(&a.mag, &b.mag), a.negative != b.negative)
}

fn div(a: &BigInteger, b: &BigInteger) -> BigInteger {
    a.div_rem(b).0
}

fn rem(a: &BigInteger, b: &BigInteger) -> BigInteger {
    a.div_rem(b).1
}

impl BigInteger {
    /// Quotient and remainder in one step, or `None` when `divisor` is zero.
    ///
    /// The quotient truncates toward zero and the remainder has the dividend's sign, so
    /// `quotient * divisor + remainder == self`.
    ///
    /// ```
    /// use syrp_maths::BigInteger;
    /// let (q, r) = BigInteger::from(-7).checked_div_rem(&BigInteger::from(2)).unwrap();
    /// assert_eq!((q, r), (BigInteger::from(-3), BigInteger::from(-1)));
    /// assert!(BigInteger::from(1).checked_div_rem(&BigInteger::default()).is_none());
    /// ```
    #[doc(alias = "divideBy")]
    pub fn checked_div_rem(&self, divisor: &BigInteger) -> Option<(BigInteger, BigInteger)> {
        if divisor.is_zero() {
            return None;
        }
        let (q, r) = mag::div_rem(&self.mag, &divisor.mag);
        Some((
            BigInteger::from_parts(q, self.negative != divisor.negative),
            BigInteger::from_parts(r, self.negative),
        ))
    }

    /// Quotient and remainder in one step, as [`checked_div_rem`](Self::checked_div_rem).
    ///
    /// Syrp's choice: dividing by zero gives `(0, 0)` rather than panicking, and so do the
    /// `/` and `%` operators.
    ///
    /// ```
    /// use syrp_maths::BigInteger;
    /// let (q, r) = BigInteger::from(7).div_rem(&BigInteger::from(-2));
    /// assert_eq!((q, r), (BigInteger::from(-3), BigInteger::from(1)));
    /// let zero = BigInteger::default();
    /// assert_eq!(BigInteger::from(7).div_rem(&zero), (zero.clone(), zero));
    /// ```
    pub fn div_rem(&self, divisor: &BigInteger) -> (BigInteger, BigInteger) {
        self.checked_div_rem(divisor).unwrap_or_default()
    }
}

// Generates the four by-value/by-reference forms of a binary operator and both `*Assign`
// forms from one `fn(&BigInteger, &BigInteger) -> BigInteger`.
macro_rules! binary_op {
    ($Op:ident, $op:ident, $OpAssign:ident, $op_assign:ident, $f:path) => {
        impl $Op<&BigInteger> for &BigInteger {
            type Output = BigInteger;
            fn $op(self, rhs: &BigInteger) -> BigInteger {
                $f(self, rhs)
            }
        }
        impl $Op<BigInteger> for &BigInteger {
            type Output = BigInteger;
            fn $op(self, rhs: BigInteger) -> BigInteger {
                $f(self, &rhs)
            }
        }
        impl $Op<&BigInteger> for BigInteger {
            type Output = BigInteger;
            fn $op(self, rhs: &BigInteger) -> BigInteger {
                $f(&self, rhs)
            }
        }
        impl $Op<BigInteger> for BigInteger {
            type Output = BigInteger;
            fn $op(self, rhs: BigInteger) -> BigInteger {
                $f(&self, &rhs)
            }
        }
        impl $OpAssign<&BigInteger> for BigInteger {
            fn $op_assign(&mut self, rhs: &BigInteger) {
                *self = $f(self, rhs);
            }
        }
        impl $OpAssign<BigInteger> for BigInteger {
            fn $op_assign(&mut self, rhs: BigInteger) {
                *self = $f(self, &rhs);
            }
        }
    };
}

binary_op!(Add, add, AddAssign, add_assign, add);
binary_op!(Sub, sub, SubAssign, sub_assign, sub);
binary_op!(Mul, mul, MulAssign, mul_assign, mul);
binary_op!(Div, div, DivAssign, div_assign, div);
binary_op!(Rem, rem, RemAssign, rem_assign, rem);

impl Neg for BigInteger {
    type Output = BigInteger;
    fn neg(mut self) -> BigInteger {
        self.negate();
        self
    }
}

impl Neg for &BigInteger {
    type Output = BigInteger;
    fn neg(self) -> BigInteger {
        -self.clone()
    }
}

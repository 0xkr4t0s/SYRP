// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Number theory for [`BigInteger`] (B5): modular exponentiation, modular inverse and the
//! greatest common divisor. Not constant-time; see the crate `AGENTS.md`.
//!
//! JUCE also has `montgomeryMultiplication`, but its documentation does not describe the
//! contract of its extra parameters, so Syrp leaves it out.

use super::BigInteger;

impl BigInteger {
    /// `self` reduced into `0..modulus` (`modulus` positive).
    fn reduce(&self, modulus: &BigInteger) -> BigInteger {
        let r = self % modulus;
        if r.is_negative() { r + modulus } else { r }
    }

    /// `self` raised to `exponent`, reduced modulo `modulus`, always in `0..|modulus|`.
    ///
    /// Syrp's choices, where the JUCE documentation says nothing: the modulus's sign is
    /// ignored; a zero modulus gives zero (like `%`); a negative exponent raises the modular
    /// inverse instead, and gives zero when there is no inverse. Not constant-time.
    ///
    /// ```
    /// use syrp_maths::BigInteger;
    /// let r = BigInteger::from(4).exponent_modulo(&BigInteger::from(13), &BigInteger::from(497));
    /// assert_eq!(r, BigInteger::from(445));
    /// let r = BigInteger::from(-2).exponent_modulo(&BigInteger::from(3), &BigInteger::from(7));
    /// assert_eq!(r, BigInteger::from(6)); // -8 mod 7
    /// ```
    #[doc(alias = "exponentModulo")]
    pub fn exponent_modulo(&self, exponent: &BigInteger, modulus: &BigInteger) -> BigInteger {
        let m = modulus.abs();
        if m.is_zero() {
            return BigInteger::default();
        }
        let base = if exponent.is_negative() {
            match self.inverse_modulo(&m) {
                Some(inverse) => inverse,
                None => return BigInteger::default(),
            }
        } else {
            self.reduce(&m)
        };
        let mut result = BigInteger::from(1).reduce(&m);
        for i in (0..=exponent.highest_bit().unwrap_or(0)).rev() {
            result = (&result * &result) % &m;
            if exponent.bit(i) {
                result = (&result * &base) % &m;
            }
        }
        result
    }

    /// The `x` in `0..|modulus|` with `self * x % modulus == 1`, or `None` when `self` and
    /// `modulus` share a factor or `modulus` is zero. The modulus's sign is ignored; for a
    /// modulus of 1 the answer is `Some(0)`, since every value is congruent there.
    ///
    /// ```
    /// use syrp_maths::BigInteger;
    /// let m = BigInteger::from(11);
    /// assert_eq!(BigInteger::from(3).inverse_modulo(&m), Some(BigInteger::from(4)));
    /// assert_eq!(BigInteger::from(22).inverse_modulo(&m), None);
    /// ```
    #[doc(alias = "inverseModulo")]
    pub fn inverse_modulo(&self, modulus: &BigInteger) -> Option<BigInteger> {
        let m = modulus.abs();
        if m.is_zero() {
            return None;
        }
        // Extended Euclid on (self mod m, m), tracking only the coefficient of `self`.
        let (mut old_r, mut r) = (self.reduce(&m), m.clone());
        let (mut old_s, mut s) = (BigInteger::from(1), BigInteger::default());
        while !r.is_zero() {
            let (q, rem) = old_r.div_rem(&r);
            old_r = std::mem::replace(&mut r, rem);
            let next = &old_s - &(&q * &s);
            old_s = std::mem::replace(&mut s, next);
        }
        if m.is_one() {
            return Some(BigInteger::default());
        }
        old_r.is_one().then(|| old_s.reduce(&m))
    }

    /// The greatest common divisor of `self` and `other`, never negative. Signs are ignored,
    /// and `gcd(0, 0)` is zero.
    ///
    /// ```
    /// use syrp_maths::BigInteger;
    /// let g = BigInteger::from(-84).find_greatest_common_divisor(&BigInteger::from(36));
    /// assert_eq!(g, BigInteger::from(12));
    /// ```
    #[doc(alias = "findGreatestCommonDivisor")]
    pub fn find_greatest_common_divisor(&self, other: &BigInteger) -> BigInteger {
        let (mut a, mut b) = (self.abs(), other.abs());
        while !b.is_zero() {
            let r = &a % &b;
            a = std::mem::replace(&mut b, r);
        }
        a
    }
}

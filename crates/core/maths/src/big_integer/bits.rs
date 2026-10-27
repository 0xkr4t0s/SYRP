// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Bit access, shifts and bitwise operators on the magnitude of a [`BigInteger`] (B3). The
//! sign is never changed here, except that a value whose magnitude becomes zero is no longer
//! negative.

use std::ops::{
    BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Shl, ShlAssign, Shr, ShrAssign,
};

use super::{BigInteger, mag};

/// The limb index and in-limb shift of bit `n`.
fn locate(n: u64) -> (usize, u32) {
    (
        usize::try_from(n / 64).expect("bit index exceeds the address space"),
        (n % 64) as u32,
    )
}

/// A mask of the bits `lo..hi` within one limb (`hi <= 64`).
fn limb_mask(lo: u32, hi: u32) -> u64 {
    let upper = if hi >= 64 {
        u64::MAX
    } else {
        (1_u64 << hi) - 1
    };
    upper & !((1_u64 << lo) - 1)
}

impl BigInteger {
    /// Restores the invariants after the magnitude was edited in place.
    fn renormalise(&mut self) {
        mag::normalise(&mut self.mag);
        self.negative &= !self.mag.is_empty();
    }

    /// True when bit `n` of the magnitude is set.
    ///
    /// ```
    /// use syrp_maths::BigInteger;
    /// let v = BigInteger::from(-6); // magnitude 0b110
    /// assert!(!v.bit(0) && v.bit(1) && v.bit(2) && !v.bit(1000));
    /// ```
    #[doc(alias = "operator[]")]
    pub fn bit(&self, n: u32) -> bool {
        let (i, s) = locate(u64::from(n));
        self.mag.get(i).is_some_and(|l| (l >> s) & 1 == 1)
    }

    /// Sets bit `n` of the magnitude to `value`, growing the value as needed.
    ///
    /// ```
    /// use syrp_maths::BigInteger;
    /// let mut v = BigInteger::default();
    /// v.set_bit(70, true);
    /// assert_eq!(v, BigInteger::from(1_u128 << 70));
    /// ```
    #[doc(alias = "setBit")]
    pub fn set_bit(&mut self, n: u32, value: bool) {
        self.set_range(n, 1, value);
    }

    /// Clears bit `n` of the magnitude.
    ///
    /// ```
    /// use syrp_maths::BigInteger;
    /// let mut v = BigInteger::from(5);
    /// v.clear_bit(0);
    /// assert_eq!(v, BigInteger::from(4));
    /// ```
    #[doc(alias = "clearBit")]
    pub fn clear_bit(&mut self, n: u32) {
        self.set_bit(n, false);
    }

    /// Inverts bit `n` of the magnitude.
    ///
    /// ```
    /// use syrp_maths::BigInteger;
    /// let mut v = BigInteger::from(5);
    /// v.flip_bit(1);
    /// assert_eq!(v, BigInteger::from(7));
    /// ```
    #[doc(alias = "flipBit")]
    pub fn flip_bit(&mut self, n: u32) {
        let set = self.bit(n);
        self.set_bit(n, !set);
    }

    /// Sets `count` bits of the magnitude, starting at bit `start`, to `value`. Bits past
    /// index `u32::MAX` are not addressable and are ignored.
    ///
    /// ```
    /// use syrp_maths::BigInteger;
    /// let mut v = BigInteger::default();
    /// v.set_range(4, 4, true);
    /// assert_eq!(v, BigInteger::from(0xF0));
    /// v.set_range(0, 6, false);
    /// assert_eq!(v, BigInteger::from(0xC0));
    /// ```
    #[doc(alias = "setRange")]
    pub fn set_range(&mut self, start: u32, count: u32, value: bool) {
        let lo = u64::from(start);
        let hi = (lo + u64::from(count)).min(1 << 32);
        if lo >= hi {
            return;
        }
        let (first, _) = locate(lo);
        let (last, _) = locate(hi - 1);
        if value && self.mag.len() <= last {
            self.mag.resize(last + 1, 0);
        }
        let end = (last + 1).min(self.mag.len());
        for i in first..end {
            let base = i as u64 * 64;
            let a = lo.saturating_sub(base) as u32;
            let b = (hi - base).min(64) as u32;
            let m = limb_mask(a, b);
            if value {
                self.mag[i] |= m;
            } else {
                self.mag[i] &= !m;
            }
        }
        self.renormalise();
    }

    /// Inserts a bit of the given value at index `n`, moving every bit at or above `n` up
    /// by one place.
    ///
    /// ```
    /// use syrp_maths::BigInteger;
    /// let mut v = BigInteger::from(0b101);
    /// v.insert_bit(1, true);
    /// assert_eq!(v, BigInteger::from(0b1011));
    /// ```
    #[doc(alias = "insertBit")]
    pub fn insert_bit(&mut self, n: u32, value: bool) {
        let high = mag::shl(&mag::shr(&self.mag, u64::from(n)), u64::from(n) + 1);
        let low = self.bit_range(0, n);
        let negative = self.negative;
        let mut out = BigInteger {
            mag: high,
            negative: false,
        } | low;
        out.set_bit(n, value);
        out.negative = negative;
        out.renormalise();
        *self = out;
    }

    /// The non-negative value made of `count` bits of the magnitude starting at `start`.
    ///
    /// ```
    /// use syrp_maths::BigInteger;
    /// let v = BigInteger::from(-0xABCD);
    /// assert_eq!(v.bit_range(4, 8), BigInteger::from(0xBC));
    /// ```
    #[doc(alias = "getBitRange")]
    pub fn bit_range(&self, start: u32, count: u32) -> BigInteger {
        let mut out = mag::shr(&self.mag, u64::from(start));
        let (limbs, s) = locate(u64::from(count));
        if limbs < out.len() {
            out.truncate(limbs + 1);
            out[limbs] &= limb_mask(0, s);
        }
        BigInteger::from_parts(out, false)
    }

    /// Up to 32 bits of the magnitude starting at `start`, as a `u32`. A `count` above 32
    /// is treated as 32 (Syrp's choice; the documentation does not allow more).
    ///
    /// ```
    /// use syrp_maths::BigInteger;
    /// let v = BigInteger::from(0x1234_5678_9ABC_u64);
    /// assert_eq!(v.bit_range_as_u32(8, 16), 0x789A);
    /// ```
    #[doc(alias = "getBitRangeAsInt")]
    pub fn bit_range_as_u32(&self, start: u32, count: u32) -> u32 {
        let count = count.min(32);
        let (i, s) = locate(u64::from(start));
        let limb = |k: usize| u128::from(self.mag.get(k).copied().unwrap_or(0));
        let window = (limb(i) | (limb(i + 1) << 64)) >> s;
        (window as u64 & limb_mask(0, count)) as u32
    }

    /// Writes the low `count` bits of `value` into the magnitude starting at bit `start`.
    /// A `count` above 32 is treated as 32.
    ///
    /// ```
    /// use syrp_maths::BigInteger;
    /// let mut v = BigInteger::from(0xFFFF);
    /// v.set_bit_range_as_u32(4, 8, 0x0A);
    /// assert_eq!(v, BigInteger::from(0xF0AF));
    /// ```
    #[doc(alias = "setBitRangeAsInt")]
    pub fn set_bit_range_as_u32(&mut self, start: u32, count: u32, value: u32) {
        for k in 0..count.min(32) {
            let Some(n) = start.checked_add(k) else { break };
            self.set_bit(n, (value >> k) & 1 == 1);
        }
    }

    /// The index of the highest set bit of the magnitude, or `None` for zero.
    ///
    /// # Panics
    /// If the magnitude has more than 2^32 bits (512 MiB), where the index does not fit.
    ///
    /// ```
    /// use syrp_maths::BigInteger;
    /// assert_eq!(BigInteger::from(-256).highest_bit(), Some(8));
    /// assert_eq!(BigInteger::default().highest_bit(), None);
    /// ```
    #[doc(alias = "getHighestBit")]
    pub fn highest_bit(&self) -> Option<u32> {
        let top = self.mag.last()?;
        let bits = (self.mag.len() as u64 - 1) * 64 + u64::from(63 - top.leading_zeros());
        Some(u32::try_from(bits).expect("BigInteger exceeds 2^32 bits"))
    }

    /// The index of the first set bit at or above `from`, or `None` if there is none.
    ///
    /// ```
    /// use syrp_maths::BigInteger;
    /// let v = BigInteger::from(0b1001_0000);
    /// assert_eq!(v.find_next_set_bit(0), Some(4));
    /// assert_eq!(v.find_next_set_bit(5), Some(7));
    /// assert_eq!(v.find_next_set_bit(8), None);
    /// ```
    #[doc(alias = "findNextSetBit")]
    pub fn find_next_set_bit(&self, from: u32) -> Option<u32> {
        self.find_next(from, false)
    }

    /// The index of the first clear bit at or above `from`. Every value has clear bits above
    /// its highest set bit, so this is `None` only if every bit from `from` to `u32::MAX`
    /// is set.
    ///
    /// ```
    /// use syrp_maths::BigInteger;
    /// let v = BigInteger::from(0b0111);
    /// assert_eq!(v.find_next_clear_bit(0), Some(3));
    /// assert_eq!(v.find_next_clear_bit(10), Some(10));
    /// ```
    #[doc(alias = "findNextClearBit")]
    pub fn find_next_clear_bit(&self, from: u32) -> Option<u32> {
        self.find_next(from, true)
    }

    /// Scans for the first set bit (or clear bit, when `invert`) at index `from` or above.
    fn find_next(&self, from: u32, invert: bool) -> Option<u32> {
        let (first, s) = locate(u64::from(from));
        let flip = if invert { u64::MAX } else { 0 };
        let found = if first >= self.mag.len() {
            invert.then_some(u64::from(from))
        } else {
            let head = ((self.mag[first] ^ flip) & limb_mask(s, 64), first);
            let tail = self.mag[first + 1..]
                .iter()
                .zip(first + 1..)
                .map(|(l, i)| (l ^ flip, i));
            std::iter::once(head)
                .chain(tail)
                .find(|&(l, _)| l != 0)
                .map(|(l, i)| i as u64 * 64 + u64::from(l.trailing_zeros()))
                .or_else(|| invert.then_some(self.mag.len() as u64 * 64))
        };
        found.and_then(|n| u32::try_from(n).ok())
    }

    /// The number of set bits in the magnitude.
    ///
    /// ```
    /// assert_eq!(syrp_maths::BigInteger::from(-0b1011).count_set_bits(), 3);
    /// ```
    #[doc(alias = "countNumberOfSetBits")]
    pub fn count_set_bits(&self) -> u64 {
        self.mag.iter().map(|l| u64::from(l.count_ones())).sum()
    }
}

/// Applies a limb-wise operation to both magnitudes; the result keeps `a`'s sign (Syrp's
/// choice, see the crate `AGENTS.md`), and zero is never negative.
fn bitwise(a: &BigInteger, b: &BigInteger, f: impl Fn(u64, u64) -> u64) -> BigInteger {
    let len = a.mag.len().max(b.mag.len());
    let limb = |v: &[u64], i: usize| v.get(i).copied().unwrap_or(0);
    let out = (0..len)
        .map(|i| f(limb(&a.mag, i), limb(&b.mag, i)))
        .collect();
    BigInteger::from_parts(out, a.negative)
}

fn bitand(a: &BigInteger, b: &BigInteger) -> BigInteger {
    bitwise(a, b, |x, y| x & y)
}

fn bitor(a: &BigInteger, b: &BigInteger) -> BigInteger {
    bitwise(a, b, |x, y| x | y)
}

fn bitxor(a: &BigInteger, b: &BigInteger) -> BigInteger {
    bitwise(a, b, |x, y| x ^ y)
}

binary_op!(BitAnd, bitand, BitAndAssign, bitand_assign, bitand);
binary_op!(BitOr, bitor, BitOrAssign, bitor_assign, bitor);
binary_op!(BitXor, bitxor, BitXorAssign, bitxor_assign, bitxor);

/// Shifts the magnitude up by `bits`; the sign is kept.
impl Shl<u32> for &BigInteger {
    type Output = BigInteger;
    fn shl(self, bits: u32) -> BigInteger {
        BigInteger::from_parts(mag::shl(&self.mag, u64::from(bits)), self.negative)
    }
}

/// Shifts the magnitude down by `bits`, dropping the low bits; the sign is kept unless the
/// result is zero (so `-5 >> 1 == -2`).
impl Shr<u32> for &BigInteger {
    type Output = BigInteger;
    fn shr(self, bits: u32) -> BigInteger {
        BigInteger::from_parts(mag::shr(&self.mag, u64::from(bits)), self.negative)
    }
}

impl Shl<u32> for BigInteger {
    type Output = BigInteger;
    fn shl(self, bits: u32) -> BigInteger {
        &self << bits
    }
}

impl Shr<u32> for BigInteger {
    type Output = BigInteger;
    fn shr(self, bits: u32) -> BigInteger {
        &self >> bits
    }
}

impl ShlAssign<u32> for BigInteger {
    fn shl_assign(&mut self, bits: u32) {
        *self = &*self << bits;
    }
}

impl ShrAssign<u32> for BigInteger {
    fn shr_assign(&mut self, bits: u32) {
        *self = &*self >> bits;
    }
}

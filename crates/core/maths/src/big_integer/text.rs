// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Text and byte forms of a [`BigInteger`] (B4).

use std::fmt;
use std::str::FromStr;

use super::{BigInteger, mag};
use crate::MathsError;

/// For a supported base: the largest power of it that fits a `u64`, and that power's exponent.
fn chunk(base: u32) -> Result<(u64, usize), MathsError> {
    let digits = match base {
        2 => 63,
        8 => 21,
        10 => 19,
        16 => 15,
        _ => return Err(MathsError::InvalidBaseOrDigit),
    };
    Ok((u64::from(base).pow(digits), digits as usize))
}

impl BigInteger {
    /// The magnitude's digits in `base` (lowercase), most significant first, `"0"` for zero.
    fn magnitude_digits(&self, base: u32) -> Result<String, MathsError> {
        let (power, width) = chunk(base)?;
        let mut chunks = Vec::new();
        let mut rest = self.mag.clone();
        while !rest.is_empty() {
            let (q, r) = mag::div_rem_limb(&rest, power);
            chunks.push(r);
            rest = q;
        }
        let mut out = String::new();
        for (i, &c) in chunks.iter().rev().enumerate() {
            let mut digits = [b'0'; 64];
            let mut v = c;
            for d in digits[..width].iter_mut().rev() {
                *d = b"0123456789abcdef"[(v % u64::from(base)) as usize];
                v /= u64::from(base);
            }
            let text = std::str::from_utf8(&digits[..width]).unwrap_or_default();
            out.push_str(if i == 0 {
                text.trim_start_matches('0')
            } else {
                text
            });
        }
        if out.is_empty() {
            out.push('0');
        }
        Ok(out)
    }

    /// The value as text in `base` (2, 8, 10 or 16), zero-padded to at least `minimum_digits`
    /// digits, with a leading `-` when negative. Hex digits are lowercase.
    ///
    /// Syrp's choices: the `-` goes before the padding and does not count as a digit, and an
    /// unsupported base is an error ([`MathsError::InvalidBaseOrDigit`]).
    ///
    /// ```
    /// use syrp_maths::BigInteger;
    /// assert_eq!(BigInteger::from(-255).to_string_radix(16, 4)?, "-00ff");
    /// assert_eq!(BigInteger::from(5).to_string_radix(2, 0)?, "101");
    /// assert!(BigInteger::from(5).to_string_radix(3, 0).is_err());
    /// # Ok::<(), syrp_maths::MathsError>(())
    /// ```
    #[doc(alias = "toString")]
    pub fn to_string_radix(&self, base: u32, minimum_digits: usize) -> Result<String, MathsError> {
        let digits = self.magnitude_digits(base)?;
        let pad = minimum_digits.saturating_sub(digits.len());
        let mut out = String::with_capacity(usize::from(self.negative) + pad + digits.len());
        if self.negative {
            out.push('-');
        }
        out.extend(std::iter::repeat_n('0', pad));
        out.push_str(&digits);
        Ok(out)
    }

    /// Reads a value written in `base` (2, 8, 10 or 16): an optional leading `-` and at least
    /// one digit. Hex digits may be either case; leading zeros are allowed and `-0` is zero.
    ///
    /// # Errors
    /// [`MathsError::InvalidBaseOrDigit`] (`SYRP-MATHS-0002`) for another base, empty text, or
    /// any character that is not a digit of `base` (spaces and `+` included).
    ///
    /// ```
    /// use syrp_maths::BigInteger;
    /// assert_eq!(BigInteger::parse("-FF", 16)?, BigInteger::from(-255));
    /// assert!(BigInteger::parse("12a", 10).is_err());
    /// # Ok::<(), syrp_maths::MathsError>(())
    /// ```
    #[doc(alias = "parseString")]
    pub fn parse(text: &str, base: u32) -> Result<BigInteger, MathsError> {
        let (power, width) = chunk(base)?;
        let (negative, digits) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text),
        };
        if digits.is_empty() {
            return Err(MathsError::InvalidBaseOrDigit);
        }
        let mut out = Vec::new();
        let first = digits.len() % width;
        let split = std::iter::once(&digits.as_bytes()[..first])
            .filter(|c| !c.is_empty())
            .chain(digits.as_bytes()[first..].chunks(width));
        for piece in split {
            let mut value = 0_u64;
            for &c in piece {
                let d = char::from(c)
                    .to_digit(base)
                    .ok_or(MathsError::InvalidBaseOrDigit)?;
                value = value * u64::from(base) + u64::from(d);
            }
            let scale = if piece.len() == width {
                power
            } else {
                u64::from(base).pow(piece.len() as u32)
            };
            mag::mul_add_limb(&mut out, scale, value);
        }
        Ok(BigInteger::from_parts(out, negative))
    }

    /// The magnitude as little-endian bytes, without high zero bytes (empty for zero). The
    /// sign is not stored, as JUCE's `toMemoryBlock` documentation describes.
    ///
    /// ```
    /// use syrp_maths::BigInteger;
    /// assert_eq!(BigInteger::from(-0x0102).to_le_bytes(), [0x02, 0x01]);
    /// assert!(BigInteger::default().to_le_bytes().is_empty());
    /// ```
    #[doc(alias = "toMemoryBlock")]
    pub fn to_le_bytes(&self) -> Vec<u8> {
        let mut out: Vec<u8> = self.mag.iter().flat_map(|l| l.to_le_bytes()).collect();
        while out.last() == Some(&0) {
            out.pop();
        }
        out
    }

    /// The non-negative value whose little-endian bytes are `bytes`. Any byte string is
    /// accepted, high zero bytes included (fixed-width fields), as JUCE's
    /// `loadFromMemoryBlock` documentation describes.
    ///
    /// ```
    /// use syrp_maths::BigInteger;
    /// assert_eq!(BigInteger::from_le_bytes(&[0x02, 0x01, 0x00]), BigInteger::from(0x0102));
    /// ```
    #[doc(alias = "loadFromMemoryBlock")]
    pub fn from_le_bytes(bytes: &[u8]) -> BigInteger {
        let limbs = bytes
            .chunks(8)
            .map(|c| {
                let mut b = [0_u8; 8];
                b[..c.len()].copy_from_slice(c);
                u64::from_le_bytes(b)
            })
            .collect();
        BigInteger::from_parts(limbs, false)
    }

    /// Writes the digits through [`fmt::Formatter::pad_integral`], so width, fill, `+` and
    /// `#` (with `prefix`) work as they do for the primitive integers.
    fn pad(&self, f: &mut fmt::Formatter<'_>, base: u32, prefix: &str, upper: bool) -> fmt::Result {
        let mut digits = self.magnitude_digits(base).map_err(|_| fmt::Error)?;
        if upper {
            digits.make_ascii_uppercase();
        }
        f.pad_integral(!self.negative, prefix, &digits)
    }
}

impl fmt::Display for BigInteger {
    /// Base 10, as [`to_string_radix(10, 0)`](BigInteger::to_string_radix).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.pad(f, 10, "", false)
    }
}

impl fmt::LowerHex for BigInteger {
    /// Base 16 in lowercase with a `-` for negative values; `{:#x}` adds `0x`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.pad(f, 16, "0x", false)
    }
}

impl fmt::UpperHex for BigInteger {
    /// Base 16 in uppercase with a `-` for negative values; `{:#X}` adds `0x`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.pad(f, 16, "0x", true)
    }
}

impl FromStr for BigInteger {
    type Err = MathsError;

    /// Base 10, as [`BigInteger::parse`].
    fn from_str(text: &str) -> Result<Self, MathsError> {
        Self::parse(text, 10)
    }
}

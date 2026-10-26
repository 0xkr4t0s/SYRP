// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`Uuid`]: a 128-bit universally unique identifier.

use std::fmt;
use std::str::FromStr;

use crate::SysError;

/// A 128-bit UUID, stored as 16 bytes in the order they are written (RFC 9562).
///
/// Ordering, equality and hashing compare the bytes. Text is lowercase hex in the
/// hyphenated 8-4-4-4-12 form ([`Display`](fmt::Display)) or as 32 undelimited digits.
///
/// ```
/// use syrp_sys::Uuid;
/// let id = Uuid::random().unwrap();
/// assert_eq!(id.version(), 4);
/// assert_eq!(id.to_string().parse::<Uuid>(), Ok(id));
/// assert!(Uuid::nil() < id);
/// ```
#[doc(alias = "juce::Uuid")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Uuid {
    bytes: [u8; 16],
}

/// Byte offsets of the hyphens in the 36-character form.
const HYPHENS: [usize; 4] = [8, 13, 18, 23];

impl Uuid {
    /// A new random (version 4, RFC variant) UUID from the operating system's random source.
    ///
    /// # Errors
    /// [`SysError::EntropyUnavailable`] (`SYRP-SYS-0003`) if the OS source fails.
    ///
    /// ```
    /// # use syrp_sys::Uuid;
    /// let (a, b) = (Uuid::random().unwrap(), Uuid::random().unwrap());
    /// assert_ne!(a, b);
    /// ```
    pub fn random() -> Result<Self, SysError> {
        let mut bytes = [0_u8; 16];
        getrandom::fill(&mut bytes).map_err(|e| SysError::EntropyUnavailable {
            os_error: e.raw_os_error(),
        })?;
        bytes[6] = (bytes[6] & 0x0f) | 0x40; // Version 4.
        bytes[8] = (bytes[8] & 0x3f) | 0x80; // RFC 9562 variant (binary 10).
        Ok(Self { bytes })
    }

    /// The all-zero UUID.
    ///
    /// ```
    /// # use syrp_sys::Uuid;
    /// assert!(Uuid::nil().is_nil());
    /// assert_eq!(Uuid::nil().to_string(), "00000000-0000-0000-0000-000000000000");
    /// ```
    #[doc(alias = "null")]
    #[must_use]
    pub const fn nil() -> Self {
        Self { bytes: [0; 16] }
    }

    /// Whether every byte is zero.
    ///
    /// ```
    /// # use syrp_sys::Uuid;
    /// assert!(!Uuid::from_bytes([1; 16]).is_nil());
    /// ```
    #[doc(alias = "isNull")]
    #[must_use]
    pub const fn is_nil(self) -> bool {
        u128::from_be_bytes(self.bytes) == 0
    }

    /// The UUID with these 16 bytes, unchanged (no version or variant bits are set).
    ///
    /// ```
    /// # use syrp_sys::Uuid;
    /// let id = Uuid::from_bytes([0xab; 16]);
    /// assert_eq!(id.to_bytes(), [0xab; 16]);
    /// ```
    #[doc(alias = "getRawData")]
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self { bytes }
    }

    /// The 16 bytes, in written order.
    ///
    /// ```
    /// # use syrp_sys::Uuid;
    /// assert_eq!(Uuid::nil().to_bytes(), [0; 16]);
    /// ```
    #[must_use]
    pub const fn to_bytes(self) -> [u8; 16] {
        self.bytes
    }

    /// The version number, the high four bits of byte 6 (4 for [`Uuid::random`]).
    ///
    /// ```
    /// # use syrp_sys::Uuid;
    /// let id: Uuid = "123e4567-e89b-12d3-a456-426614174000".parse().unwrap();
    /// assert_eq!(id.version(), 1);
    /// ```
    #[must_use]
    pub const fn version(self) -> u8 {
        self.bytes[6] >> 4
    }

    /// Parses the 36-character hyphenated form or the 32-character undelimited form.
    /// Hex digits may be upper or lower case; nothing else is accepted.
    ///
    /// # Errors
    /// [`SysError::InvalidUuid`] (`SYRP-SYS-0002`) for any other text.
    ///
    /// ```
    /// # use syrp_sys::Uuid;
    /// let a = Uuid::parse("123E4567-E89B-12D3-A456-426614174000").unwrap();
    /// assert_eq!(Uuid::parse("123e4567e89b12d3a456426614174000"), Ok(a));
    /// assert!(Uuid::parse("123e4567-e89b-12d3-a456").is_err());
    /// ```
    pub fn parse(text: &str) -> Result<Self, SysError> {
        let s = text.as_bytes();
        let fail = |position, reason| Err(SysError::InvalidUuid { position, reason });
        let hyphenated = match s.len() {
            36 => true,
            32 => false,
            _ => return fail(0, "expected 36 characters (hyphenated) or 32 (undelimited)"),
        };
        let mut value = 0_u128;
        for (i, &c) in s.iter().enumerate() {
            if hyphenated && HYPHENS.contains(&i) {
                if c != b'-' {
                    return fail(i, "expected '-'");
                }
                continue;
            }
            let Some(digit) = char::from(c).to_digit(16) else {
                return fail(i, "expected a hex digit");
            };
            value = (value << 4) | u128::from(digit);
        }
        Ok(Self {
            bytes: value.to_be_bytes(),
        })
    }

    /// The 32-character undelimited lowercase form.
    ///
    /// ```
    /// # use syrp_sys::Uuid;
    /// let id = Uuid::from_bytes([0x0f; 16]);
    /// assert_eq!(id.to_undelimited_string(), "0f".repeat(16));
    /// ```
    #[doc(alias = "toString")]
    #[must_use]
    pub fn to_undelimited_string(self) -> String {
        format!("{:032x}", u128::from_be_bytes(self.bytes))
    }

    /// The 36-character hyphenated lowercase form, the same as `to_string()`.
    ///
    /// ```
    /// # use syrp_sys::Uuid;
    /// let id = Uuid::from_bytes([0xff; 16]);
    /// assert_eq!(id.to_hyphenated_string(), "ffffffff-ffff-ffff-ffff-ffffffffffff");
    /// ```
    #[doc(alias = "toDashedString")]
    #[must_use]
    pub fn to_hyphenated_string(self) -> String {
        self.to_string()
    }
}

impl fmt::Display for Uuid {
    /// Writes the hyphenated 8-4-4-4-12 lowercase form.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let v = u128::from_be_bytes(self.bytes);
        write!(
            f,
            "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
            v >> 96,
            (v >> 80) & 0xffff,
            (v >> 64) & 0xffff,
            (v >> 48) & 0xffff,
            v & 0xffff_ffff_ffff
        )
    }
}

impl FromStr for Uuid {
    type Err = SysError;
    /// Same as [`Uuid::parse`].
    fn from_str(s: &str) -> Result<Self, SysError> {
        Self::parse(s)
    }
}

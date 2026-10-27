// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`MacAddress`]: a 6-byte hardware address, and its enumeration.

use std::fmt;
use std::str::FromStr;

use crate::NetError;
use crate::ifaces;

/// A 6-byte (EUI-48) hardware address (JUCE's `MACAddress`).
///
/// Ordering and hashing are by bytes. [`Display`](fmt::Display) writes lower-case hex bytes
/// separated by `-`, as in `00-1a-2b-3c-4d-5e`; [`MacAddress::to_string_with`] chooses another
/// separator. The default value is the null address (all zero).
///
/// ```
/// use syrp_net::MacAddress;
/// let m = MacAddress::new([0x00, 0x1a, 0x2b, 0x3c, 0x4d, 0x5e]);
/// assert_eq!(m.to_string(), "00-1a-2b-3c-4d-5e");
/// assert_eq!(m.bytes()[1], 0x1a);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[doc(alias = "MACAddress")]
pub struct MacAddress([u8; 6]);

impl MacAddress {
    /// A MAC address from its bytes, in transmission order.
    ///
    /// ```
    /// use syrp_net::MacAddress;
    /// assert!(!MacAddress::new([2, 0, 0, 0, 0, 1]).is_null());
    /// ```
    pub const fn new(bytes: [u8; 6]) -> Self {
        Self(bytes)
    }

    /// The six bytes, in transmission order.
    ///
    /// ```
    /// use syrp_net::MacAddress;
    /// assert_eq!(MacAddress::new([1, 2, 3, 4, 5, 6]).bytes(), [1, 2, 3, 4, 5, 6]);
    /// ```
    pub const fn bytes(self) -> [u8; 6] {
        self.0
    }

    /// Whether every byte is zero.
    ///
    /// ```
    /// use syrp_net::MacAddress;
    /// assert!(MacAddress::default().is_null());
    /// ```
    pub const fn is_null(self) -> bool {
        matches!(self.0, [0, 0, 0, 0, 0, 0])
    }

    /// The address as lower-case hex bytes joined by `separator` (which may be empty).
    ///
    /// ```
    /// use syrp_net::MacAddress;
    /// let m = MacAddress::new([0xde, 0xad, 0xbe, 0xef, 0, 1]);
    /// assert_eq!(m.to_string_with(":"), "de:ad:be:ef:00:01");
    /// assert_eq!(m.to_string_with(""), "deadbeef0001");
    /// ```
    #[doc(alias = "toString")]
    pub fn to_string_with(self, separator: &str) -> String {
        let mut out = String::with_capacity(12 + 5 * separator.len());
        for (i, b) in self.0.iter().enumerate() {
            if i > 0 {
                out.push_str(separator);
            }
            out.push(hex_digit(b >> 4));
            out.push(hex_digit(b & 0xf));
        }
        out
    }

    /// Parses six two-digit hex bytes separated by `-`, by `:`, or by nothing. Hex digits may
    /// be either case and surrounding whitespace is ignored; separators cannot be mixed.
    ///
    /// # Errors
    /// [`NetError::InvalidMacAddress`] (SYRP-NET-0004) for anything else.
    ///
    /// ```
    /// use syrp_net::MacAddress;
    /// let m = MacAddress::new([0, 0x1a, 0x2b, 0x3c, 0x4d, 0x5e]);
    /// assert_eq!(MacAddress::parse("00-1a-2b-3c-4d-5e")?, m);
    /// assert_eq!(MacAddress::parse("00:1A:2B:3C:4D:5E")?, m);
    /// assert_eq!(MacAddress::parse("001a2b3c4d5e")?, m);
    /// assert!(MacAddress::parse("00-1a:2b-3c-4d-5e").is_err());
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub fn parse(text: &str) -> Result<Self, NetError> {
        let fail = |reason| NetError::InvalidMacAddress {
            input: text.to_owned(),
            reason,
        };
        let s = text.trim().as_bytes();
        let stride = match s.len() {
            12 => 2,
            17 => 3,
            _ => {
                return Err(fail(
                    "expected 12 hex digits, or 17 characters with separators",
                ));
            }
        };
        let separator = s.get(2).copied().filter(|_| stride == 3);
        if let Some(sep) = separator {
            if sep != b'-' && sep != b':' {
                return Err(fail("the separator must be `-` or `:`"));
            }
            if (1..6).any(|i| s[i * 3 - 1] != sep) {
                return Err(fail(
                    "separators must all be the same and between every byte",
                ));
            }
        }
        let mut bytes = [0u8; 6];
        for (i, byte) in bytes.iter_mut().enumerate() {
            let hi = hex_value(s[i * stride]).ok_or_else(|| fail("expected a hex digit"))?;
            let lo = hex_value(s[i * stride + 1]).ok_or_else(|| fail("expected a hex digit"))?;
            *byte = (hi << 4) | lo;
        }
        Ok(Self(bytes))
    }

    /// The hardware addresses of this machine's interfaces, without duplicates, excluding the
    /// null address and loopback interfaces. A machine (or container) may have none.
    ///
    /// # Errors
    /// [`NetError::InterfaceQuery`] when the OS query fails, [`NetError::Unsupported`] on
    /// targets without one.
    ///
    /// ```
    /// use syrp_net::MacAddress;
    /// let all = MacAddress::find_all_addresses()?;
    /// assert!(all.iter().all(|m| !m.is_null()));
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    #[doc(alias = "findAllAddresses")]
    pub fn find_all_addresses() -> Result<Vec<Self>, NetError> {
        let mut out = Vec::new();
        for entry in ifaces::interfaces()? {
            let Some(mac) = entry.mac.map(Self) else {
                continue;
            };
            if !entry.loopback && !mac.is_null() && !out.contains(&mac) {
                out.push(mac);
            }
        }
        Ok(out)
    }
}

fn hex_digit(nibble: u8) -> char {
    char::from(b"0123456789abcdef"[usize::from(nibble & 0xf)])
}

fn hex_value(c: u8) -> Option<u8> {
    char::from(c)
        .to_digit(16)
        .and_then(|d| u8::try_from(d).ok())
}

impl fmt::Display for MacAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_string_with("-"))
    }
}

impl FromStr for MacAddress {
    type Err = NetError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl From<[u8; 6]> for MacAddress {
    fn from(bytes: [u8; 6]) -> Self {
        Self(bytes)
    }
}

// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`IpAddress`]: an IPv4 or IPv6 address, and interface address enumeration.

use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

use crate::NetError;
use crate::ifaces;

/// An IP address family.
///
/// ```
/// use syrp_net::{IpAddress, IpFamily};
/// assert_eq!(IpAddress::local(IpFamily::V6).family(), IpFamily::V6);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum IpFamily {
    /// IPv4: 4 bytes.
    V4,
    /// IPv6: 16 bytes.
    V6,
}

/// An IPv4 or IPv6 address (JUCE's `IPAddress`).
///
/// It converts to and from [`std::net::IpAddr`] losslessly. Ordering puts every IPv4 address
/// before every IPv6 address, then compares bytes in network order; hashing agrees with
/// equality. [`Display`](fmt::Display) writes IPv4 as a dotted quad and IPv6 in the RFC 5952
/// recommended form (lower case, longest zero run compressed, IPv4-mapped addresses as
/// `::ffff:a.b.c.d`). The default value is the null IPv4 address `0.0.0.0`.
///
/// ```
/// use std::net::IpAddr;
/// use syrp_net::IpAddress;
/// let a = IpAddress::v4(192, 168, 0, 1);
/// let std: IpAddr = a.into();
/// assert_eq!(IpAddress::from(std), a);
/// assert!(IpAddress::v4(10, 0, 0, 1) < IpAddress::v6([0, 0, 0, 0, 0, 0, 0, 1]));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[doc(alias = "IPAddress")]
pub struct IpAddress(IpAddr);

impl IpAddress {
    /// An IPv4 address from its four bytes, most significant first.
    ///
    /// ```
    /// use syrp_net::IpAddress;
    /// assert_eq!(IpAddress::v4(127, 0, 0, 1).to_string(), "127.0.0.1");
    /// ```
    pub const fn v4(a: u8, b: u8, c: u8, d: u8) -> Self {
        Self(IpAddr::V4(Ipv4Addr::new(a, b, c, d)))
    }

    /// An IPv6 address from its eight 16-bit groups, most significant first.
    ///
    /// ```
    /// use syrp_net::IpAddress;
    /// assert_eq!(IpAddress::v6([0x2001, 0xdb8, 0, 0, 0, 0, 0, 1]).to_string(), "2001:db8::1");
    /// ```
    pub const fn v6(groups: [u16; 8]) -> Self {
        let [a, b, c, d, e, f, g, h] = groups;
        Self(IpAddr::V6(Ipv6Addr::new(a, b, c, d, e, f, g, h)))
    }

    /// Parses an IPv4 dotted quad or an IPv6 address in RFC 4291 text form. Surrounding
    /// whitespace is ignored; ports, brackets, zone indices and prefix lengths are rejected.
    ///
    /// # Errors
    /// [`NetError::InvalidIpAddress`] (SYRP-NET-0001) when the text is not an address.
    ///
    /// ```
    /// use syrp_net::IpAddress;
    /// assert_eq!(IpAddress::parse(" 10.0.0.7 ")?, IpAddress::v4(10, 0, 0, 7));
    /// assert!(IpAddress::parse("[::1]:80").is_err());
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub fn parse(text: &str) -> Result<Self, NetError> {
        text.trim()
            .parse::<IpAddr>()
            .map(Self)
            .map_err(|_| NetError::InvalidIpAddress {
                input: text.to_owned(),
            })
    }

    /// The wildcard address of a family: `0.0.0.0` or `::`. Binding to it listens on every
    /// interface.
    ///
    /// ```
    /// use syrp_net::{IpAddress, IpFamily};
    /// assert_eq!(IpAddress::any(IpFamily::V6).to_string(), "::");
    /// ```
    pub const fn any(family: IpFamily) -> Self {
        match family {
            IpFamily::V4 => Self(IpAddr::V4(Ipv4Addr::UNSPECIFIED)),
            IpFamily::V6 => Self(IpAddr::V6(Ipv6Addr::UNSPECIFIED)),
        }
    }

    /// The address that reaches every host on the local network: `255.255.255.255` for IPv4.
    /// IPv6 has no broadcast, so for IPv6 this is the link-local all-nodes multicast group
    /// `ff02::1`, which plays the same role.
    ///
    /// ```
    /// use syrp_net::{IpAddress, IpFamily};
    /// assert_eq!(IpAddress::broadcast(IpFamily::V4).to_string(), "255.255.255.255");
    /// assert_eq!(IpAddress::broadcast(IpFamily::V6).to_string(), "ff02::1");
    /// ```
    pub const fn broadcast(family: IpFamily) -> Self {
        match family {
            IpFamily::V4 => Self(IpAddr::V4(Ipv4Addr::BROADCAST)),
            IpFamily::V6 => Self::v6([0xff02, 0, 0, 0, 0, 0, 0, 1]),
        }
    }

    /// The loopback address of a family: `127.0.0.1` or `::1`.
    ///
    /// ```
    /// use syrp_net::{IpAddress, IpFamily};
    /// assert_eq!(IpAddress::local(IpFamily::V4).to_string(), "127.0.0.1");
    /// ```
    pub const fn local(family: IpFamily) -> Self {
        match family {
            IpFamily::V4 => Self(IpAddr::V4(Ipv4Addr::LOCALHOST)),
            IpFamily::V6 => Self(IpAddr::V6(Ipv6Addr::LOCALHOST)),
        }
    }

    /// This address's family.
    ///
    /// ```
    /// use syrp_net::{IpAddress, IpFamily};
    /// assert_eq!(IpAddress::v4(1, 2, 3, 4).family(), IpFamily::V4);
    /// ```
    pub const fn family(self) -> IpFamily {
        match self.0 {
            IpAddr::V4(_) => IpFamily::V4,
            IpAddr::V6(_) => IpFamily::V6,
        }
    }

    /// Whether this is an IPv6 address.
    ///
    /// ```
    /// use syrp_net::IpAddress;
    /// assert!(IpAddress::parse("::1")?.is_ipv6());
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub const fn is_ipv6(self) -> bool {
        matches!(self.0, IpAddr::V6(_))
    }

    /// Whether every byte is zero (`0.0.0.0` or `::`), JUCE's "null" address.
    ///
    /// ```
    /// use syrp_net::{IpAddress, IpFamily};
    /// assert!(IpAddress::default().is_null());
    /// assert!(IpAddress::any(IpFamily::V6).is_null());
    /// assert!(!IpAddress::v4(0, 0, 0, 1).is_null());
    /// ```
    pub const fn is_null(self) -> bool {
        self.0.is_unspecified()
    }

    /// Whether this is a loopback address (`127.0.0.0/8` or `::1`).
    ///
    /// ```
    /// use syrp_net::IpAddress;
    /// assert!(IpAddress::v4(127, 1, 2, 3).is_loopback());
    /// ```
    pub const fn is_loopback(self) -> bool {
        self.0.is_loopback()
    }

    /// Whether this is an IPv4-mapped IPv6 address (`::ffff:a.b.c.d`).
    ///
    /// ```
    /// use syrp_net::IpAddress;
    /// assert!(IpAddress::parse("::ffff:10.0.0.1")?.is_ipv4_mapped());
    /// assert!(!IpAddress::v4(10, 0, 0, 1).is_ipv4_mapped());
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub const fn is_ipv4_mapped(self) -> bool {
        match self.0 {
            IpAddr::V4(_) => false,
            IpAddr::V6(v6) => v6.to_ipv4_mapped().is_some(),
        }
    }

    /// The IPv4-mapped IPv6 form of an IPv4 address (`a.b.c.d` becomes `::ffff:a.b.c.d`). An
    /// IPv6 address is returned unchanged.
    ///
    /// ```
    /// use syrp_net::IpAddress;
    /// let m = IpAddress::v4(192, 0, 2, 1).to_ipv4_mapped();
    /// assert_eq!(m.to_string(), "::ffff:192.0.2.1");
    /// assert_eq!(m.to_ipv4_mapped(), m);
    /// ```
    pub const fn to_ipv4_mapped(self) -> Self {
        match self.0 {
            IpAddr::V4(v4) => Self(IpAddr::V6(v4.to_ipv6_mapped())),
            IpAddr::V6(_) => self,
        }
    }

    /// The IPv4 address behind this one: itself if it is IPv4, the embedded address if it is
    /// IPv4-mapped IPv6, and `None` for any other IPv6 address.
    ///
    /// ```
    /// use syrp_net::IpAddress;
    /// let v4 = IpAddress::v4(192, 0, 2, 1);
    /// assert_eq!(v4.to_ipv4_mapped().to_ipv4(), Some(v4));
    /// assert_eq!(IpAddress::parse("2001:db8::1")?.to_ipv4(), None);
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub const fn to_ipv4(self) -> Option<Self> {
        match self.0 {
            IpAddr::V4(_) => Some(self),
            IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
                Some(v4) => Some(Self(IpAddr::V4(v4))),
                None => None,
            },
        }
    }

    /// The equivalent [`std::net::IpAddr`].
    ///
    /// ```
    /// use std::net::{IpAddr, Ipv4Addr};
    /// use syrp_net::IpAddress;
    /// assert_eq!(IpAddress::v4(1, 2, 3, 4).to_std(), IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)));
    /// ```
    pub const fn to_std(self) -> IpAddr {
        self.0
    }

    /// Every address assigned to this machine's interfaces, loopback included, without
    /// duplicates, in the order the OS lists them. IPv6 addresses are included only when
    /// `include_ipv6` is true.
    ///
    /// # Errors
    /// [`NetError::InterfaceQuery`] when the OS query fails, [`NetError::Unsupported`] on
    /// targets without one.
    ///
    /// ```
    /// use syrp_net::IpAddress;
    /// let all = IpAddress::all_addresses(false)?;
    /// assert!(all.iter().all(|a| !a.is_ipv6()));
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    #[doc(alias = "findAllAddresses")]
    pub fn all_addresses(include_ipv6: bool) -> Result<Vec<Self>, NetError> {
        let mut out = Vec::new();
        for entry in ifaces::interfaces()? {
            let ip = Self(entry.ip);
            if (include_ipv6 || !ip.is_ipv6()) && !out.contains(&ip) {
                out.push(ip);
            }
        }
        Ok(out)
    }

    /// The machine's first non-loopback, non-null address (IPv4 first, then IPv6 when
    /// `include_ipv6` is true), or [`IpAddress::local`]`(IpFamily::V4)` when it has none.
    ///
    /// # Errors
    /// As [`IpAddress::all_addresses`].
    ///
    /// ```
    /// use syrp_net::IpAddress;
    /// let a = IpAddress::local_address(true)?;
    /// assert!(!a.is_null());
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    #[doc(alias = "getLocalAddress")]
    pub fn local_address(include_ipv6: bool) -> Result<Self, NetError> {
        let all = Self::all_addresses(include_ipv6)?;
        let real = |a: &&Self| !a.is_loopback() && !a.is_null();
        Ok(all
            .iter()
            .filter(|a| !a.is_ipv6())
            .find(real)
            .or_else(|| all.iter().find(real))
            .copied()
            .unwrap_or(Self::local(IpFamily::V4)))
    }

    /// The broadcast address of the interface that has `address` assigned, or `None` when no
    /// interface has it or that interface has no broadcast address (IPv6 interfaces never do).
    ///
    /// # Errors
    /// As [`IpAddress::all_addresses`].
    ///
    /// ```
    /// use syrp_net::IpAddress;
    /// let none = IpAddress::interface_broadcast_address(IpAddress::v4(203, 0, 113, 77))?;
    /// assert_eq!(none, None);
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    #[doc(alias = "getInterfaceBroadcastAddress")]
    pub fn interface_broadcast_address(address: Self) -> Result<Option<Self>, NetError> {
        Ok(ifaces::interfaces()?
            .into_iter()
            .find(|e| e.ip == address.0)
            .and_then(|e| e.broadcast)
            .map(|b| Self(IpAddr::V4(b))))
    }
}

impl Default for IpAddress {
    fn default() -> Self {
        Self::any(IpFamily::V4)
    }
}

impl fmt::Display for IpAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // std's formatting follows RFC 5952 (lower case, `::` for the longest run of two or
        // more zero groups, the first run on a tie, `::ffff:a.b.c.d` for mapped addresses).
        fmt::Display::fmt(&self.0, f)
    }
}

impl FromStr for IpAddress {
    type Err = NetError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl From<IpAddr> for IpAddress {
    fn from(a: IpAddr) -> Self {
        Self(a)
    }
}

impl From<Ipv4Addr> for IpAddress {
    fn from(a: Ipv4Addr) -> Self {
        Self(IpAddr::V4(a))
    }
}

impl From<Ipv6Addr> for IpAddress {
    fn from(a: Ipv6Addr) -> Self {
        Self(IpAddr::V6(a))
    }
}

impl From<IpAddress> for IpAddr {
    fn from(a: IpAddress) -> Self {
        a.0
    }
}

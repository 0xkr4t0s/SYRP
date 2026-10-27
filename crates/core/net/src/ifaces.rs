// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Network interface enumeration: `getifaddrs` on Linux, Android and Apple platforms,
//! `GetAdaptersAddresses` on Windows, and `NetError::Unsupported` elsewhere.

use std::net::{IpAddr, Ipv4Addr};

/// One assigned address from the OS interface list.
#[derive(Debug, Clone, Copy)]
pub(crate) struct IfEntry {
    /// The assigned IP address.
    pub(crate) ip: IpAddr,
    /// The IPv4 broadcast address of `ip`'s subnet, when the interface supports broadcast.
    pub(crate) broadcast: Option<Ipv4Addr>,
}

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
pub(crate) use unix::interfaces;
#[cfg(windows)]
pub(crate) use windows::interfaces;

/// Interface enumeration on targets without an implementation.
#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_vendor = "apple",
    windows
)))]
pub(crate) fn interfaces() -> Result<Vec<IfEntry>, crate::NetError> {
    Err(crate::NetError::Unsupported {
        operation: "network interface enumeration",
    })
}

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
mod unix {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    use super::IfEntry;
    use crate::NetError;

    /// Reads the IP address from a socket address of family `AF_INET` or `AF_INET6`.
    ///
    /// # Safety
    /// `sa` is null or points to a readable socket address whose length is at least that of
    /// the structure its `sa_family` names (`sockaddr_in` or `sockaddr_in6`); for other
    /// families only `sa_family` needs to be readable. No alignment is required.
    unsafe fn sockaddr_ip(sa: *const libc::sockaddr) -> Option<IpAddr> {
        if sa.is_null() {
            return None;
        }
        // SAFETY: `sa` is non-null and readable per this fn's contract; read_unaligned
        // needs no alignment.
        let family = i32::from(unsafe { sa.read_unaligned() }.sa_family);
        match family {
            libc::AF_INET => {
                // SAFETY: the family is AF_INET, so per the contract the whole sockaddr_in
                // is readable.
                let sin = unsafe { sa.cast::<libc::sockaddr_in>().read_unaligned() };
                let bytes = sin.sin_addr.s_addr.to_ne_bytes();
                Some(IpAddr::V4(Ipv4Addr::from(bytes)))
            }
            libc::AF_INET6 => {
                // SAFETY: the family is AF_INET6, so the whole sockaddr_in6 is readable.
                let sin6 = unsafe { sa.cast::<libc::sockaddr_in6>().read_unaligned() };
                Some(IpAddr::V6(Ipv6Addr::from(sin6.sin6_addr.s6_addr)))
            }
            _ => None,
        }
    }

    /// Lists the assigned addresses through `getifaddrs`.
    pub(crate) fn interfaces() -> Result<Vec<IfEntry>, NetError> {
        let mut head: *mut libc::ifaddrs = std::ptr::null_mut();
        // SAFETY: `head` is a valid, writable out-pointer; on success the list it receives is
        // freed exactly once below.
        if unsafe { libc::getifaddrs(&mut head) } != 0 {
            return Err(NetError::InterfaceQuery {
                os_error: std::io::Error::last_os_error().raw_os_error(),
            });
        }
        let mut out = Vec::new();
        let mut node = head.cast_const();
        while !node.is_null() {
            // SAFETY: `node` is a non-null element of the list getifaddrs returned, which
            // stays valid until freeifaddrs; the sockaddrs it points to are null or valid
            // for their family, as getifaddrs(3) guarantees.
            let (ip, bcast) = unsafe {
                let ifa = &*node;
                node = ifa.ifa_next;
                let can_broadcast = ifa.ifa_flags & libc::IFF_BROADCAST as libc::c_uint != 0;
                #[cfg(any(target_os = "linux", target_os = "android"))]
                let bcast = ifa.ifa_ifu;
                #[cfg(target_vendor = "apple")]
                let bcast = ifa.ifa_dstaddr;
                let bcast = if can_broadcast {
                    sockaddr_ip(bcast)
                } else {
                    None
                };
                (sockaddr_ip(ifa.ifa_addr), bcast)
            };
            let Some(ip) = ip else { continue };
            let broadcast = match (ip, bcast) {
                (IpAddr::V4(_), Some(IpAddr::V4(b))) => Some(b),
                _ => None,
            };
            out.push(IfEntry { ip, broadcast });
        }
        // SAFETY: `head` came from a successful getifaddrs and is freed only here.
        unsafe { libc::freeifaddrs(head) };
        Ok(out)
    }
}

#[cfg(windows)]
mod windows {
    use std::mem::offset_of;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    use windows_sys::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_NO_DATA, ERROR_SUCCESS};
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST,
        GetAdaptersAddresses, IF_TYPE_SOFTWARE_LOOPBACK, IP_ADAPTER_ADDRESSES_LH,
    };
    use windows_sys::Win32::Networking::WinSock::{
        AF_INET, AF_INET6, AF_UNSPEC, SOCKADDR, SOCKADDR_IN, SOCKADDR_IN6,
    };

    use super::IfEntry;
    use crate::NetError;

    /// Reads the IP address from a `SOCKADDR_IN` or `SOCKADDR_IN6`.
    ///
    /// # Safety
    /// `sa` is null or points to a readable socket address of at least the size of the
    /// structure its `sa_family` names; for other families only `sa_family` needs to be
    /// readable. No alignment is required.
    unsafe fn sockaddr_ip(sa: *const SOCKADDR) -> Option<IpAddr> {
        if sa.is_null() {
            return None;
        }
        // SAFETY: non-null and readable per the contract; no alignment is assumed.
        let family = unsafe { sa.read_unaligned() }.sa_family;
        let base = sa.cast::<u8>();
        if family == AF_INET {
            let at = base.wrapping_add(offset_of!(SOCKADDR_IN, sin_addr));
            // SAFETY: the family is AF_INET, so a whole SOCKADDR_IN is readable and its
            // 4-byte sin_addr starts at `at`; no alignment is assumed.
            let b = unsafe { at.cast::<[u8; 4]>().read_unaligned() };
            Some(IpAddr::V4(Ipv4Addr::from(b)))
        } else if family == AF_INET6 {
            let at = base.wrapping_add(offset_of!(SOCKADDR_IN6, sin6_addr));
            // SAFETY: the family is AF_INET6, so a whole SOCKADDR_IN6 is readable and its
            // 16-byte sin6_addr starts at `at`; no alignment is assumed.
            let b = unsafe { at.cast::<[u8; 16]>().read_unaligned() };
            Some(IpAddr::V6(Ipv6Addr::from(b)))
        } else {
            None
        }
    }

    /// Lists the assigned addresses through `GetAdaptersAddresses`.
    pub(crate) fn interfaces() -> Result<Vec<IfEntry>, NetError> {
        let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
        let mut size: u32 = 16 * 1024;
        // The required size can grow between calls when adapters appear, so retry a few times.
        for _ in 0..4 {
            let words = usize::try_from(size).unwrap_or(usize::MAX).div_ceil(8);
            // u64 words give the 8-byte alignment IP_ADAPTER_ADDRESSES_LH needs.
            let mut buf: Vec<u64> = Vec::new();
            buf.try_reserve_exact(words)
                .map_err(|_| NetError::InterfaceQuery { os_error: None })?;
            buf.resize(words, 0);
            let first = buf.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
            let family = u32::from(AF_UNSPEC);
            // SAFETY: `first` is non-null, 8-byte aligned and points to words * 8 >= size
            // writable bytes; `size` is a valid in/out pointer; the reserved argument is null.
            let rc =
                unsafe { GetAdaptersAddresses(family, flags, std::ptr::null(), first, &mut size) };
            match rc {
                // SAFETY: on success the buffer holds a valid adapter list starting at `first`,
                // and `buf` outlives the walk.
                ERROR_SUCCESS => return Ok(unsafe { walk(first) }),
                ERROR_NO_DATA => return Ok(Vec::new()),
                ERROR_BUFFER_OVERFLOW => continue,
                e => {
                    let os_error = i32::try_from(e).ok();
                    return Err(NetError::InterfaceQuery { os_error });
                }
            }
        }
        let os_error = i32::try_from(ERROR_BUFFER_OVERFLOW).ok();
        Err(NetError::InterfaceQuery { os_error })
    }

    /// Converts the adapter list into address records.
    ///
    /// # Safety
    /// `first` is the head of a list filled in by a successful `GetAdaptersAddresses` call,
    /// still alive and unmodified.
    unsafe fn walk(first: *const IP_ADAPTER_ADDRESSES_LH) -> Vec<IfEntry> {
        let mut out = Vec::new();
        let mut adapter = first;
        while !adapter.is_null() {
            // SAFETY: a non-null node of a valid adapter list (this fn's contract).
            let a = unsafe { &*adapter };
            let loopback = a.IfType == IF_TYPE_SOFTWARE_LOOPBACK;
            let mut unicast = a.FirstUnicastAddress.cast_const();
            while !unicast.is_null() {
                // SAFETY: a non-null node of the adapter's unicast list, valid with the
                // adapter list; its lpSockaddr is null or a valid SOCKADDR_IN / SOCKADDR_IN6.
                let (ip, prefix) = unsafe {
                    let u = &*unicast;
                    unicast = u.Next.cast_const();
                    (sockaddr_ip(u.Address.lpSockaddr), u.OnLinkPrefixLength)
                };
                let Some(ip) = ip else { continue };
                // Windows reports no broadcast address; derive it from the on-link prefix.
                let broadcast = match ip {
                    IpAddr::V4(v4) if !loopback && prefix < 32 => {
                        let host_bits = u32::MAX.checked_shr(u32::from(prefix)).unwrap_or(0);
                        Some(Ipv4Addr::from(u32::from(v4) | host_bits))
                    }
                    _ => None,
                };
                out.push(IfEntry { ip, broadcast });
            }
            adapter = a.Next.cast_const();
        }
        out
    }
}

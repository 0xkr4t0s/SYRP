// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Network interface enumeration: `getifaddrs` on Linux, Android and Apple platforms,
//! `GetAdaptersAddresses` on Windows, and `NetError::Unsupported` elsewhere.

use std::net::{IpAddr, Ipv4Addr};

/// One record from the OS interface list. An interface usually yields several: one per
/// assigned IP address, plus (on Unix) one for its link-layer address.
#[derive(Debug, Clone, Copy)]
pub(crate) struct IfEntry {
    /// The assigned IP address, if this record has one.
    pub(crate) ip: Option<IpAddr>,
    /// The IPv4 broadcast address of `ip`'s subnet, when the interface supports broadcast.
    pub(crate) broadcast: Option<Ipv4Addr>,
    /// The 6-byte hardware address, if this record has one.
    pub(crate) mac: Option<[u8; 6]>,
    /// Whether the interface is a loopback interface.
    pub(crate) loopback: bool,
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
    /// `sa` is null or points to a socket address whose `sa_family` field (and, on Apple
    /// platforms, `sa_len` field) is readable. Beyond those fields, the address must be
    /// readable for its full length: on Apple that is `sa_len` bytes, and elsewhere it is
    /// the size of the structure `sa_family` names. No alignment is required.
    unsafe fn sockaddr_ip(sa: *const libc::sockaddr) -> Option<IpAddr> {
        use std::mem::size_of;
        use std::ptr::addr_of;
        if sa.is_null() {
            return None;
        }
        // SAFETY: `sa` is non-null and its sa_family field is readable (contract); addr_of!
        // reads only that field, without a reference, and read_unaligned needs no alignment.
        let family = i32::from(unsafe { addr_of!((*sa).sa_family).read_unaligned() });
        // Apple can return sockaddrs shorter than their family's structure; sa_len says how
        // many bytes are really there. Elsewhere the full structure is readable.
        #[cfg(target_vendor = "apple")]
        // SAFETY: as above, for the sa_len field.
        let len = usize::from(unsafe { addr_of!((*sa).sa_len).read_unaligned() });
        #[cfg(not(target_vendor = "apple"))]
        let len = usize::MAX;
        match family {
            libc::AF_INET if len >= size_of::<libc::sockaddr_in>() => {
                // SAFETY: the family is AF_INET and the length check passed, so per the
                // contract the whole sockaddr_in is readable.
                let sin = unsafe { sa.cast::<libc::sockaddr_in>().read_unaligned() };
                let bytes = sin.sin_addr.s_addr.to_ne_bytes();
                Some(IpAddr::V4(Ipv4Addr::from(bytes)))
            }
            libc::AF_INET6 if len >= size_of::<libc::sockaddr_in6>() => {
                // SAFETY: the family is AF_INET6 and the length check passed, so the whole
                // sockaddr_in6 is readable.
                let sin6 = unsafe { sa.cast::<libc::sockaddr_in6>().read_unaligned() };
                Some(IpAddr::V6(Ipv6Addr::from(sin6.sin6_addr.s6_addr)))
            }
            _ => None,
        }
    }

    /// Reads a 6-byte link-layer address from an `AF_PACKET` socket address.
    ///
    /// # Safety
    /// As [`sockaddr_ip`]; for `AF_PACKET` the whole `sockaddr_ll` must be readable.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    unsafe fn sockaddr_mac(sa: *const libc::sockaddr) -> Option<[u8; 6]> {
        if sa.is_null() {
            return None;
        }
        // SAFETY: non-null, and the contract makes sa_family readable; addr_of! reads just
        // that field, without a reference or alignment.
        let family = unsafe { std::ptr::addr_of!((*sa).sa_family).read_unaligned() };
        if i32::from(family) != libc::AF_PACKET {
            return None;
        }
        // SAFETY: the family is AF_PACKET, so the whole sockaddr_ll is readable.
        let ll = unsafe { sa.cast::<libc::sockaddr_ll>().read_unaligned() };
        let mut mac = [0u8; 6];
        mac.copy_from_slice(&ll.sll_addr[..6]);
        (ll.sll_halen == 6).then_some(mac)
    }

    /// Reads a 6-byte link-layer address from an `AF_LINK` socket address.
    ///
    /// # Safety
    /// As [`sockaddr_ip`]: `sa_len` bytes from `sa` are readable.
    #[cfg(target_vendor = "apple")]
    unsafe fn sockaddr_mac(sa: *const libc::sockaddr) -> Option<[u8; 6]> {
        use std::mem::offset_of;
        use std::ptr::addr_of;
        if sa.is_null() {
            return None;
        }
        // SAFETY: non-null, and sa_len and sa_family are readable per the contract; addr_of!
        // reads only those fields, without a reference or alignment.
        let (len, family) = unsafe {
            let len = addr_of!((*sa).sa_len).read_unaligned();
            (usize::from(len), addr_of!((*sa).sa_family).read_unaligned())
        };
        // The fixed header ends where the variable-length sdl_data begins.
        let header = offset_of!(libc::sockaddr_dl, sdl_data);
        if i32::from(family) != libc::AF_LINK || len < header {
            return None;
        }
        let dl = sa.cast::<libc::sockaddr_dl>();
        // SAFETY: sdl_nlen and sdl_alen lie inside the header, and len >= header bytes are
        // readable; addr_of! reads only those fields.
        let (nlen, alen) = unsafe {
            let nlen = addr_of!((*dl).sdl_nlen).read_unaligned();
            (nlen, addr_of!((*dl).sdl_alen).read_unaligned())
        };
        // The address follows the interface name inside sdl_data.
        let start = header + usize::from(nlen);
        if alen != 6 || start + 6 > len {
            return None;
        }
        let mut mac = [0u8; 6];
        // SAFETY: start + 6 <= sa_len, and sa_len bytes from sa are readable per the
        // contract; the destination is a distinct 6-byte local array.
        unsafe { std::ptr::copy_nonoverlapping(sa.cast::<u8>().add(start), mac.as_mut_ptr(), 6) };
        Some(mac)
    }

    /// Lists the interface records through `getifaddrs`.
    pub(crate) fn interfaces() -> Result<Vec<IfEntry>, NetError> {
        let mut head = std::mem::MaybeUninit::<*mut libc::ifaddrs>::uninit();
        // SAFETY: `head` is a valid, writable out-pointer for getifaddrs; on success the list
        // it receives is freed exactly once below.
        if unsafe { libc::getifaddrs(head.as_mut_ptr()) } != 0 {
            return Err(NetError::InterfaceQuery {
                os_error: std::io::Error::last_os_error().raw_os_error(),
            });
        }
        // SAFETY: getifaddrs returned 0, so it wrote the list head (possibly null for an
        // empty list).
        let head = unsafe { head.assume_init() };
        let mut out = Vec::new();
        let mut node = head.cast_const();
        while !node.is_null() {
            // SAFETY: `node` is a non-null element of the list getifaddrs returned, which
            // stays valid until freeifaddrs; the sockaddrs it points to are null or valid
            // for their family, as getifaddrs(3) guarantees.
            let entry = unsafe {
                let ifa = &*node;
                node = ifa.ifa_next;
                let loopback = ifa.ifa_flags & libc::IFF_LOOPBACK as libc::c_uint != 0;
                let can_broadcast = ifa.ifa_flags & libc::IFF_BROADCAST as libc::c_uint != 0;
                #[cfg(any(target_os = "linux", target_os = "android"))]
                let bcast = ifa.ifa_ifu;
                #[cfg(target_vendor = "apple")]
                let bcast = ifa.ifa_dstaddr;
                let ip = sockaddr_ip(ifa.ifa_addr);
                let broadcast = match (can_broadcast, sockaddr_ip(bcast)) {
                    (true, Some(IpAddr::V4(b))) if matches!(ip, Some(IpAddr::V4(_))) => Some(b),
                    _ => None,
                };
                let mac = sockaddr_mac(ifa.ifa_addr);
                IfEntry {
                    ip,
                    broadcast,
                    mac,
                    loopback,
                }
            };
            out.push(entry);
        }
        // SAFETY: `head` came from a successful getifaddrs and is freed only here.
        unsafe { libc::freeifaddrs(head) };
        Ok(out)
    }
}

#[cfg(windows)]
mod windows {
    use std::mem::{offset_of, size_of};
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
    /// `sa` is null or points to `len` readable bytes holding a socket address, and its
    /// `sa_family` field is readable whatever `len` says. No alignment is required.
    unsafe fn sockaddr_ip(sa: *const SOCKADDR, len: usize) -> Option<IpAddr> {
        if sa.is_null() {
            return None;
        }
        // SAFETY: non-null and sa_family is readable per the contract; addr_of! reads only
        // that field, without a reference, and no alignment is assumed.
        let family = unsafe { std::ptr::addr_of!((*sa).sa_family).read_unaligned() };
        let base = sa.cast::<u8>();
        if family == AF_INET && len >= size_of::<SOCKADDR_IN>() {
            let at = base.wrapping_add(offset_of!(SOCKADDR_IN, sin_addr));
            // SAFETY: `len` covers a whole SOCKADDR_IN, so its 4-byte sin_addr, which starts
            // at `at`, is readable; no alignment is assumed.
            let b = unsafe { at.cast::<[u8; 4]>().read_unaligned() };
            Some(IpAddr::V4(Ipv4Addr::from(b)))
        } else if family == AF_INET6 && len >= size_of::<SOCKADDR_IN6>() {
            let at = base.wrapping_add(offset_of!(SOCKADDR_IN6, sin6_addr));
            // SAFETY: `len` covers a whole SOCKADDR_IN6, so its 16-byte sin6_addr, which
            // starts at `at`, is readable; no alignment is assumed.
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

    /// Converts the adapter list into interface records.
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
            if a.PhysicalAddressLength == 6 {
                let mut mac = [0u8; 6];
                mac.copy_from_slice(&a.PhysicalAddress[..6]);
                let (ip, broadcast, mac) = (None, None, Some(mac));
                out.push(IfEntry {
                    ip,
                    broadcast,
                    mac,
                    loopback,
                });
            }
            let mut unicast = a.FirstUnicastAddress.cast_const();
            while !unicast.is_null() {
                // SAFETY: a non-null node of the adapter's unicast list, valid with the
                // adapter list; its lpSockaddr is null or points to iSockaddrLength readable
                // bytes of socket address, which always include sa_family.
                let (ip, prefix) = unsafe {
                    let u = &*unicast;
                    unicast = u.Next.cast_const();
                    let len = usize::try_from(u.Address.iSockaddrLength).unwrap_or(0);
                    (sockaddr_ip(u.Address.lpSockaddr, len), u.OnLinkPrefixLength)
                };
                // Windows reports no broadcast address; derive it from the on-link prefix.
                let broadcast = match ip {
                    Some(IpAddr::V4(v4)) if !loopback && prefix < 32 => {
                        let host_bits = u32::MAX.checked_shr(u32::from(prefix)).unwrap_or(0);
                        Some(Ipv4Addr::from(u32::from(v4) | host_bits))
                    }
                    _ => None,
                };
                out.push(IfEntry {
                    ip,
                    broadcast,
                    mac: None,
                    loopback,
                });
            }
            adapter = a.Next.cast_const();
        }
        out
    }
}

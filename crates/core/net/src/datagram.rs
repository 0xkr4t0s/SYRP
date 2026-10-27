// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`DatagramSocket`] (UDP over IPv4).

use std::io;
use std::mem::MaybeUninit;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use socket2::{Domain, MaybeUninitSlice, Protocol, SockAddr, Socket, Type};

use crate::socket::{not_supported, resolve, retryable, socket_error, wait_ready};
use crate::{IpAddress, IpFamily, NetError, Readiness, SocketOptions};

/// A UDP socket over IPv4 (JUCE's `DatagramSocket`).
///
/// Bind it with [`bind_to_port`](Self::bind_to_port) to receive; an unbound socket can still
/// [`write`](Self::write), and the OS then picks its port. Every method takes `&self`, and
/// [`close`](Self::close) from another thread ends a blocked [`read`](Self::read) or
/// [`wait_until_ready`](Self::wait_until_ready) with [`NetError::Closed`].
///
/// ```
/// use syrp_net::DatagramSocket;
/// let rx = DatagramSocket::new(false)?;
/// rx.bind_to_port(0, "127.0.0.1")?;
/// let port = rx.bound_port().expect("bound");
/// DatagramSocket::new(false)?.write("127.0.0.1", port, b"hi")?;
/// let mut buf = [0u8; 16];
/// let (n, from, _) = rx.read(&mut buf, true)?;
/// assert_eq!((&buf[..n], from.to_string().as_str()), (&b"hi"[..], "127.0.0.1"));
/// # Ok::<(), syrp_net::NetError>(())
/// ```
#[derive(Debug)]
pub struct DatagramSocket {
    sock: Socket,
    closed: AtomicBool,
}

impl DatagramSocket {
    /// An unbound UDP socket, allowed to send to broadcast addresses when
    /// `enable_broadcasting` is true.
    ///
    /// # Errors
    /// [`NetError::Socket`] when the OS cannot create the socket.
    ///
    /// ```
    /// let s = syrp_net::DatagramSocket::new(true)?;
    /// assert_eq!(s.bound_port(), None);
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub fn new(enable_broadcasting: bool) -> Result<Self, NetError> {
        Self::new_with_options(enable_broadcasting, SocketOptions::new())
    }

    /// [`new`](Self::new), applying `options` on creation.
    ///
    /// # Errors
    /// As [`new`](Self::new), and when the OS rejects an option.
    ///
    /// ```
    /// use syrp_net::{DatagramSocket, SocketOptions};
    /// let o = SocketOptions::new().with_receive_buffer_size(1 << 16);
    /// let s = DatagramSocket::new_with_options(false, o)?;
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub fn new_with_options(
        enable_broadcasting: bool,
        options: SocketOptions,
    ) -> Result<Self, NetError> {
        let create = || {
            let sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
            options.apply(&sock)?;
            sock.set_broadcast(enable_broadcasting)?;
            // Non-blocking, so a datagram that vanishes between the readiness wait and the
            // receive (another reader took it) sends the reader back to the wait, which
            // notices `close`, instead of blocking in the OS.
            sock.set_nonblocking(true)?;
            Ok(sock)
        };
        let sock = create().map_err(|e| socket_error("create", &e))?;
        let closed = AtomicBool::new(false);
        Ok(Self { sock, closed })
    }

    /// Binds to `port` (0 picks a free port) at `local_addr`, an IPv4 address or empty for
    /// every interface. On Unix the address may be shared (`SO_REUSEADDR`), so several
    /// multicast receivers can bind one port.
    ///
    /// # Errors
    /// [`NetError::Resolve`] when `local_addr` is not an IPv4 address, [`NetError::Socket`]
    /// when binding fails (for example when already bound), [`NetError::Closed`] once closed.
    ///
    /// ```
    /// let s = syrp_net::DatagramSocket::new(false)?;
    /// s.bind_to_port(0, "")?;
    /// assert!(s.bound_port().is_some());
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub fn bind_to_port(&self, port: u16, local_addr: &str) -> Result<(), NetError> {
        self.check_open()?;
        let ip = match local_addr.trim() {
            "" => Some(IpAddress::any(IpFamily::V4)),
            text => IpAddress::parse(text).ok().filter(|ip| !ip.is_ipv6()),
        };
        let ip = ip.ok_or_else(|| NetError::Resolve {
            host: local_addr.to_owned(),
            os_error: None,
        })?;
        let addr = SockAddr::from(SocketAddr::new(ip.into(), port));
        #[cfg(unix)]
        let bound = self
            .sock
            .set_reuse_address(true)
            .and_then(|()| self.sock.bind(&addr));
        #[cfg(windows)]
        let bound = self.sock.bind(&addr);
        bound.map_err(|e| socket_error("bind", &e))
    }

    /// The local port, or `None` while unbound or once closed.
    ///
    /// ```
    /// let s = syrp_net::DatagramSocket::new(false)?;
    /// s.bind_to_port(0, "127.0.0.1")?;
    /// assert!(s.bound_port().is_some_and(|p| p != 0));
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub fn bound_port(&self) -> Option<u16> {
        let port = self.sock.local_addr().ok()?.as_socket()?.port();
        (port != 0 && self.check_open().is_ok()).then_some(port)
    }

    /// Receives one datagram into `buf` and returns its length (truncated to `buf`) with the
    /// sender's address and port. With `block` false it does not wait: when nothing is
    /// queued it returns `(0, 0.0.0.0, 0)`.
    ///
    /// # Errors
    /// [`NetError::Closed`] when the socket is (or becomes) closed, [`NetError::Socket`]
    /// with [`ErrorKind::NotConnected`](std::io::ErrorKind::NotConnected) when it has no
    /// port yet (neither bound nor written from), and [`NetError::Socket`] when the OS read
    /// fails.
    ///
    /// ```
    /// let s = syrp_net::DatagramSocket::new(false)?;
    /// s.bind_to_port(0, "127.0.0.1")?;
    /// assert_eq!(s.read(&mut [0u8; 8], false)?.0, 0);
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub fn read(&self, buf: &mut [u8], block: bool) -> Result<(usize, IpAddress, u16), NetError> {
        if self.bound_port().is_none() {
            self.check_open()?;
            return Err(socket_error("read", &io::ErrorKind::NotConnected.into()));
        }
        let nothing = (0, IpAddress::any(IpFamily::V4), 0);
        let timeout = (!block).then_some(Duration::ZERO);
        // SAFETY: `[u8]` and `[MaybeUninit<u8>]` have the same size, alignment and layout,
        // the reborrow lives no longer than `buf`, and the OS only ever writes initialised
        // bytes into it, so `buf` is still fully initialised afterwards.
        let uninit = unsafe { &mut *(std::ptr::from_mut(buf) as *mut [MaybeUninit<u8>]) };
        let mut bufs = [MaybeUninitSlice::new(uninit)];
        loop {
            if self.wait_until_ready(true, timeout)? == Readiness::TimedOut {
                return Ok(nothing);
            }
            // The vectored form truncates an oversized datagram on every OS (Windows would
            // otherwise fail with WSAEMSGSIZE).
            match self.sock.recv_from_vectored(&mut bufs) {
                Ok((n, _, from)) => {
                    let from = from.as_socket();
                    let ip = from.map_or(IpAddress::any(IpFamily::V4), |a| a.ip().into());
                    return Ok((n.min(buf.len()), ip, from.map_or(0, |a| a.port())));
                }
                Err(e) if retryable(&e) && !block => return Ok(nothing),
                Err(e) if retryable(&e) => {}
                Err(e) => return Err(self.error("read", &e)),
            }
        }
    }

    /// Sends `buf` as one datagram to `host` (a name or an IPv4 address) on `port`, and
    /// returns its length.
    ///
    /// # Errors
    /// [`NetError::Resolve`] when `host` has no IPv4 address, [`NetError::Socket`] when the
    /// OS send fails (for example a broadcast address without broadcasting enabled),
    /// [`NetError::Closed`] once closed.
    ///
    /// ```no_run
    /// let s = syrp_net::DatagramSocket::new(true)?;
    /// s.write("255.255.255.255", 9000, b"discover")?;
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub fn write(&self, host: &str, port: u16, buf: &[u8]) -> Result<usize, NetError> {
        self.check_open()?;
        let target = resolve(host, port)?.into_iter().find(SocketAddr::is_ipv4);
        let target = target.ok_or_else(|| NetError::Resolve {
            host: host.to_owned(),
            os_error: None,
        })?;
        loop {
            match self.sock.send_to(buf, &SockAddr::from(target)) {
                Ok(n) => return Ok(n),
                Err(e) if retryable(&e) => {
                    self.wait_until_ready(false, None)?;
                }
                Err(e) => return Err(self.error("write", &e)),
            }
        }
    }

    /// Joins the IPv4 multicast `group` on the default interface.
    ///
    /// # Errors
    /// [`NetError::Socket`] when the OS refuses (for example when the machine has no
    /// multicast route) or `group` is IPv6; [`NetError::Closed`] once closed.
    ///
    /// ```no_run
    /// use syrp_net::{DatagramSocket, IpAddress};
    /// let s = DatagramSocket::new(false)?;
    /// s.bind_to_port(9000, "")?;
    /// s.join_multicast(IpAddress::parse("239.255.0.1")?)?;
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub fn join_multicast(&self, group: IpAddress) -> Result<(), NetError> {
        let group = self.group(group, "join multicast")?;
        let joined = self.sock.join_multicast_v4(&group, &Ipv4Addr::UNSPECIFIED);
        joined.map_err(|e| self.error("join multicast", &e))
    }

    /// Leaves an IPv4 multicast `group` joined with [`join_multicast`](Self::join_multicast).
    ///
    /// # Errors
    /// As [`join_multicast`](Self::join_multicast); also when the group was not joined.
    ///
    /// ```no_run
    /// # let s = syrp_net::DatagramSocket::new(false)?;
    /// s.leave_multicast(syrp_net::IpAddress::parse("239.255.0.1")?)?;
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub fn leave_multicast(&self, group: IpAddress) -> Result<(), NetError> {
        let group = self.group(group, "leave multicast")?;
        let left = self.sock.leave_multicast_v4(&group, &Ipv4Addr::UNSPECIFIED);
        left.map_err(|e| self.error("leave multicast", &e))
    }

    /// Whether multicast datagrams this socket sends are also delivered to this machine.
    ///
    /// # Errors
    /// [`NetError::Socket`] when the OS rejects the option, [`NetError::Closed`] once closed.
    ///
    /// ```
    /// syrp_net::DatagramSocket::new(false)?.set_multicast_loopback_enabled(false)?;
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub fn set_multicast_loopback_enabled(&self, enabled: bool) -> Result<(), NetError> {
        self.check_open()?;
        let set = self.sock.set_multicast_loop_v4(enabled);
        set.map_err(|e| self.error("set multicast loopback", &e))
    }

    /// Whether this socket may send to broadcast addresses (`SO_BROADCAST`).
    ///
    /// # Errors
    /// [`NetError::Socket`] when the OS rejects the option, [`NetError::Closed`] once closed.
    ///
    /// ```
    /// syrp_net::DatagramSocket::new(false)?.set_enable_broadcasting(true)?;
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub fn set_enable_broadcasting(&self, enabled: bool) -> Result<(), NetError> {
        self.check_open()?;
        let set = self.sock.set_broadcast(enabled);
        set.map_err(|e| self.error("set broadcast", &e))
    }

    /// Waits until a datagram can be read (`for_reading`) or sent, for at most `timeout`
    /// (`None` waits indefinitely).
    ///
    /// # Errors
    /// [`NetError::Closed`] when the socket is (or becomes) closed, [`NetError::Socket`] when
    /// the OS wait fails.
    ///
    /// ```
    /// use std::time::Duration;
    /// use syrp_net::{DatagramSocket, Readiness};
    /// let s = DatagramSocket::new(false)?;
    /// s.bind_to_port(0, "127.0.0.1")?;
    /// let r = s.wait_until_ready(true, Some(Duration::from_millis(10)))?;
    /// assert_eq!(r, Readiness::TimedOut);
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub fn wait_until_ready(
        &self,
        for_reading: bool,
        timeout: Option<Duration>,
    ) -> Result<Readiness, NetError> {
        wait_ready(&self.sock, &self.closed, for_reading, timeout)
    }

    /// Closes the socket: further calls fail with [`NetError::Closed`], and calls blocked in
    /// other threads return within a few tens of milliseconds. Closing twice does nothing.
    ///
    /// ```
    /// let s = syrp_net::DatagramSocket::new(false)?;
    /// s.close();
    /// assert!(s.write("127.0.0.1", 9, b"x").is_err());
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub fn close(&self) {
        self.closed.store(true, Ordering::Release);
    }

    fn check_open(&self) -> Result<(), NetError> {
        match self.closed.load(Ordering::Acquire) {
            true => Err(NetError::Closed),
            false => Ok(()),
        }
    }

    /// An OS failure, reported as [`NetError::Closed`] when a close caused it.
    fn error(&self, operation: &'static str, e: &io::Error) -> NetError {
        self.check_open()
            .map_or_else(|closed| closed, |()| socket_error(operation, e))
    }

    fn group(&self, group: IpAddress, operation: &'static str) -> Result<Ipv4Addr, NetError> {
        self.check_open()?;
        match IpAddr::from(group) {
            IpAddr::V4(v4) => Ok(v4),
            IpAddr::V6(_) => Err(not_supported(operation)),
        }
    }
}

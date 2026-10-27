// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`StreamingSocket`] (TCP).

use std::io::{self, Read};
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use socket2::{Domain, Protocol, SockAddr, Socket, Type};

use crate::{IpAddress, NetError, Readiness, SocketOptions};

/// How long one blocking wait lasts before it checks again whether the socket was closed.
pub(crate) const SLICE: Duration = Duration::from_millis(50);

/// `send` flags: Linux and Android suppress `SIGPIPE` per call; Apple sets `SO_NOSIGPIPE`
/// on the socket instead, and Windows has no `SIGPIPE`.
#[cfg(any(target_os = "linux", target_os = "android"))]
const SEND_FLAGS: libc::c_int = libc::MSG_NOSIGNAL;
#[cfg(not(any(target_os = "linux", target_os = "android")))]
const SEND_FLAGS: i32 = 0;

/// A TCP socket: either a connection or a listener (JUCE's `StreamingSocket`).
///
/// Every method takes `&self`, so one thread can [`close`](Self::close) a socket that another
/// is blocked on: the blocked [`read`](Self::read), [`wait_until_ready`](Self::wait_until_ready)
/// or [`wait_for_next_connection`](Self::wait_for_next_connection) then returns
/// [`NetError::Closed`]. The OS handle is released when the value is dropped.
///
/// ```
/// use syrp_net::StreamingSocket;
/// let listener = StreamingSocket::create_listener(0, "127.0.0.1")?;
/// let port = listener.bound_port().expect("bound");
/// let client = StreamingSocket::connect("127.0.0.1", port, None)?;
/// let server = listener.wait_for_next_connection()?;
/// client.write(b"ping")?;
/// let mut buf = [0u8; 4];
/// assert_eq!(server.read(&mut buf, true)?, 4);
/// assert_eq!(&buf, b"ping");
/// # Ok::<(), syrp_net::NetError>(())
/// ```
#[derive(Debug)]
pub struct StreamingSocket {
    sock: Socket,
    listener: bool,
    host: String,
    port: u16,
    closed: AtomicBool,
}

impl StreamingSocket {
    /// Connects to `host` (a name or an address) on `port`, trying each resolved address in
    /// turn. `timeout` bounds each attempt; `None` uses the OS default.
    ///
    /// # Errors
    /// [`NetError::Resolve`] when `host` does not resolve, [`NetError::Socket`] when no
    /// address accepts the connection.
    ///
    /// ```no_run
    /// use std::time::Duration;
    /// use syrp_net::StreamingSocket;
    /// let s = StreamingSocket::connect("example.com", 80, Some(Duration::from_secs(5)))?;
    /// assert!(s.is_connected());
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub fn connect(host: &str, port: u16, timeout: Option<Duration>) -> Result<Self, NetError> {
        Self::connect_with_options(host, port, timeout, SocketOptions::new())
    }

    /// [`connect`](Self::connect), applying `options` before connecting.
    ///
    /// # Errors
    /// As [`connect`](Self::connect).
    ///
    /// ```no_run
    /// use syrp_net::{SocketOptions, StreamingSocket};
    /// let o = SocketOptions::new().with_send_buffer_size(1 << 16);
    /// let s = StreamingSocket::connect_with_options("localhost", 8080, None, o)?;
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub fn connect_with_options(
        host: &str,
        port: u16,
        timeout: Option<Duration>,
        options: SocketOptions,
    ) -> Result<Self, NetError> {
        let mut last = NetError::Resolve {
            host: host.to_owned(),
            os_error: None,
        };
        for addr in resolve(host, port)? {
            let attempt = new_socket(addr, options).and_then(|sock| {
                match timeout {
                    Some(t) => sock.connect_timeout(&SockAddr::from(addr), t)?,
                    None => sock.connect(&SockAddr::from(addr))?,
                }
                Ok(sock)
            });
            match attempt {
                Ok(sock) => return Ok(Self::wrap(sock, false, host.to_owned(), port)),
                Err(e) => last = socket_error("connect", &e),
            }
        }
        Err(last)
    }

    /// A listener on `port` (0 picks a free port; see [`bound_port`](Self::bound_port)) at
    /// `local_host`, which is an address to bind to or empty for every IPv4 interface. Use
    /// `"::"` to listen on IPv6.
    ///
    /// # Errors
    /// [`NetError::Resolve`] for a bad `local_host`, [`NetError::Socket`] when binding or
    /// listening fails (for example when the port is taken).
    ///
    /// ```
    /// use syrp_net::StreamingSocket;
    /// let l = StreamingSocket::create_listener(0, "")?;
    /// assert_ne!(l.bound_port(), Some(0));
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub fn create_listener(port: u16, local_host: &str) -> Result<Self, NetError> {
        Self::create_listener_with_options(port, local_host, SocketOptions::new())
    }

    /// [`create_listener`](Self::create_listener), applying `options` to the listener (and so
    /// to the connections it accepts).
    ///
    /// # Errors
    /// As [`create_listener`](Self::create_listener).
    ///
    /// ```
    /// use syrp_net::{SocketOptions, StreamingSocket};
    /// let o = SocketOptions::new().with_receive_buffer_size(1 << 16);
    /// let l = StreamingSocket::create_listener_with_options(0, "127.0.0.1", o)?;
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub fn create_listener_with_options(
        port: u16,
        local_host: &str,
        options: SocketOptions,
    ) -> Result<Self, NetError> {
        let ip = if local_host.trim().is_empty() {
            IpAddress::any(crate::IpFamily::V4)
        } else {
            IpAddress::parse(local_host).map_err(|_| NetError::Resolve {
                host: local_host.to_owned(),
                os_error: None,
            })?
        };
        let addr = SocketAddr::new(ip.into(), port);
        let listen = || -> io::Result<Socket> {
            let sock = new_socket(addr, options)?;
            // Windows SO_REUSEADDR lets another process steal the port, so only Unix sets it.
            #[cfg(unix)]
            sock.set_reuse_address(true)?;
            sock.bind(&SockAddr::from(addr))?;
            sock.listen(128)?;
            // Accepts happen after a readiness wait; non-blocking covers a vanished client.
            sock.set_nonblocking(true)?;
            Ok(sock)
        };
        let sock = listen().map_err(|e| socket_error("listen", &e))?;
        Ok(Self::wrap(sock, true, local_host.to_owned(), port))
    }

    /// Blocks until a client connects to this listener and returns the connection.
    ///
    /// # Errors
    /// [`NetError::Closed`] when the listener is (or becomes) closed, [`NetError::Socket`]
    /// when this is not a listener or accepting fails.
    ///
    /// ```no_run
    /// # let l = syrp_net::StreamingSocket::create_listener(9000, "")?;
    /// let conn = l.wait_for_next_connection()?;
    /// println!("client {}:{}", conn.host_name(), conn.port());
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub fn wait_for_next_connection(&self) -> Result<Self, NetError> {
        if !self.listener {
            return Err(not_supported("accept"));
        }
        loop {
            self.wait_until_ready(true, None)?;
            match self.sock.accept() {
                Ok((sock, peer)) => {
                    let accepted = sock.set_nonblocking(false).and_then(|()| no_sigpipe(&sock));
                    accepted.map_err(|e| socket_error("accept", &e))?;
                    let peer = peer.as_socket();
                    let host = peer.map(|p| p.ip().to_string()).unwrap_or_default();
                    let port = peer.map_or(0, |p| p.port());
                    return Ok(Self::wrap(sock, false, host, port));
                }
                Err(e) if retryable(&e) => {}
                Err(e) => return Err(self.error("accept", &e)),
            }
        }
    }

    /// Reads into `buf` and returns how many bytes arrived; 0 means the peer closed the
    /// connection (or `buf` is empty). With `block_until_full` it keeps reading until `buf`
    /// is full or the stream ends; otherwise it returns as soon as some bytes arrive.
    ///
    /// # Errors
    /// [`NetError::Closed`] when the socket is (or becomes) closed, [`NetError::Socket`] when
    /// the OS read fails or this is a listener.
    ///
    /// ```no_run
    /// # let s = syrp_net::StreamingSocket::connect("127.0.0.1", 9000, None)?;
    /// let mut header = [0u8; 8];
    /// let n = s.read(&mut header, true)?;
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub fn read(&self, buf: &mut [u8], block_until_full: bool) -> Result<usize, NetError> {
        if self.listener {
            return Err(not_supported("read"));
        }
        let mut filled = 0;
        while filled < buf.len() {
            self.wait_until_ready(true, None)?;
            match (&self.sock).read(&mut buf[filled..]) {
                Ok(0) => break,
                Ok(n) => filled += n,
                Err(e) if retryable(&e) => continue,
                Err(e) => return Err(self.error("read", &e)),
            }
            if !block_until_full {
                break;
            }
        }
        Ok(filled)
    }

    /// Writes all of `buf` and returns its length.
    ///
    /// # Errors
    /// [`NetError::Closed`] when the socket is closed, [`NetError::Socket`] when the OS write
    /// fails (for example because the peer reset the connection) or this is a listener.
    ///
    /// ```no_run
    /// # let s = syrp_net::StreamingSocket::connect("127.0.0.1", 9000, None)?;
    /// assert_eq!(s.write(b"hello")?, 5);
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub fn write(&self, buf: &[u8]) -> Result<usize, NetError> {
        if self.listener {
            return Err(not_supported("write"));
        }
        let mut sent = 0;
        while sent < buf.len() {
            self.check_open()?;
            match self.sock.send_with_flags(&buf[sent..], SEND_FLAGS) {
                Ok(n) => sent += n,
                Err(e) if retryable(&e) => {}
                Err(e) => return Err(self.error("write", &e)),
            }
        }
        Ok(sent)
    }

    /// Waits until the socket can be read (`for_reading`; for a listener, until a client is
    /// waiting) or written, for at most `timeout` (`None` waits indefinitely).
    ///
    /// # Errors
    /// [`NetError::Closed`] when the socket is (or becomes) closed, [`NetError::Socket`] when
    /// the OS wait fails.
    ///
    /// ```
    /// use std::time::Duration;
    /// use syrp_net::{Readiness, StreamingSocket};
    /// let l = StreamingSocket::create_listener(0, "127.0.0.1")?;
    /// let r = l.wait_until_ready(true, Some(Duration::from_millis(10)))?;
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
    /// let l = syrp_net::StreamingSocket::create_listener(0, "127.0.0.1")?;
    /// l.close();
    /// assert!(!l.is_connected());
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub fn close(&self) {
        if !self.closed.swap(true, Ordering::AcqRel) && !self.listener {
            // Wakes a blocked send; failure (already disconnected) changes nothing.
            let _ = self.sock.shutdown(std::net::Shutdown::Both);
        }
    }

    /// Whether the socket is open: [`close`](Self::close) has not been called. A peer that
    /// disconnects shows up as a read of 0 bytes, not here.
    ///
    /// ```
    /// let l = syrp_net::StreamingSocket::create_listener(0, "127.0.0.1")?;
    /// assert!(l.is_connected());
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub fn is_connected(&self) -> bool {
        !self.closed.load(Ordering::Acquire)
    }

    /// Whether the other end is on this machine: the peer is a loopback address or this
    /// socket's own address. For a listener, whether it listens on a loopback address.
    ///
    /// ```
    /// let l = syrp_net::StreamingSocket::create_listener(0, "127.0.0.1")?;
    /// assert!(l.is_local());
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub fn is_local(&self) -> bool {
        let ip = |a: io::Result<SockAddr>| a.ok().and_then(|a| a.as_socket()).map(|s| s.ip());
        let local = ip(self.sock.local_addr());
        if self.listener {
            return local.is_some_and(|l| l.is_loopback());
        }
        ip(self.sock.peer_addr()).is_some_and(|p| p.is_loopback() || Some(p) == local)
    }

    /// The host this socket was created with: the name given to [`connect`](Self::connect),
    /// the `local_host` of a listener, or the peer's address for an accepted connection.
    ///
    /// ```
    /// let l = syrp_net::StreamingSocket::create_listener(0, "127.0.0.1")?;
    /// assert_eq!(l.host_name(), "127.0.0.1");
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub fn host_name(&self) -> &str {
        &self.host
    }

    /// The port this socket was created with: the remote port of a connection, or the port
    /// requested for a listener (which is 0 when the OS chose one).
    ///
    /// ```
    /// let l = syrp_net::StreamingSocket::create_listener(0, "127.0.0.1")?;
    /// assert_eq!(l.port(), 0);
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The local port the socket is bound to, or `None` once closed.
    ///
    /// ```
    /// let l = syrp_net::StreamingSocket::create_listener(0, "127.0.0.1")?;
    /// assert!(l.bound_port().is_some_and(|p| p != 0));
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub fn bound_port(&self) -> Option<u16> {
        let addr = self.sock.local_addr().ok()?.as_socket()?;
        self.is_connected().then_some(addr.port())
    }

    fn wrap(sock: Socket, listener: bool, host: String, port: u16) -> Self {
        let closed = AtomicBool::new(false);
        Self {
            sock,
            listener,
            host,
            port,
            closed,
        }
    }

    fn check_open(&self) -> Result<(), NetError> {
        if self.is_connected() {
            Ok(())
        } else {
            Err(NetError::Closed)
        }
    }

    /// An OS failure, reported as [`NetError::Closed`] when a close caused it.
    fn error(&self, operation: &'static str, e: &io::Error) -> NetError {
        self.check_open()
            .map_or_else(|closed| closed, |()| socket_error(operation, e))
    }
}

/// Waits for `sock` in [`SLICE`]s, failing with [`NetError::Closed`] once `closed` is set.
pub(crate) fn wait_ready(
    sock: &Socket,
    closed: &AtomicBool,
    for_reading: bool,
    timeout: Option<Duration>,
) -> Result<Readiness, NetError> {
    let open = || match closed.load(Ordering::Acquire) {
        true => Err(NetError::Closed),
        false => Ok(()),
    };
    let deadline = timeout.and_then(|t| Instant::now().checked_add(t));
    loop {
        open()?;
        let left = deadline.map_or(SLICE, |d| {
            d.saturating_duration_since(Instant::now()).min(SLICE)
        });
        let ms = u16::try_from(left.as_millis()).unwrap_or(u16::MAX);
        let ready = poll(sock, for_reading, ms);
        open()?;
        if ready.map_err(|e| socket_error("wait", &e))? {
            return Ok(Readiness::Ready);
        }
        if deadline.is_some_and(|d| Instant::now() >= d) {
            return Ok(Readiness::TimedOut);
        }
    }
}

pub(crate) fn resolve(host: &str, port: u16) -> Result<Vec<SocketAddr>, NetError> {
    let fail = |os_error| NetError::Resolve {
        host: host.to_owned(),
        os_error,
    };
    let addrs: Vec<_> = (host.trim(), port)
        .to_socket_addrs()
        .map_err(|e| fail(e.raw_os_error()))?
        .collect();
    if addrs.is_empty() {
        return Err(fail(None));
    }
    Ok(addrs)
}

/// Applies `options` to a new socket.
pub(crate) fn apply_options(options: SocketOptions, sock: &Socket) -> io::Result<()> {
    if let Some(n) = options.receive_buffer_size() {
        sock.set_recv_buffer_size(n)?;
    }
    if let Some(n) = options.send_buffer_size() {
        sock.set_send_buffer_size(n)?;
    }
    Ok(())
}

fn new_socket(addr: SocketAddr, options: SocketOptions) -> io::Result<Socket> {
    let sock = Socket::new(Domain::for_address(addr), Type::STREAM, Some(Protocol::TCP))?;
    apply_options(options, &sock)?;
    no_sigpipe(&sock)?;
    Ok(sock)
}

/// Stops writes to a disconnected peer from raising `SIGPIPE` (Apple; see [`SEND_FLAGS`]).
fn no_sigpipe(sock: &Socket) -> io::Result<()> {
    #[cfg(target_vendor = "apple")]
    sock.set_nosigpipe(true)?;
    let _ = sock;
    Ok(())
}

pub(crate) fn socket_error(operation: &'static str, e: &io::Error) -> NetError {
    NetError::Socket {
        operation,
        kind: e.kind(),
        os_error: e.raw_os_error(),
    }
}

pub(crate) fn not_supported(operation: &'static str) -> NetError {
    socket_error(operation, &io::ErrorKind::Unsupported.into())
}

pub(crate) fn retryable(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
    )
}

/// Waits up to `ms` milliseconds for `sock` to become readable or writable.
#[cfg(unix)]
fn poll(sock: &Socket, for_reading: bool, ms: u16) -> io::Result<bool> {
    use std::os::fd::AsRawFd;
    poll_fd(sock.as_raw_fd(), for_reading, ms)
}

/// Waits up to `ms` milliseconds for `fd` to become readable or writable.
#[cfg(unix)]
pub(crate) fn poll_fd(fd: std::os::fd::RawFd, for_reading: bool, ms: u16) -> io::Result<bool> {
    let events = if for_reading {
        libc::POLLIN
    } else {
        libc::POLLOUT
    };
    let mut pfd = libc::pollfd {
        fd,
        events,
        revents: 0,
    };
    // SAFETY: `pfd` is one valid, writable pollfd and the count passed is 1.
    let n = unsafe { libc::poll(&mut pfd, 1, libc::c_int::from(ms)) };
    if n < 0 {
        let e = io::Error::last_os_error();
        return if retryable(&e) { Ok(false) } else { Err(e) };
    }
    Ok(n > 0)
}

/// Waits up to `ms` milliseconds for `sock` to become readable or writable.
#[cfg(windows)]
fn poll(sock: &Socket, for_reading: bool, ms: u16) -> io::Result<bool> {
    use std::os::windows::io::AsRawSocket;
    use windows_sys::Win32::Networking::WinSock::{
        POLLRDNORM, POLLWRNORM, SOCKET, SOCKET_ERROR, WSAPOLLFD, WSAPoll,
    };
    let events = if for_reading { POLLRDNORM } else { POLLWRNORM };
    let fd = SOCKET::try_from(sock.as_raw_socket()).map_err(|_| io::ErrorKind::InvalidInput)?;
    let mut pfd = WSAPOLLFD {
        fd,
        events,
        revents: 0,
    };
    // SAFETY: `pfd` is one valid, writable WSAPOLLFD and the count passed is 1.
    let n = unsafe { WSAPoll(&mut pfd, 1, i32::from(ms)) };
    if n == SOCKET_ERROR {
        return Err(io::Error::last_os_error());
    }
    Ok(n > 0)
}

// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The socket and pipe types on targets without an OS network API (such as `wasm32`).
//!
//! They have the same methods as on Unix and Windows, so code that uses them compiles
//! everywhere, but every constructor returns [`NetError::Unsupported`], and so no value of
//! these types can exist: each holds an uninhabited field, and its methods are unreachable.

use std::convert::Infallible;
use std::time::Duration;

use crate::{IpAddress, NetError, Readiness, SocketOptions};

fn unsupported<T>(operation: &'static str) -> Result<T, NetError> {
    Err(NetError::Unsupported { operation })
}

/// A TCP socket. On this target every constructor returns [`NetError::Unsupported`].
///
/// ```
/// let r = syrp_net::StreamingSocket::create_listener(0, "127.0.0.1");
/// # #[cfg(not(any(unix, windows)))]
/// assert!(matches!(r, Err(syrp_net::NetError::Unsupported { .. })));
/// ```
#[derive(Debug)]
pub struct StreamingSocket {
    never: Infallible,
}

impl StreamingSocket {
    /// Connects to `host` on `port`.
    ///
    /// # Errors
    /// Always [`NetError::Unsupported`] on this target.
    pub fn connect(host: &str, port: u16, timeout: Option<Duration>) -> Result<Self, NetError> {
        let _ = (host, port, timeout);
        unsupported("StreamingSocket::connect")
    }

    /// Connects to `host` on `port` with `options`.
    ///
    /// # Errors
    /// Always [`NetError::Unsupported`] on this target.
    pub fn connect_with_options(
        host: &str,
        port: u16,
        timeout: Option<Duration>,
        options: SocketOptions,
    ) -> Result<Self, NetError> {
        let _ = (host, port, timeout, options);
        unsupported("StreamingSocket::connect")
    }

    /// Listens on `port` at `local_host`.
    ///
    /// # Errors
    /// Always [`NetError::Unsupported`] on this target.
    pub fn create_listener(port: u16, local_host: &str) -> Result<Self, NetError> {
        let _ = (port, local_host);
        unsupported("StreamingSocket::create_listener")
    }

    /// Listens on `port` at `local_host` with `options`.
    ///
    /// # Errors
    /// Always [`NetError::Unsupported`] on this target.
    pub fn create_listener_with_options(
        port: u16,
        local_host: &str,
        options: SocketOptions,
    ) -> Result<Self, NetError> {
        let _ = (port, local_host, options);
        unsupported("StreamingSocket::create_listener")
    }

    /// Unreachable: no value exists on this target.
    ///
    /// # Errors
    /// None; there is no value to call it on.
    pub fn wait_for_next_connection(&self) -> Result<Self, NetError> {
        match self.never {}
    }

    /// Unreachable: no value exists on this target.
    ///
    /// # Errors
    /// None; there is no value to call it on.
    pub fn read(&self, buf: &mut [u8], block_until_full: bool) -> Result<usize, NetError> {
        let _ = (buf, block_until_full);
        match self.never {}
    }

    /// Unreachable: no value exists on this target.
    ///
    /// # Errors
    /// None; there is no value to call it on.
    pub fn write(&self, buf: &[u8]) -> Result<usize, NetError> {
        let _ = buf;
        match self.never {}
    }

    /// Unreachable: no value exists on this target.
    ///
    /// # Errors
    /// None; there is no value to call it on.
    pub fn wait_until_ready(
        &self,
        for_reading: bool,
        timeout: Option<Duration>,
    ) -> Result<Readiness, NetError> {
        let _ = (for_reading, timeout);
        match self.never {}
    }

    /// Unreachable: no value exists on this target.
    pub fn close(&self) {
        match self.never {}
    }

    /// Unreachable: no value exists on this target.
    pub fn is_connected(&self) -> bool {
        match self.never {}
    }

    /// Unreachable: no value exists on this target.
    pub fn is_local(&self) -> bool {
        match self.never {}
    }

    /// Unreachable: no value exists on this target.
    pub fn host_name(&self) -> &str {
        match self.never {}
    }

    /// Unreachable: no value exists on this target.
    pub fn port(&self) -> u16 {
        match self.never {}
    }

    /// Unreachable: no value exists on this target.
    pub fn bound_port(&self) -> Option<u16> {
        match self.never {}
    }
}

/// A UDP socket. On this target every constructor returns [`NetError::Unsupported`].
///
/// ```
/// let r = syrp_net::DatagramSocket::new(false);
/// # #[cfg(not(any(unix, windows)))]
/// assert!(matches!(r, Err(syrp_net::NetError::Unsupported { .. })));
/// ```
#[derive(Debug)]
pub struct DatagramSocket {
    never: Infallible,
}

impl DatagramSocket {
    /// Creates a UDP socket.
    ///
    /// # Errors
    /// Always [`NetError::Unsupported`] on this target.
    pub fn new(enable_broadcasting: bool) -> Result<Self, NetError> {
        let _ = enable_broadcasting;
        unsupported("DatagramSocket::new")
    }

    /// Creates a UDP socket with `options`.
    ///
    /// # Errors
    /// Always [`NetError::Unsupported`] on this target.
    pub fn new_with_options(
        enable_broadcasting: bool,
        options: SocketOptions,
    ) -> Result<Self, NetError> {
        let _ = (enable_broadcasting, options);
        unsupported("DatagramSocket::new")
    }

    /// Unreachable: no value exists on this target.
    ///
    /// # Errors
    /// None; there is no value to call it on.
    pub fn bind_to_port(&self, port: u16, local_addr: &str) -> Result<(), NetError> {
        let _ = (port, local_addr);
        match self.never {}
    }

    /// Unreachable: no value exists on this target.
    pub fn bound_port(&self) -> Option<u16> {
        match self.never {}
    }

    /// Unreachable: no value exists on this target.
    ///
    /// # Errors
    /// None; there is no value to call it on.
    pub fn read(&self, buf: &mut [u8], block: bool) -> Result<(usize, IpAddress, u16), NetError> {
        let _ = (buf, block);
        match self.never {}
    }

    /// Unreachable: no value exists on this target.
    ///
    /// # Errors
    /// None; there is no value to call it on.
    pub fn write(&self, host: &str, port: u16, buf: &[u8]) -> Result<usize, NetError> {
        let _ = (host, port, buf);
        match self.never {}
    }

    /// Unreachable: no value exists on this target.
    ///
    /// # Errors
    /// None; there is no value to call it on.
    pub fn join_multicast(&self, group: IpAddress) -> Result<(), NetError> {
        let _ = group;
        match self.never {}
    }

    /// Unreachable: no value exists on this target.
    ///
    /// # Errors
    /// None; there is no value to call it on.
    pub fn leave_multicast(&self, group: IpAddress) -> Result<(), NetError> {
        let _ = group;
        match self.never {}
    }

    /// Unreachable: no value exists on this target.
    ///
    /// # Errors
    /// None; there is no value to call it on.
    pub fn set_multicast_loopback_enabled(&self, enabled: bool) -> Result<(), NetError> {
        let _ = enabled;
        match self.never {}
    }

    /// Unreachable: no value exists on this target.
    ///
    /// # Errors
    /// None; there is no value to call it on.
    pub fn set_enable_broadcasting(&self, enabled: bool) -> Result<(), NetError> {
        let _ = enabled;
        match self.never {}
    }

    /// Unreachable: no value exists on this target.
    ///
    /// # Errors
    /// None; there is no value to call it on.
    pub fn wait_until_ready(
        &self,
        for_reading: bool,
        timeout: Option<Duration>,
    ) -> Result<Readiness, NetError> {
        let _ = (for_reading, timeout);
        match self.never {}
    }

    /// Unreachable: no value exists on this target.
    pub fn close(&self) {
        match self.never {}
    }
}

/// A named pipe. On this target every constructor returns [`NetError::Unsupported`].
///
/// ```
/// let r = syrp_net::NamedPipe::open_existing("my-app");
/// # #[cfg(not(any(unix, windows)))]
/// assert!(matches!(r, Err(syrp_net::NetError::Unsupported { .. })));
/// ```
#[derive(Debug)]
pub struct NamedPipe {
    never: Infallible,
}

impl NamedPipe {
    /// Creates the pipe `name`.
    ///
    /// # Errors
    /// Always [`NetError::Unsupported`] on this target.
    pub fn create_new_pipe(name: &str, must_not_exist: bool) -> Result<Self, NetError> {
        let _ = (name, must_not_exist);
        unsupported("NamedPipe::create_new_pipe")
    }

    /// Opens the pipe `name`.
    ///
    /// # Errors
    /// Always [`NetError::Unsupported`] on this target.
    pub fn open_existing(name: &str) -> Result<Self, NetError> {
        let _ = name;
        unsupported("NamedPipe::open_existing")
    }

    /// Unreachable: no value exists on this target.
    ///
    /// # Errors
    /// None; there is no value to call it on.
    pub fn read(&self, buf: &mut [u8], timeout: Option<Duration>) -> Result<usize, NetError> {
        let _ = (buf, timeout);
        match self.never {}
    }

    /// Unreachable: no value exists on this target.
    ///
    /// # Errors
    /// None; there is no value to call it on.
    pub fn write(&self, buf: &[u8], timeout: Option<Duration>) -> Result<usize, NetError> {
        let _ = (buf, timeout);
        match self.never {}
    }

    /// Unreachable: no value exists on this target.
    pub fn close(&self) {
        match self.never {}
    }

    /// Unreachable: no value exists on this target.
    pub fn is_open(&self) -> bool {
        match self.never {}
    }

    /// Unreachable: no value exists on this target.
    pub fn name(&self) -> &str {
        match self.never {}
    }
}

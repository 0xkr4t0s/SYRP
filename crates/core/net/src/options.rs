// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`SocketOptions`] and [`Readiness`], shared by the socket types on every target.

/// Options applied when a socket is created (JUCE's `SocketOptions`). Unset sizes keep the
/// OS default; the OS may round or clamp a size it is given.
///
/// ```
/// use syrp_net::SocketOptions;
/// let o = SocketOptions::new().with_receive_buffer_size(64 * 1024);
/// assert_eq!(o.receive_buffer_size(), Some(64 * 1024));
/// assert_eq!(o.send_buffer_size(), None);
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SocketOptions {
    receive_buffer_size: Option<usize>,
    send_buffer_size: Option<usize>,
}

impl SocketOptions {
    /// Options that keep every OS default.
    ///
    /// ```
    /// use syrp_net::SocketOptions;
    /// assert_eq!(SocketOptions::new(), SocketOptions::default());
    /// ```
    pub const fn new() -> Self {
        Self {
            receive_buffer_size: None,
            send_buffer_size: None,
        }
    }

    /// These options with `SO_RCVBUF` set to `bytes`.
    ///
    /// ```
    /// use syrp_net::SocketOptions;
    /// assert_eq!(SocketOptions::new().with_receive_buffer_size(4096).receive_buffer_size(), Some(4096));
    /// ```
    #[must_use]
    pub const fn with_receive_buffer_size(mut self, bytes: usize) -> Self {
        self.receive_buffer_size = Some(bytes);
        self
    }

    /// These options with `SO_SNDBUF` set to `bytes`.
    ///
    /// ```
    /// use syrp_net::SocketOptions;
    /// assert_eq!(SocketOptions::new().with_send_buffer_size(4096).send_buffer_size(), Some(4096));
    /// ```
    #[must_use]
    pub const fn with_send_buffer_size(mut self, bytes: usize) -> Self {
        self.send_buffer_size = Some(bytes);
        self
    }

    /// The receive buffer size to request, if any.
    ///
    /// ```
    /// assert_eq!(syrp_net::SocketOptions::new().receive_buffer_size(), None);
    /// ```
    pub const fn receive_buffer_size(&self) -> Option<usize> {
        self.receive_buffer_size
    }

    /// The send buffer size to request, if any.
    ///
    /// ```
    /// assert_eq!(syrp_net::SocketOptions::new().send_buffer_size(), None);
    /// ```
    pub const fn send_buffer_size(&self) -> Option<usize> {
        self.send_buffer_size
    }
}

/// The outcome of [`StreamingSocket::wait_until_ready`](crate::StreamingSocket::wait_until_ready); failures are `Err`.
///
/// ```
/// use syrp_net::Readiness;
/// assert_ne!(Readiness::Ready, Readiness::TimedOut);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Readiness {
    /// The socket can be read or written (or has hit end of stream or an error, which the
    /// next read or write reports).
    Ready,
    /// The timeout passed first.
    TimedOut,
}

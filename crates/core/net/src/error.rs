// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The crate's error type and its `SYRP-NET-*` codes (catalogued in `errors/net.toml`).

use std::borrow::Cow;
use std::fmt;

use syrp_error::{Diagnostic, ErrorCode};

/// Everything that can fail in this crate. Each variant has a stable code, and variants that
/// come from the operating system carry its error code.
///
/// ```
/// use syrp_net::{Diagnostic, IpAddress, NetError};
/// let err: NetError = IpAddress::parse("300.1.2.3").unwrap_err();
/// assert_eq!(err.code().as_str(), "SYRP-NET-0001");
/// assert!(err.to_string().contains("300.1.2.3"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NetError {
    /// SYRP-NET-0001: text given to [`IpAddress::parse`](crate::IpAddress::parse) is not an
    /// IPv4 or IPv6 address.
    #[non_exhaustive]
    InvalidIpAddress {
        /// The text that was rejected.
        input: String,
    },
    /// SYRP-NET-0002: listing the network interfaces failed.
    #[non_exhaustive]
    InterfaceQuery {
        /// The OS error code, when the OS reported one.
        os_error: Option<i32>,
    },
    /// SYRP-NET-0003: the operation is not available on this target.
    #[non_exhaustive]
    Unsupported {
        /// The operation that was attempted.
        operation: &'static str,
    },
    /// SYRP-NET-0004: text given to [`MacAddress::parse`](crate::MacAddress::parse) is not six
    /// hex bytes.
    #[non_exhaustive]
    InvalidMacAddress {
        /// The text that was rejected.
        input: String,
        /// What was wrong with it.
        reason: &'static str,
    },
    /// SYRP-NET-0005: a host name or address did not resolve to any socket address.
    #[non_exhaustive]
    Resolve {
        /// The host that was looked up.
        host: String,
        /// The OS error code, when the OS reported one.
        os_error: Option<i32>,
    },
    /// SYRP-NET-0006: a socket operation failed.
    #[non_exhaustive]
    Socket {
        /// The operation, such as `"connect"`, `"listen"`, `"accept"`, `"read"` or `"write"`.
        operation: &'static str,
        /// The kind of I/O failure.
        kind: std::io::ErrorKind,
        /// The OS error code, when the OS reported one.
        os_error: Option<i32>,
    },
    /// SYRP-NET-0007: the socket was closed, before or during the call.
    Closed,
}

impl NetError {
    /// The operating-system error code carried by this error, if any.
    ///
    /// ```
    /// use syrp_net::{IpAddress, NetError};
    /// assert_eq!(IpAddress::parse("x").unwrap_err().os_error(), None);
    /// ```
    pub fn os_error(&self) -> Option<i32> {
        match self {
            Self::InterfaceQuery { os_error }
            | Self::Resolve { os_error, .. }
            | Self::Socket { os_error, .. } => *os_error,
            Self::InvalidIpAddress { .. }
            | Self::InvalidMacAddress { .. }
            | Self::Unsupported { .. }
            | Self::Closed => None,
        }
    }
}

impl Diagnostic for NetError {
    fn code(&self) -> ErrorCode {
        ErrorCode::new(match self {
            Self::InvalidIpAddress { .. } => "SYRP-NET-0001",
            Self::InterfaceQuery { .. } => "SYRP-NET-0002",
            Self::Unsupported { .. } => "SYRP-NET-0003",
            Self::InvalidMacAddress { .. } => "SYRP-NET-0004",
            Self::Resolve { .. } => "SYRP-NET-0005",
            Self::Socket { .. } => "SYRP-NET-0006",
            Self::Closed => "SYRP-NET-0007",
        })
    }

    fn hint(&self) -> Option<Cow<'static, str>> {
        Some(match self {
            Self::InvalidIpAddress { .. } => {
                "use 192.168.0.1 or 2001:db8::1, without port, brackets or zone".into()
            }
            Self::InterfaceQuery { .. } => "retry; a sandbox may deny interface listing".into(),
            Self::Unsupported { .. } => "build for Linux, macOS or Windows".into(),
            Self::InvalidMacAddress { .. } => {
                "use 00-1a-2b-3c-4d-5e, 00:1a:2b:3c:4d:5e or 001a2b3c4d5e".into()
            }
            Self::Resolve { .. } => "check the host name, or pass an IP address".into(),
            Self::Socket { .. } => "check the address, port and peer; the OS code says more".into(),
            Self::Closed => "create a new socket; a closed one cannot be reopened".into(),
        })
    }
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let code = self.code();
        match self {
            Self::InvalidIpAddress { input } => {
                write!(f, "{code}: {input:?} is not an IPv4 or IPv6 address")
            }
            Self::InterfaceQuery { os_error: Some(e) } => {
                write!(
                    f,
                    "{code}: listing network interfaces failed (OS error {e})"
                )
            }
            Self::InterfaceQuery { os_error: None } => {
                write!(f, "{code}: listing network interfaces failed")
            }
            Self::Unsupported { operation } => {
                write!(f, "{code}: {operation} is not supported on this target")
            }
            Self::InvalidMacAddress { input, reason } => {
                write!(f, "{code}: {input:?} is not a MAC address: {reason}")
            }
            Self::Resolve { host, .. } => write!(f, "{code}: could not resolve host {host:?}"),
            Self::Socket {
                operation, kind, ..
            } => {
                write!(f, "{code}: socket {operation} failed ({kind})")?;
                self.os_error()
                    .map_or(Ok(()), |e| write!(f, " (OS error {e})"))
            }
            Self::Closed => write!(f, "{code}: the socket is closed"),
        }
    }
}

impl std::error::Error for NetError {}

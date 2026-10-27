// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The Syrp networking transport layer (master plan §5.2, L2 Core).
//!
//! - [`IpAddress`]: one value type for IPv4 and IPv6, with RFC 5952 text, the IPv4-mapped
//!   conversions and interface enumeration ([`IpAddress::all_addresses`]).
//! - [`MacAddress`]: a 6-byte hardware address, with its text forms and
//!   [`MacAddress::find_all_addresses`].
//! - [`StreamingSocket`]: TCP connections and listeners, with [`SocketOptions`] on creation.
//! - [`DatagramSocket`]: UDP over IPv4, with broadcast and multicast.
//! - [`NamedPipe`]: a named, two-way byte pipe between processes on this machine.
//!
//! URL and HTTP come from WP-1.1-09. The socket and pipe types exist on Unix and Windows only.
//! Errors are [`NetError`], with codes `SYRP-NET-*` in `errors/net.toml`. On targets without an
//! OS network API (such as `wasm32`) the value types work and every OS query returns
//! [`NetError::Unsupported`].
//!
//! ```
//! use syrp_net::{IpAddress, IpFamily};
//! let a = IpAddress::parse("2001:0db8:0000:0000:0000:0000:0000:0001")?;
//! assert_eq!(a.to_string(), "2001:db8::1");
//! assert!(IpAddress::local(IpFamily::V4).is_loopback());
//! # Ok::<(), syrp_net::NetError>(())
//! ```

#[cfg(any(unix, windows))]
mod datagram;
mod error;
mod ifaces;
mod ip;
mod mac;
#[cfg(any(unix, windows))]
mod pipe;
#[cfg(unix)]
mod pipe_unix;
#[cfg(windows)]
mod pipe_windows;
#[cfg(any(unix, windows))]
mod socket;

#[cfg(any(unix, windows))]
pub use datagram::DatagramSocket;
pub use error::NetError;
pub use ip::{IpAddress, IpFamily};
pub use mac::MacAddress;
#[cfg(any(unix, windows))]
pub use pipe::NamedPipe;
#[cfg(any(unix, windows))]
pub use socket::{Readiness, SocketOptions, StreamingSocket};
pub use syrp_error::Diagnostic;

// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The Syrp networking transport layer (master plan §5.2, L2 Core).
//!
//! - [`IpAddress`]: one value type for IPv4 and IPv6, with RFC 5952 text, the IPv4-mapped
//!   conversions and interface enumeration ([`IpAddress::all_addresses`]).
//!
//! `MacAddress`, sockets and named pipes follow in later increments of WP-1.1-10; URL and
//! HTTP in WP-1.1-09.
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

mod error;
mod ifaces;
mod ip;

pub use error::NetError;
pub use ip::{IpAddress, IpFamily};
pub use syrp_error::Diagnostic;

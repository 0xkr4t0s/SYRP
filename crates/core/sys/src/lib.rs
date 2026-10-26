// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The Syrp system layer (master plan §5.2, L2 Core): wall-clock and calendar time, durations
//! the high-resolution monotonic counters, UUIDs and system facts.
//!
//! - [`RelativeTime`]: a signed duration in seconds, with a readable description.
//! - [`Time`]: milliseconds since the Unix epoch, calendar fields in UTC or local time
//!   ([`DateTime`]), arithmetic with [`RelativeTime`], ISO 8601 and strftime-style text.
//! - Counters on [`Time`]: [`Time::millisecond_counter`], [`Time::millisecond_counter_hi_res`],
//!   [`Time::high_resolution_ticks`] and [`Time::high_resolution_ticks_per_second`] are
//!   RT-safe (lock-free, allocation-free, non-blocking).
//! - [`Uuid`]: random version 4 UUIDs, parsing and formatting.
//! - [`SystemStats`]: CPU count and features, memory and page size, environment variables.
//!
//! Errors are [`SysError`], with codes `SYRP-SYS-*` in `errors/sys.toml`.
//!
//! ```
//! use syrp_sys::{RelativeTime, Time};
//! let start = Time::high_resolution_ticks();
//! let t = Time::from_components(2024, 2, 1, 9, 30, 0, 0, false);
//! assert_eq!((t + RelativeTime::minutes(45.0)).utc().hours(), 10);
//! assert!(Time::high_resolution_ticks() >= start);
//! ```

mod error;
mod format;
mod os;
mod relative;
mod stats;
mod time;
mod uuid;

pub use error::SysError;
pub use relative::RelativeTime;
pub use stats::{CpuFeature, SystemStats};
pub use syrp_error::Diagnostic;
pub use time::{DateTime, Time};
pub use uuid::Uuid;

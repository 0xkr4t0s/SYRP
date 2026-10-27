// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Platform backends. Each provides a `Waker` (any thread) and a `Poller` (the loop's thread)
//! whose `wait` blocks until a wake, a ready source or the timeout, and runs ready sources.

#[cfg(target_os = "linux")]
pub(crate) mod epoll;
#[cfg(target_os = "linux")]
pub(crate) use epoll::{Poller, Waker};

#[cfg(target_os = "macos")]
mod cfrunloop;
#[cfg(target_os = "macos")]
pub(crate) use cfrunloop::{Poller, Waker};

#[cfg(windows)]
mod win32;
#[cfg(windows)]
pub(crate) use win32::{Poller, Waker};

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod unsupported;
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
pub(crate) use unsupported::{Poller, Waker};

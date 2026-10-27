// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Platform backends. Each provides a `Waker` (any thread) and a `Poller` (the loop's thread)
//! whose `wait` blocks until a wake, a ready source or the timeout, and runs ready sources.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub(crate) use linux::{Poller, Waker};

#[cfg(target_os = "macos")]
mod kqueue;
#[cfg(target_os = "macos")]
pub(crate) use kqueue::{Poller, Waker};

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod portable;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub(crate) use portable::{Poller, Waker};

/// Takes ownership of a descriptor a libc call returned, or reports the failed call.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn owned(
    fd: std::os::fd::RawFd,
    call: &'static str,
) -> Result<std::os::fd::OwnedFd, crate::EventsError> {
    use std::os::fd::FromRawFd;
    if fd < 0 {
        return Err(crate::EventsError::os(
            call,
            &std::io::Error::last_os_error(),
        ));
    }
    // SAFETY: `fd` was just returned by a successful libc call, so it is open and owned by
    // nobody else.
    Ok(unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) })
}

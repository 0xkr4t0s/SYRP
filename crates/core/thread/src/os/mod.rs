// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Platform scheduling calls. Each backend provides `set_priority`, `set_affinity` and
//! `enter_realtime`, which all act on the calling thread. Errors are raw OS codes.

#[cfg(any(target_os = "linux", target_os = "android"))]
mod linux;
#[cfg(any(target_os = "linux", target_os = "android"))]
pub(crate) use linux::*;

#[cfg(target_vendor = "apple")]
mod apple;
#[cfg(target_vendor = "apple")]
pub(crate) use apple::*;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub(crate) use windows::*;

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_vendor = "apple",
    windows
)))]
mod other;
#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_vendor = "apple",
    windows
)))]
pub(crate) use other::*;

/// The last OS error code of the calling thread.
#[allow(dead_code)] // unused by the fallback backend
pub(crate) fn last_error() -> Option<i32> {
    std::io::Error::last_os_error().raw_os_error()
}

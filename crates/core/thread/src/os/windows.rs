// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Windows: `SetThreadPriority`, MMCSS "Pro Audio" and `SetThreadAffinityMask`.

use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::System::Threading as w;

use super::last_error;
use crate::{Priority, RealtimeOptions, ThreadError};

/// The MMCSS task handle; dropping it on the registered thread reverts the registration.
pub(crate) struct RealtimeToken(HANDLE);

impl Drop for RealtimeToken {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful AvSetMmThreadCharacteristicsW on this
        // thread (the token is not Send) and is reverted exactly once, here.
        unsafe { w::AvRevertMmThreadCharacteristics(self.0) };
    }
}

pub(crate) fn set_priority(priority: Priority) -> Result<(), Option<i32>> {
    let level = match priority {
        Priority::Lowest => w::THREAD_PRIORITY_LOWEST,
        Priority::Low => w::THREAD_PRIORITY_BELOW_NORMAL,
        Priority::Normal => w::THREAD_PRIORITY_NORMAL,
        Priority::High => w::THREAD_PRIORITY_ABOVE_NORMAL,
        Priority::Highest => w::THREAD_PRIORITY_HIGHEST,
    };
    // SAFETY: GetCurrentThread returns a pseudo-handle that is always valid on the calling
    // thread, and `level` is one of the documented priority constants.
    let ok = unsafe { w::SetThreadPriority(w::GetCurrentThread(), level) };
    if ok != 0 { Ok(()) } else { Err(last_error()) }
}

pub(crate) fn enter_realtime(opts: &RealtimeOptions) -> Result<RealtimeToken, Option<i32>> {
    let task: [u16; 10] = {
        let mut buf = [0u16; 10];
        for (dst, src) in buf.iter_mut().zip("Pro Audio".encode_utf16()) {
            *dst = src;
        }
        buf
    };
    let mut index = 0u32;
    // SAFETY: `task` is a NUL-terminated UTF-16 string and `index` a valid, aligned, writable
    // u32 (0 asks MMCSS to pick a task index); both outlive the call.
    let handle = unsafe { w::AvSetMmThreadCharacteristicsW(task.as_ptr(), &mut index) };
    if handle.is_null() {
        return Err(last_error());
    }
    let token = RealtimeToken(handle);
    let level = match opts.priority() {
        0..=2 => w::AVRT_PRIORITY_LOW,
        3..=6 => w::AVRT_PRIORITY_NORMAL,
        7..=8 => w::AVRT_PRIORITY_HIGH,
        _ => w::AVRT_PRIORITY_CRITICAL,
    };
    // SAFETY: `token.0` is the live MMCSS handle registered just above on this thread.
    if unsafe { w::AvSetMmThreadPriority(token.0, level) } == 0 {
        return Err(last_error());
    }
    Ok(token)
}

pub(crate) fn set_affinity(mask: u64) -> Result<(), ThreadError> {
    let mask =
        usize::try_from(mask).map_err(|_| ThreadError::AffinityRefused { os_error: None })?;
    // SAFETY: GetCurrentThread returns a pseudo-handle that is always valid on the calling
    // thread; the mask is a plain integer.
    let previous = unsafe { w::SetThreadAffinityMask(w::GetCurrentThread(), mask) };
    if previous != 0 {
        Ok(())
    } else {
        Err(ThreadError::AffinityRefused {
            os_error: last_error(),
        })
    }
}

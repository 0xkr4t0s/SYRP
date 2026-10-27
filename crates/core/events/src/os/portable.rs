// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The interim backend for Windows until its Win32 backend (B6): a flag and a condition variable, with no OS event sources. On Windows a loop holds a 1 ms
//! system timer resolution (`timeBeginPeriod`), because condition-variable timeouts otherwise
//! follow the 15.6 ms system tick. Other targets get `UnsupportedTarget`.

use std::sync::{Condvar, Mutex, PoisonError};
use std::time::Duration;

use syrp_loop_api::MessageThread;

use crate::EventsError;

pub(crate) struct Waker {
    woken: Mutex<bool>,
    cv: Condvar,
}

impl Waker {
    pub(crate) fn new() -> Result<Self, EventsError> {
        if cfg!(not(target_os = "windows")) {
            return Err(EventsError::UnsupportedTarget);
        }
        Ok(Waker {
            woken: Mutex::new(false),
            cv: Condvar::new(),
        })
    }

    pub(crate) fn wake(&self) {
        *self.woken.lock().unwrap_or_else(PoisonError::into_inner) = true;
        self.cv.notify_one();
    }
}

pub(crate) struct Poller;

impl Poller {
    pub(crate) fn new(_waker: &Waker) -> Result<Self, EventsError> {
        // SAFETY: takes a plain integer; `Drop` ends the request. A refusal only leaves the
        // default resolution.
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::Media::timeBeginPeriod(1)
        };
        Ok(Poller)
    }

    /// Blocks until woken or `timeout` passes; runs no sources, so returns 0.
    pub(crate) fn wait(
        &self,
        waker: &Waker,
        timeout: Option<Duration>,
        _mt: &MessageThread<'_>,
    ) -> usize {
        let woken = waker.woken.lock().unwrap_or_else(PoisonError::into_inner);
        let mut woken = match timeout {
            None => (waker.cv.wait_while(woken, |w| !*w)).unwrap_or_else(PoisonError::into_inner),
            Some(t) => {
                (waker.cv.wait_timeout_while(woken, t, |w| !*w))
                    .unwrap_or_else(PoisonError::into_inner)
                    .0
            }
        };
        *woken = false;
        0
    }
}

impl Drop for Poller {
    fn drop(&mut self) {
        // SAFETY: ends the timeBeginPeriod(1) request made in `new`.
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::Media::timeEndPeriod(1)
        };
    }
}

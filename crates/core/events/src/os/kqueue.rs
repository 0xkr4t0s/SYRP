// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The interim macOS backend until the `CFRunLoop` one (B5): a `kqueue` with an `EVFILT_USER`
//! event that posted tasks and quits trigger, and an `EVFILT_TIMER` for the wait timeout.
//!
//! The timer is marked `NOTE_CRITICAL`. Darwin coalesces ordinary timed waits (condition
//! variables, `nanosleep`, the `kevent` timeout), and on CI that made 10 ms waits overshoot by
//! more than a whole period, so the loop skipped periods. A critical timer is not coalesced.

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::ptr;
use std::time::Duration;

use syrp_loop_api::MessageThread;

use crate::EventsError;

/// The `ident` of the user event that wakes the loop.
const WAKE: usize = 0;
/// The `ident` of the one-shot timer that ends a wait.
const TIMEOUT: usize = 1;

fn event(ident: usize, filter: i16, flags: u16, fflags: u32, data: isize) -> libc::kevent {
    libc::kevent {
        ident,
        filter,
        flags,
        fflags,
        data,
        udata: ptr::null_mut(),
    }
}

/// Wakes the loop from any thread by triggering a user event on its kqueue.
pub(crate) struct Waker {
    kq: OwnedFd,
}

impl Waker {
    pub(crate) fn new() -> Result<Self, EventsError> {
        // SAFETY: kqueue takes no arguments and touches no memory of ours.
        let fd = unsafe { libc::kqueue() };
        if fd < 0 {
            return Err(EventsError::os("kqueue", &io::Error::last_os_error()));
        }
        // SAFETY: `fd` was just returned by a successful kqueue call, so it is open and owned
        // by nobody else.
        let kq = unsafe { OwnedFd::from_raw_fd(fd) };
        let waker = Waker { kq };
        let add = event(WAKE, libc::EVFILT_USER, libc::EV_ADD | libc::EV_CLEAR, 0, 0);
        waker
            .kevent(&[add], &mut [], None)
            .map_err(|e| EventsError::os("kevent", &e))?;
        Ok(waker)
    }

    pub(crate) fn wake(&self) {
        let trigger = event(WAKE, libc::EVFILT_USER, 0, libc::NOTE_TRIGGER, 0);
        // A failure leaves the loop to its timeout; there is nobody to report it to.
        let _ = self.kevent(&[trigger], &mut [], None);
    }

    /// Applies `changes`, then waits for up to `out.len()` events or `timeout`.
    fn kevent(
        &self,
        changes: &[libc::kevent],
        out: &mut [libc::kevent],
        timeout: Option<&libc::timespec>,
    ) -> io::Result<usize> {
        // Both lengths are at most 2.
        let (n_changes, n_out) = (changes.len() as i32, out.len() as i32);
        let timeout = timeout.map_or(ptr::null(), ptr::from_ref);
        // SAFETY: the kqueue fd is live, both slices are valid for their lengths for the call,
        // and `timeout` is null or points at a timespec that outlives the call.
        let n = unsafe {
            let (c, o) = (changes.as_ptr(), out.as_mut_ptr());
            libc::kevent(self.kq.as_raw_fd(), c, n_changes, o, n_out, timeout)
        };
        usize::try_from(n).map_err(|_| io::Error::last_os_error())
    }
}

/// The kqueue lives in the waker; the poller only waits on it.
pub(crate) struct Poller;

impl Poller {
    pub(crate) fn new(_waker: &Waker) -> Result<Self, EventsError> {
        Ok(Poller)
    }

    /// Waits for a wake or the timeout (`None`: no limit). Runs no callbacks, so returns 0.
    pub(crate) fn wait(
        &self,
        waker: &Waker,
        timeout: Option<Duration>,
        _mt: &MessageThread<'_>,
    ) -> usize {
        let mut out = [event(0, 0, 0, 0, 0); 2];
        let zero = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // An earlier one-shot timer that did not fire may still be armed; EV_ADD re-arms it,
        // and a stale firing after a `None` wait is only a spurious wake, which the loop
        // tolerates. Failures (such as EINTR) also count as "nothing ready".
        let _ = match timeout {
            Some(t) if t.is_zero() => waker.kevent(&[], &mut out, Some(&zero)),
            Some(t) => {
                let ns = isize::try_from(t.as_nanos()).unwrap_or(isize::MAX);
                let flags = libc::EV_ADD | libc::EV_ONESHOT;
                let fflags = libc::NOTE_NSECONDS | libc::NOTE_CRITICAL;
                let timer = event(TIMEOUT, libc::EVFILT_TIMER, flags, fflags, ns);
                waker.kevent(&[timer], &mut out, None)
            }
            None => waker.kevent(&[], &mut out, None),
        };
        0
    }
}

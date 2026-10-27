// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The interim macOS backend until the `CFRunLoop` one (B5): a `kqueue` with a user event for
//! wakes and a one-shot `NOTE_CRITICAL` timer for the wait timeout. Darwin coalesces ordinary
//! timed waits (condvars, `nanosleep`, the `kevent` timeout), which on CI overshot 10 ms waits
//! by more than a period; critical timers are not coalesced.

use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::{io, ptr, time::Duration};

use syrp_loop_api::MessageThread;

use crate::EventsError;

fn ev(ident: usize, filter: i16, flags: u16, fflags: u32, data: isize) -> libc::kevent {
    let udata = ptr::null_mut();
    libc::kevent {
        ident,
        filter,
        flags,
        fflags,
        data,
        udata,
    }
}

/// Wakes the loop from any thread by triggering user event 0 on its kqueue.
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
        // SAFETY: `fd` was just returned by a successful kqueue call, so nobody else owns it.
        let kq = unsafe { OwnedFd::from_raw_fd(fd) };
        let waker = Waker { kq };
        let add = ev(0, libc::EVFILT_USER, libc::EV_ADD | libc::EV_CLEAR, 0, 0);
        let rc = waker.kevent(&[add], 0, None);
        rc.map_err(|e| EventsError::os("kevent", &e)).map(|_| waker)
    }

    pub(crate) fn wake(&self) {
        // A failure leaves the loop to its timeout; there is nobody to report it to.
        let trigger = ev(0, libc::EVFILT_USER, 0, libc::NOTE_TRIGGER, 0);
        let _ = self.kevent(&[trigger], 0, None);
    }

    /// Applies `changes`, then waits for up to `max` (0..=2) events or `timeout`.
    fn kevent(
        &self,
        changes: &[libc::kevent],
        max: i32,
        timeout: Option<&libc::timespec>,
    ) -> io::Result<()> {
        let mut out = [ev(0, 0, 0, 0, 0); 2];
        let timeout = timeout.map_or(ptr::null(), ptr::from_ref);
        let n_changes = changes.len() as i32; // at most 1
        // SAFETY: the kqueue is live, `changes` and `out` are valid for the lengths passed
        // (`max` <= 2), and `timeout` is null or points at a timespec that outlives the call.
        let n = unsafe {
            let (c, o) = (changes.as_ptr(), out.as_mut_ptr());
            libc::kevent(self.kq.as_raw_fd(), c, n_changes, o, max, timeout)
        };
        if n < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

/// The kqueue lives in the waker; the poller only waits on it.
pub(crate) struct Poller;

impl Poller {
    pub(crate) fn new(_waker: &Waker) -> Result<Self, EventsError> {
        Ok(Poller)
    }

    /// Waits for a wake or the timeout (`None`: no limit). Runs no callbacks, so returns 0.
    /// An unfired one-shot timer from an earlier wait is re-armed by `EV_ADD`, or at worst
    /// fires as a spurious wake; failures such as EINTR also just end the wait.
    pub(crate) fn wait(
        &self,
        waker: &Waker,
        timeout: Option<Duration>,
        _mt: &MessageThread<'_>,
    ) -> usize {
        let zero = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        let _ = match timeout {
            Some(t) if t.is_zero() => waker.kevent(&[], 2, Some(&zero)),
            Some(t) => {
                let ns = isize::try_from(t.as_nanos()).unwrap_or(isize::MAX);
                let flags = libc::EV_ADD | libc::EV_ONESHOT;
                let fflags = libc::NOTE_NSECONDS | libc::NOTE_CRITICAL;
                waker.kevent(&[ev(1, libc::EVFILT_TIMER, flags, fflags, ns)], 2, None)
            }
            None => waker.kevent(&[], 2, None),
        };
        0
    }
}

// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The Linux backend: an `epoll` instance, an `eventfd` that posted tasks and quits write to,
//! and the `epoll_wait` timeout for timers. Peer file descriptors (`add_fd`) come in the next
//! increment and will share this epoll instance.

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::time::Duration;

use syrp_loop_api::MessageThread;

use crate::EventsError;

/// Events taken per `epoll_wait`; more ready descriptors are reported on the next pass.
const MAX_EVENTS: usize = 8;

/// Wakes the loop from any thread by writing to an `eventfd`.
pub(crate) struct Waker {
    fd: OwnedFd,
}

impl Waker {
    pub(crate) fn new() -> Result<Self, EventsError> {
        // SAFETY: eventfd takes plain integer arguments and touches no memory of ours.
        let fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) };
        Ok(Waker {
            fd: owned(fd, "eventfd")?,
        })
    }

    pub(crate) fn wake(&self) {
        let one = 1u64.to_ne_bytes();
        // SAFETY: the fd is a live eventfd we own, and `one` is 8 readable bytes for the call.
        // A full counter (EAGAIN) already means "woken", so the result is ignored.
        let _ = unsafe { libc::write(self.fd.as_raw_fd(), one.as_ptr().cast(), one.len()) };
    }

    fn drain(&self) {
        let mut buf = [0u8; 8];
        // SAFETY: the fd is a live eventfd we own, and `buf` is 8 writable bytes. A
        // non-blocking read of an unset counter fails with EAGAIN, which is fine.
        let _ = unsafe { libc::read(self.fd.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
    }
}

/// The epoll instance the loop blocks on.
pub(crate) struct Poller {
    epfd: OwnedFd,
}

impl Poller {
    pub(crate) fn new(waker: &Waker) -> Result<Self, EventsError> {
        // SAFETY: epoll_create1 takes a flag and touches no memory of ours.
        let epfd = unsafe { libc::epoll_create1(libc::EPOLL_CLOEXEC) };
        let epfd = owned(epfd, "epoll_create1")?;
        let wake_fd = waker.fd.as_raw_fd();
        let mut ev = libc::epoll_event {
            events: libc::EPOLLIN as u32,
            u64: 0,
        };
        // SAFETY: both fds are live descriptors we own, and `ev` is a valid, aligned
        // epoll_event that lives for the call.
        let rc =
            unsafe { libc::epoll_ctl(epfd.as_raw_fd(), libc::EPOLL_CTL_ADD, wake_fd, &mut ev) };
        if rc < 0 {
            return Err(EventsError::os("epoll_ctl", &io::Error::last_os_error()));
        }
        Ok(Poller { epfd })
    }

    /// Waits for a wake or the timeout (`None`: no limit). Runs no callbacks yet, so it
    /// returns 0.
    pub(crate) fn wait(
        &self,
        waker: &Waker,
        timeout: Option<Duration>,
        _mt: &MessageThread<'_>,
    ) -> usize {
        // Round up so a timer is never woken early and polled in a busy loop.
        let ms = timeout.map_or(-1, |t| {
            i32::try_from(t.as_nanos().div_ceil(1_000_000)).unwrap_or(i32::MAX)
        });
        let mut events = [libc::epoll_event { events: 0, u64: 0 }; MAX_EVENTS];
        // SAFETY: `events` is a valid, aligned, writable buffer of MAX_EVENTS entries that
        // outlives the call, and MAX_EVENTS fits in an i32.
        let n = unsafe {
            let max = MAX_EVENTS as i32;
            libc::epoll_wait(self.epfd.as_raw_fd(), events.as_mut_ptr(), max, ms)
        };
        // EINTR and other failures count as "nothing ready"; the loop re-checks and waits again.
        if n > 0 {
            waker.drain();
        }
        0
    }
}

/// Takes ownership of a descriptor a libc call returned, or reports the failed call.
fn owned(fd: RawFd, call: &'static str) -> Result<OwnedFd, EventsError> {
    if fd < 0 {
        return Err(EventsError::os(call, &io::Error::last_os_error()));
    }
    // SAFETY: `fd` was just returned by a successful libc call, so it is open and owned by
    // nobody else.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

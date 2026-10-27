// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The Linux backend: an `epoll` instance, an `eventfd` that posted tasks and quits write to,
//! and the `epoll_wait` timeout for timers. Peers add their own descriptors with
//! [`EventLoop::add_fd`].

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::io;
use std::os::fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd};
use std::rc::Rc;
use std::time::Duration;

use syrp_loop_api::{CallbackGuard, MessageThread};

use crate::{EventLoop, EventsError};

/// The epoll data value that marks the waker's eventfd; descriptor ids count up from 0.
const WAKE: u64 = u64::MAX;
/// Events taken per `epoll_wait`; more ready descriptors are reported on the next pass.
const MAX_EVENTS: usize = 32;

/// A file-descriptor callback: runs on the message thread while its descriptor is ready.
/// ```
/// let f: syrp_events::FdFn = Box::new(|_mt, ready| assert!(!ready.is_error()));
/// # drop(f);
/// ```
pub type FdFn = Box<dyn FnMut(&MessageThread<'_>, Readiness)>;

/// Returned by [`EventLoop::add_fd`]; dropping it removes the descriptor from the loop, also
/// from inside its own callback.
/// ```
/// use std::os::fd::AsFd;
/// use syrp_events::{EventLoop, Interest};
/// let lp = EventLoop::new()?;
/// let (a, _b) = std::os::unix::net::UnixStream::pair().unwrap();
/// let guard = lp.add_fd(a.as_fd(), Interest::WRITABLE, Box::new(|_mt, _r| {}))?;
/// assert_eq!(lp.dispatch_pending(), 1); // a socket with buffer space is writable
/// drop(guard);
/// assert_eq!(lp.dispatch_pending(), 0);
/// # Ok::<(), syrp_events::EventsError>(())
/// ```
pub type FdGuard = CallbackGuard;

/// Which readiness of a descriptor a callback wants (level-triggered).
/// ```
/// use syrp_events::Interest;
/// assert!(Interest::BOTH.is_readable() && Interest::BOTH.is_writable());
/// assert!(!Interest::READABLE.is_writable());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Interest {
    readable: bool,
    writable: bool,
}

impl Interest {
    /// Data can be read.
    pub const READABLE: Self = Interest {
        readable: true,
        writable: false,
    };
    /// Data can be written.
    pub const WRITABLE: Self = Interest {
        readable: false,
        writable: true,
    };
    /// Either.
    pub const BOTH: Self = Interest {
        readable: true,
        writable: true,
    };

    /// Whether readability is wanted.
    /// ```
    /// assert!(syrp_events::Interest::READABLE.is_readable());
    /// ```
    #[must_use]
    pub const fn is_readable(self) -> bool {
        self.readable
    }

    /// Whether writability is wanted.
    /// ```
    /// assert!(syrp_events::Interest::WRITABLE.is_writable());
    /// ```
    #[must_use]
    pub const fn is_writable(self) -> bool {
        self.writable
    }
}

/// What a ready descriptor reported, passed to its [`FdFn`]. Only the loop makes one; see
/// [`EventLoop::add_fd`] and the method examples.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Readiness {
    bits: u32,
}

impl Readiness {
    /// Data can be read (or the peer closed, so a read returns end of file).
    /// ```
    /// # use std::io::Write;
    /// # use std::os::fd::AsFd;
    /// # let lp = syrp_events::EventLoop::new()?;
    /// let (mut a, b) = std::os::unix::net::UnixStream::pair().unwrap();
    /// let f: syrp_events::FdFn = Box::new(|_mt, r| assert!(r.is_readable()));
    /// let _g = lp.add_fd(b.as_fd(), syrp_events::Interest::READABLE, f)?;
    /// a.write_all(b"x").unwrap();
    /// assert_eq!(lp.dispatch_pending(), 1);
    /// # Ok::<(), syrp_events::EventsError>(())
    /// ```
    #[must_use]
    pub const fn is_readable(self) -> bool {
        self.bits & (libc::EPOLLIN | libc::EPOLLRDHUP) as u32 != 0
    }

    /// Data can be written.
    /// ```
    /// # use std::os::fd::AsFd;
    /// # let lp = syrp_events::EventLoop::new()?;
    /// let (a, b) = std::os::unix::net::UnixStream::pair().unwrap();
    /// let f: syrp_events::FdFn = Box::new(|_mt, r| assert!(r.is_writable()));
    /// let _g = lp.add_fd(b.as_fd(), syrp_events::Interest::WRITABLE, f)?;
    /// assert_eq!(lp.dispatch_pending(), 1);
    /// # Ok::<(), syrp_events::EventsError>(())
    /// ```
    #[must_use]
    pub const fn is_writable(self) -> bool {
        self.bits & libc::EPOLLOUT as u32 != 0
    }

    /// The descriptor reported an error (always reported, whatever the [`Interest`]).
    /// ```
    /// # use std::os::fd::AsFd;
    /// # let lp = syrp_events::EventLoop::new()?;
    /// let (a, b) = std::os::unix::net::UnixStream::pair().unwrap();
    /// let f: syrp_events::FdFn = Box::new(|_mt, r| assert!(!r.is_error()));
    /// let _g = lp.add_fd(b.as_fd(), syrp_events::Interest::WRITABLE, f)?;
    /// assert_eq!(lp.dispatch_pending(), 1);
    /// # Ok::<(), syrp_events::EventsError>(())
    /// ```
    #[must_use]
    pub const fn is_error(self) -> bool {
        self.bits & libc::EPOLLERR as u32 != 0
    }

    /// The other end hung up.
    /// ```
    /// # use std::os::fd::AsFd;
    /// # let lp = syrp_events::EventLoop::new()?;
    /// let (a, b) = std::os::unix::net::UnixStream::pair().unwrap();
    /// let f: syrp_events::FdFn = Box::new(|_mt, r| assert!(r.is_hangup()));
    /// let _g = lp.add_fd(b.as_fd(), syrp_events::Interest::READABLE, f)?;
    /// drop(a);
    /// assert_eq!(lp.dispatch_pending(), 1);
    /// # Ok::<(), syrp_events::EventsError>(())
    /// ```
    #[must_use]
    pub const fn is_hangup(self) -> bool {
        self.bits & (libc::EPOLLHUP | libc::EPOLLRDHUP) as u32 != 0
    }
}

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

struct FdSlot {
    fd: RawFd,
    f: Option<FdFn>,
}

/// The epoll instance and the descriptors registered on it.
pub(crate) struct Poller {
    epfd: OwnedFd,
    fds: RefCell<HashMap<u64, FdSlot>>,
    next_id: Cell<u64>,
}

impl Poller {
    pub(crate) fn new(waker: &Waker) -> Result<Self, EventsError> {
        // SAFETY: epoll_create1 takes a flag and touches no memory of ours.
        let epfd = owned(
            unsafe { libc::epoll_create1(libc::EPOLL_CLOEXEC) },
            "epoll_create1",
        )?;
        let poller = Poller {
            epfd,
            fds: RefCell::default(),
            next_id: Cell::new(0),
        };
        poller
            .ctl(
                libc::EPOLL_CTL_ADD,
                waker.fd.as_raw_fd(),
                libc::EPOLLIN as u32,
                WAKE,
            )
            .map_err(|e| EventsError::os("epoll_ctl", &e))?;
        Ok(poller)
    }

    fn ctl(&self, op: i32, fd: RawFd, events: u32, data: u64) -> io::Result<()> {
        let mut ev = libc::epoll_event { events, u64: data };
        // SAFETY: both fds are plain integers (the kernel validates them) and `ev` is a valid,
        // aligned epoll_event that lives for the call.
        let rc = unsafe { libc::epoll_ctl(self.epfd.as_raw_fd(), op, fd, &raw mut ev) };
        if rc < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    /// Nothing else drives an epoll loop, so the core never needs calling back.
    pub(crate) fn attach(&self, _dispatch: Rc<dyn Fn() -> usize>) {}

    /// The `wait` timeout covers timers, so there is nothing to arm.
    pub(crate) fn arm(&self, _next: Option<Duration>) {}

    /// Waits for a wake, a ready descriptor or the timeout, then runs ready descriptors'
    /// callbacks. Returns how many ran.
    pub(crate) fn wait(
        &self,
        waker: &Waker,
        timeout: Option<Duration>,
        mt: &MessageThread<'_>,
    ) -> usize {
        // Round up so a timer is never woken early and polled in a busy loop.
        let ms = timeout.map_or(-1, |t| {
            i32::try_from(t.as_nanos().div_ceil(1_000_000)).unwrap_or(i32::MAX)
        });
        let mut events = [libc::epoll_event { events: 0, u64: 0 }; MAX_EVENTS];
        // SAFETY: `events` is a valid, aligned buffer of MAX_EVENTS entries that outlives the
        // call, and MAX_EVENTS fits in an i32.
        let n = unsafe {
            libc::epoll_wait(
                self.epfd.as_raw_fd(),
                events.as_mut_ptr(),
                MAX_EVENTS as i32,
                ms,
            )
        };
        // EINTR and other failures count as "nothing ready"; the loop re-checks and waits again.
        let ready = &events[..usize::try_from(n).unwrap_or(0)];
        let mut ran = 0;
        for ev in ready {
            let (id, bits) = (ev.u64, ev.events);
            if id == WAKE {
                waker.drain();
                continue;
            }
            let taken = self.fds.borrow_mut().get_mut(&id).and_then(|s| s.f.take());
            let Some(mut f) = taken else { continue };
            f(mt, Readiness { bits });
            ran += 1;
            let mut fds = self.fds.borrow_mut();
            match fds.get_mut(&id) {
                Some(slot) => slot.f = Some(f),
                None => {
                    drop(fds); // removed in its own callback; drop it outside the borrow
                    drop(f);
                }
            }
        }
        ran
    }
}

impl EventLoop {
    /// Calls `f` on the message thread whenever `fd` is ready for `interest`
    /// (level-triggered), until the guard is dropped. For window-system peers and IPC; the
    /// loop opens no display connection itself. Linux only for now.
    ///
    /// Keep `fd` open while the guard lives: the guard unregisters by descriptor number, so
    /// closing first and reusing the number for another registration would remove that one.
    ///
    /// # Errors
    /// [`EventsError::Os`] if epoll refuses the descriptor (for example a regular file, or
    /// one that is already registered).
    /// ```
    /// use std::io::Write;
    /// use std::os::fd::AsFd;
    /// use std::os::unix::net::UnixStream;
    /// use syrp_events::{EventLoop, Interest};
    /// let lp = EventLoop::new()?;
    /// let (mut a, b) = UnixStream::pair().unwrap();
    /// let _g = lp.add_fd(b.as_fd(), Interest::READABLE, Box::new(|_mt, r| assert!(r.is_readable())))?;
    /// a.write_all(b"x").unwrap();
    /// assert_eq!(lp.dispatch_pending(), 1);
    /// # Ok::<(), syrp_events::EventsError>(())
    /// ```
    pub fn add_fd(
        &self,
        fd: BorrowedFd<'_>,
        interest: Interest,
        f: FdFn,
    ) -> Result<FdGuard, EventsError> {
        let inner = self.inner();
        let poller = &inner.poller;
        let id = poller.next_id.get();
        poller.next_id.set(id + 1);
        let mut events = 0;
        if interest.readable {
            events |= (libc::EPOLLIN | libc::EPOLLRDHUP) as u32;
        }
        if interest.writable {
            events |= libc::EPOLLOUT as u32;
        }
        let raw = fd.as_raw_fd();
        (poller.ctl(libc::EPOLL_CTL_ADD, raw, events, id))
            .map_err(|e| EventsError::os("epoll_ctl", &e))?;
        poller.fds.borrow_mut().insert(
            id,
            FdSlot {
                fd: raw,
                f: Some(f),
            },
        );
        let weak = Rc::downgrade(inner);
        Ok(FdGuard::new(move || {
            let Some(inner) = weak.upgrade() else { return };
            let removed = inner.poller.fds.borrow_mut().remove(&id);
            if let Some(slot) = &removed {
                // The descriptor may be closed already, which removed it; ignore that error.
                let _ = inner.poller.ctl(libc::EPOLL_CTL_DEL, slot.fd, 0, id);
            }
            drop(removed); // outside the borrow: the callback may own other guards
        }))
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

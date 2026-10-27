// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The macOS backend: the current thread's `CFRunLoop`. Posted tasks and quits signal a
//! version-0 `CFRunLoopSource`. Timer deadlines come from a `kqueue` whose one-shot
//! `EVFILT_TIMER` carries `NOTE_CRITICAL`, surfaced in the run loop through a
//! `CFFileDescriptor`. Both sources are scheduled in the common modes.
//!
//! Why not a `CFRunLoopTimer`: Darwin coalesces ordinary timed waits (increment 1 measured
//! condvar and plain `kevent` timeouts overshooting a 10 ms period on CI), and run-loop timers
//! are armed as ordinary kernel timers, so a tolerance of zero does not rule coalescing out.
//! Critical kqueue timers are exempt, and their wake reaches the run loop through a port, with
//! no timeout of its own. (Syrp's choice; the spec names `CFRunLoopTimer`.)
//!
//! When something else runs the run loop (such as `NSApplication run` on the main thread), the
//! source callbacks dispatch tasks and timers themselves. Inside the loop's own `wait` they
//! only wake it and the core dispatches, so a pass behaves as on Linux.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::Duration;
use std::{io, ptr};

use core_foundation_sys::base::{CFOptionFlags, CFRelease, CFRetain, CFTypeRef};
use core_foundation_sys::filedescriptor::{
    CFFileDescriptorContext, CFFileDescriptorCreate, CFFileDescriptorCreateRunLoopSource,
    CFFileDescriptorEnableCallBacks, CFFileDescriptorInvalidate, CFFileDescriptorRef,
    kCFFileDescriptorReadCallBack,
};
use core_foundation_sys::runloop::{
    CFRunLoopAddSource, CFRunLoopGetCurrent, CFRunLoopRunInMode, CFRunLoopSourceContext,
    CFRunLoopSourceCreate, CFRunLoopSourceInvalidate, CFRunLoopSourceSignal, CFRunLoopWakeUp,
    kCFRunLoopCommonModes, kCFRunLoopDefaultMode,
};
use syrp_loop_api::MessageThread;

use crate::EventsError;

/// Seconds that stand for "no limit" in `CFRunLoopRunInMode`.
const NEVER: f64 = 1.0e9;
/// Extra seconds on a timed run-loop wait: the critical kqueue timer ends it, and the
/// run loop's own (coalescible) timeout is only a backstop.
const BACKSTOP: f64 = 0.05;

/// One reference to a Core Foundation object, released on drop.
struct Cf(CFTypeRef);

// SAFETY: Core Foundation reference counting is thread-safe, and off the loop's thread the
// backend only calls CFRunLoopSourceSignal, CFRunLoopWakeUp and CFRelease, which Apple
// documents as safe from any thread.
unsafe impl Send for Cf {}
// SAFETY: as for `Send`; shared access makes the same thread-safe calls.
unsafe impl Sync for Cf {}

impl Cf {
    /// Takes a reference a CF call returned, or reports the call that returned null.
    fn new<T>(obj: *mut T, call: &'static str) -> Result<Self, EventsError> {
        if obj.is_null() {
            return Err(EventsError::Os {
                call,
                os_error: None,
            });
        }
        Ok(Cf(obj.cast_const().cast()))
    }

    fn get<T>(&self) -> *mut T {
        self.0.cast_mut().cast()
    }
}

impl Drop for Cf {
    fn drop(&mut self) {
        // SAFETY: we own one reference to this non-null object, and give it up once.
        unsafe { CFRelease(self.0) }
    }
}

/// Wakes the loop from any thread by signalling its run-loop source.
pub(crate) struct Waker {
    run_loop: Cf,
    /// Set by `Poller::new` right after the waker is made, before anyone can post.
    source: OnceLock<Cf>,
}

impl Waker {
    pub(crate) fn new() -> Result<Self, EventsError> {
        // SAFETY: CFRunLoopGetCurrent returns the current thread's run loop without giving us
        // a reference; CFRetain takes one, and returns null only for a null argument.
        let run_loop = unsafe { CFRetain(CFRunLoopGetCurrent().cast_const().cast()) };
        Ok(Waker {
            run_loop: Cf::new(run_loop.cast_mut(), "CFRunLoopGetCurrent")?,
            source: OnceLock::new(),
        })
    }

    pub(crate) fn wake(&self) {
        let Some(source) = self.source.get() else {
            return;
        };
        // SAFETY: both objects are live (we hold references), and Apple documents both calls
        // as safe from any thread.
        unsafe {
            CFRunLoopSourceSignal(source.get());
            CFRunLoopWakeUp(self.run_loop.get());
        }
    }
}

/// What the run-loop callbacks reach through their `info` pointer. Loop thread only.
struct Hooks {
    /// The core's dispatch (tasks, then due timers), set by `attach`.
    dispatch: RefCell<Option<Rc<dyn Fn() -> usize>>>,
    /// Set while the loop's own `wait` runs the run loop; callbacks then only wake it.
    in_wait: Cell<bool>,
    /// The kqueue that holds the deadline timer, watched by `fd_ref`.
    kq: OwnedFd,
}

impl Hooks {
    /// Applies `changes` (at most one), then takes up to two ready events without waiting.
    fn kevent(&self, changes: &[libc::kevent]) {
        let zero = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        let mut out = [timer_event(0, 0); 2];
        let n_changes = i32::from(!changes.is_empty());
        // SAFETY: the kqueue is live, `changes` and `out` are valid for the lengths passed,
        // and `zero` outlives the call. A failure (EINTR) only loses a wake that the core's
        // own clock check makes up for.
        unsafe {
            let (c, o) = (changes.as_ptr(), out.as_mut_ptr());
            libc::kevent(self.kq.as_raw_fd(), c, n_changes, o, 2, &raw const zero);
        }
    }
}

/// A one-shot critical timer `ns` from now (`flags` 0 deletes it).
fn timer_event(flags: u16, ns: isize) -> libc::kevent {
    libc::kevent {
        ident: 1,
        filter: libc::EVFILT_TIMER,
        flags,
        fflags: libc::NOTE_NSECONDS | libc::NOTE_CRITICAL,
        data: ns,
        udata: ptr::null_mut(),
    }
}

/// The run-loop sources, scheduled on the loop thread's run loop.
pub(crate) struct Poller {
    hooks: Rc<Hooks>,
    source: Cf,
    fd_ref: Cf,
    fd_source: Cf,
}

impl Poller {
    pub(crate) fn new(waker: &Waker) -> Result<Self, EventsError> {
        // SAFETY: kqueue takes no arguments and touches no memory of ours.
        let kq = unsafe { libc::kqueue() };
        if kq < 0 {
            return Err(EventsError::os("kqueue", &io::Error::last_os_error()));
        }
        // SAFETY: `kq` was just returned by a successful kqueue call; nobody else owns it.
        let kq = unsafe { OwnedFd::from_raw_fd(kq) };
        let raw_kq = kq.as_raw_fd();
        let hooks = Rc::new(Hooks {
            dispatch: RefCell::new(None),
            in_wait: Cell::new(false),
            kq,
        });
        let info = Rc::as_ptr(&hooks).cast_mut().cast::<c_void>();
        let mut source_cx = CFRunLoopSourceContext {
            version: 0,
            info,
            retain: None,
            release: None,
            copyDescription: None,
            equal: None,
            hash: None,
            schedule: None,
            cancel: None,
            perform: on_source,
        };
        let fd_cx = CFFileDescriptorContext {
            version: 0,
            info,
            retain: None,
            release: None,
            copyDescription: None,
        };
        // SAFETY: CF copies both contexts during the create calls. `info` stays valid for as
        // long as the sources can call back, because `Drop` invalidates them before `hooks`
        // is released. The run loop is the current thread's, so callbacks run on this thread.
        // `raw_kq` stays open until `hooks` drops (CF is told not to close it).
        let (source, fd_ref, fd_source) = unsafe {
            let source = CFRunLoopSourceCreate(ptr::null(), 0, &raw mut source_cx);
            let source = Cf::new(source, "CFRunLoopSourceCreate")?;
            let fd_ref = CFFileDescriptorCreate(ptr::null(), raw_kq, 0, on_fd, &raw const fd_cx);
            let fd_ref = Cf::new(fd_ref, "CFFileDescriptorCreate")?;
            let fd_source = CFFileDescriptorCreateRunLoopSource(ptr::null(), fd_ref.get(), 0);
            let fd_source = Cf::new(fd_source, "CFFileDescriptorCreateRunLoopSource")?;
            let run_loop = CFRunLoopGetCurrent();
            CFRunLoopAddSource(run_loop, source.get(), kCFRunLoopCommonModes);
            CFRunLoopAddSource(run_loop, fd_source.get(), kCFRunLoopCommonModes);
            CFFileDescriptorEnableCallBacks(fd_ref.get(), kCFFileDescriptorReadCallBack);
            (source, fd_ref, fd_source)
        };
        // SAFETY: `source` is live; CFRetain takes the waker's own reference to it.
        let for_waker = unsafe { CFRetain(source.0) };
        let _ = waker.source.set(Cf::new(for_waker.cast_mut(), "CFRetain")?);
        Ok(Poller {
            hooks,
            source,
            fd_ref,
            fd_source,
        })
    }

    /// Gives the run-loop callbacks the core's dispatch, for when something else runs the
    /// run loop.
    pub(crate) fn attach(&self, dispatch: Rc<dyn Fn() -> usize>) {
        *self.hooks.dispatch.borrow_mut() = Some(dispatch);
    }

    /// Arms the critical timer to fire `next` from now (`None`: never).
    pub(crate) fn arm(&self, next: Option<Duration>) {
        let change = match next {
            Some(d) => {
                let ns = isize::try_from(d.as_nanos()).unwrap_or(isize::MAX).max(1);
                timer_event(libc::EV_ADD | libc::EV_ONESHOT, ns)
            }
            None => timer_event(libc::EV_DELETE, 0),
        };
        self.hooks.kevent(&[change]);
    }

    /// Runs the run loop until a source fires or the timeout (`None`: no limit) passes. The
    /// callbacks only wake it, so it runs nothing itself and returns 0.
    pub(crate) fn wait(
        &self,
        _waker: &Waker,
        timeout: Option<Duration>,
        _mt: &MessageThread<'_>,
    ) -> usize {
        let secs = match timeout {
            Some(t) if t.is_zero() => 0.0,
            Some(t) => {
                self.arm(Some(t));
                t.as_secs_f64() + BACKSTOP
            }
            None => NEVER,
        };
        let was = self.hooks.in_wait.replace(true);
        // SAFETY: runs the current thread's run loop in its default mode, which holds both
        // sources (common modes); it returns once one is handled or on timeout.
        unsafe { CFRunLoopRunInMode(kCFRunLoopDefaultMode, secs, 1) };
        self.hooks.in_wait.set(was);
        0
    }
}

impl Drop for Poller {
    fn drop(&mut self) {
        // SAFETY: all three objects are live. Invalidating removes the sources from every run
        // loop, so CF makes no more callbacks with `info` once `hooks` is released.
        unsafe {
            CFRunLoopSourceInvalidate(self.source.get());
            CFRunLoopSourceInvalidate(self.fd_source.get());
            CFFileDescriptorInvalidate(self.fd_ref.get());
        }
    }
}

extern "C" fn on_source(info: *const c_void) {
    fire(info, |_| {});
}

extern "C" fn on_fd(f: CFFileDescriptorRef, _types: CFOptionFlags, info: *mut c_void) {
    fire(info, |hooks| {
        // Take the timer event, then ask CF for the next readability (it disables callbacks
        // after each one).
        hooks.kevent(&[]);
        // SAFETY: `f` is the live descriptor object this callback was registered on.
        unsafe { CFFileDescriptorEnableCallBacks(f, kCFFileDescriptorReadCallBack) };
    });
}

/// Runs `first` (draining), then dispatches tasks and timers if something else runs the run
/// loop. A panic here aborts the process, as for any panic that would unwind into CF.
fn fire(info: *const c_void, first: impl FnOnce(&Hooks)) {
    let hooks = info.cast::<Hooks>();
    // SAFETY: `info` is the `Rc<Hooks>` of a live `Poller` (its `Drop` stops all callbacks
    // first), and CF calls back only on the loop's thread. Taking our own count keeps `hooks`
    // alive if the dispatch drops the loop.
    let hooks = unsafe {
        Rc::increment_strong_count(hooks);
        Rc::from_raw(hooks)
    };
    first(&hooks);
    if hooks.in_wait.get() {
        return;
    }
    let dispatch = hooks.dispatch.borrow().clone();
    if let Some(dispatch) = dispatch {
        dispatch();
    }
}

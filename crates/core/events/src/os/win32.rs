// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The Windows backend: a hidden message-only window (`HWND_MESSAGE`) of the crate's own
//! class. Posted tasks and quits wake it with `PostMessageW` of a private message; timers come
//! from the core's deadline schedule as the `MsgWaitForMultipleObjectsEx` timeout, not from
//! `SetTimer`. Each wait translates and dispatches the thread's other messages, so windows of
//! later peers work. The loop holds a 1 ms system timer resolution (`timeBeginPeriod`) for its
//! lifetime, because wait timeouts otherwise follow the 15.6 ms tick.
//!
//! When something else pumps the thread's messages (a modal dialog), the window procedure
//! dispatches tasks and due timers on each wake message. Timers alone do not wake a foreign
//! pump.

use std::cell::{Cell, RefCell};
use std::ptr;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use syrp_loop_api::MessageThread;
use windows_sys::Win32::Foundation::{GetLastError, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Media::{timeBeginPeriod, timeEndPeriod};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GWLP_USERDATA,
    GetWindowLongPtrW, HWND_MESSAGE, MSG, MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx,
    PM_REMOVE, PeekMessageW, PostMessageW, QS_ALLINPUT, RegisterClassExW, SetWindowLongPtrW,
    TranslateMessage, WM_APP, WNDCLASSEXW,
};

use crate::EventsError;

/// The private wake message; `WM_APP` values are private to a window class.
const WM_WAKE: u32 = WM_APP + 0x5359;
/// Messages taken per wait, so a source that keeps posting cannot starve tasks and timers.
const MAX_MESSAGES: usize = 64;
/// `INFINITE` for the wait timeout.
const INFINITE: u32 = u32::MAX;

/// The state the waker shares with the poller.
#[derive(Default)]
struct WakeState {
    /// Set by `Poller::new` and cleared by its `Drop`; null means no window.
    hwnd: AtomicPtr<core::ffi::c_void>,
    /// Whether a `WM_WAKE` is already queued, so wakes do not flood the message queue.
    pending: AtomicBool,
}

/// Wakes the loop from any thread by posting `WM_WAKE` to its window.
pub(crate) struct Waker {
    state: Arc<WakeState>,
}

impl Waker {
    pub(crate) fn new() -> Result<Self, EventsError> {
        Ok(Waker {
            state: Arc::default(),
        })
    }

    pub(crate) fn wake(&self) {
        let hwnd = self.state.hwnd.load(Ordering::Acquire);
        if hwnd.is_null() || self.state.pending.swap(true, Ordering::AcqRel) {
            return;
        }
        // SAFETY: PostMessageW is documented as callable from any thread. If the window was
        // destroyed meanwhile, the call fails (or reaches a reused handle that ignores an
        // unknown private message); either way nothing is touched on our side.
        if unsafe { PostMessageW(hwnd, WM_WAKE, 0, 0) } == 0 {
            self.state.pending.store(false, Ordering::Release);
        }
    }
}

/// What the window procedure reaches through `GWLP_USERDATA`. Loop thread only.
struct Hooks {
    dispatch: RefCell<Option<Rc<dyn Fn() -> usize>>>,
    in_wait: Cell<bool>,
    /// Shared with the waker; its pending flag is cleared when the message arrives.
    wake: Arc<WakeState>,
}

/// The loop's window and its 1 ms timer-resolution request.
pub(crate) struct Poller {
    hwnd: HWND,
    hooks: Rc<Hooks>,
}

/// Registers the crate's window class once per process; returns its atom.
fn class_atom() -> Result<u16, EventsError> {
    static ATOM: OnceLock<Result<u16, EventsError>> = OnceLock::new();
    ATOM.get_or_init(|| {
        let name: Vec<u16> = "SyrpEventLoopWindow\0".encode_utf16().collect();
        let class = WNDCLASSEXW {
            cbSize: u32::try_from(size_of::<WNDCLASSEXW>()).unwrap_or(u32::MAX),
            lpfnWndProc: Some(window_proc),
            // SAFETY: a null name asks for the module that created this process; no memory
            // of ours is read.
            hInstance: unsafe { GetModuleHandleW(ptr::null()) },
            lpszClassName: name.as_ptr(),
            ..WNDCLASSEXW::default()
        };
        // SAFETY: `class` is a fully initialised WNDCLASSEXW with its size set, and `name`
        // outlives the call (the system copies the class name).
        let atom = unsafe { RegisterClassExW(&raw const class) };
        if atom != 0 {
            return Ok(atom);
        }
        // SAFETY: reads this thread's last error; no arguments.
        let err = unsafe { GetLastError() };
        // Fails too if another copy of this crate registered the name (with its own procedure).
        Err(EventsError::Os {
            call: "RegisterClassExW",
            os_error: i32::try_from(err).ok(),
        })
    })
    .clone()
}

impl Poller {
    pub(crate) fn new(waker: &Waker) -> Result<Self, EventsError> {
        let atom = class_atom()?;
        // SAFETY: the atom names a registered class (passed as MAKEINTATOM); HWND_MESSAGE
        // makes a message-only window; no other pointers are passed.
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                usize::from(atom) as *const u16,
                ptr::null(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                ptr::null_mut(),
                GetModuleHandleW(ptr::null()),
                ptr::null(),
            )
        };
        if hwnd.is_null() {
            // SAFETY: reads this thread's last error; no arguments.
            let err = unsafe { GetLastError() };
            return Err(EventsError::Os {
                call: "CreateWindowExW",
                os_error: i32::try_from(err).ok(),
            });
        }
        let hooks = Rc::new(Hooks {
            dispatch: RefCell::new(None),
            in_wait: Cell::new(false),
            wake: Arc::clone(&waker.state),
        });
        // SAFETY: `hwnd` is our live window, and the pointer stays valid until `Drop` clears
        // it before `hooks` is released. timeBeginPeriod takes a plain integer and `Drop` ends
        // the request; a refusal only leaves the default resolution.
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Rc::as_ptr(&hooks) as isize);
            timeBeginPeriod(1);
        }
        waker.state.hwnd.store(hwnd, Ordering::Release);
        Ok(Poller { hwnd, hooks })
    }

    /// Gives the window procedure the core's dispatch, for when something else pumps
    /// messages.
    pub(crate) fn attach(&self, dispatch: Rc<dyn Fn() -> usize>) {
        *self.hooks.dispatch.borrow_mut() = Some(dispatch);
    }

    /// The `wait` timeout covers timers, so there is nothing to arm.
    pub(crate) fn arm(&self, _next: Option<Duration>) {}

    /// Waits for a message or the timeout (`None`: no limit), then translates and dispatches
    /// up to `MAX_MESSAGES` queued messages. Our own wake only ends the wait, so this runs no
    /// loop callbacks and returns 0. A `WM_QUIT` is taken and ignored: stop the loop with a
    /// `QuitHandle`.
    pub(crate) fn wait(
        &self,
        _waker: &Waker,
        timeout: Option<Duration>,
        _mt: &MessageThread<'_>,
    ) -> usize {
        // Round up so a timer is never woken early and polled in a busy loop.
        let ms = timeout.map_or(INFINITE, |t| {
            u32::try_from(t.as_nanos().div_ceil(1_000_000))
                .map_or(INFINITE - 1, |m| m.min(INFINITE - 1))
        });
        let was = self.hooks.in_wait.replace(true);
        // SAFETY: no handles are passed (count 0, null array); the call only waits on this
        // thread's message queue.
        unsafe {
            MsgWaitForMultipleObjectsEx(0, ptr::null(), ms, QS_ALLINPUT, MWMO_INPUTAVAILABLE)
        };
        // SAFETY: MSG is plain data, all-zero is a valid value.
        let mut msg: MSG = unsafe { std::mem::zeroed() };
        for _ in 0..MAX_MESSAGES {
            // SAFETY: `msg` is a valid MSG to write to; a null window takes any message of
            // this thread. Translate and dispatch get the message just read.
            unsafe {
                if PeekMessageW(&raw mut msg, ptr::null_mut(), 0, 0, PM_REMOVE) == 0 {
                    break;
                }
                TranslateMessage(&raw const msg);
                DispatchMessageW(&raw const msg);
            }
        }
        self.hooks.in_wait.set(was);
        0
    }
}

impl Drop for Poller {
    fn drop(&mut self) {
        self.hooks
            .wake
            .hwnd
            .store(ptr::null_mut(), Ordering::Release);
        // SAFETY: this is our window, destroyed on its own thread; clearing the user data
        // first stops the procedure from reaching `hooks` during destruction. Ends the
        // timeBeginPeriod(1) request made in `new`.
        unsafe {
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
            DestroyWindow(self.hwnd);
            timeEndPeriod(1);
        }
    }
}

/// The window procedure: `WM_WAKE` clears the pending flag and, under a foreign pump,
/// dispatches tasks and due timers. A panic here aborts the process, as for any panic that
/// would unwind into the system.
unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // SAFETY: `hwnd` is the window this procedure was called for.
    let data = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const Hooks;
    if msg != WM_WAKE || data.is_null() {
        // SAFETY: forwards the call unchanged to the default procedure.
        return unsafe { DefWindowProcW(hwnd, msg, wp, lp) };
    }
    // SAFETY: `data` is the `Rc<Hooks>` of the live `Poller` that owns this window (its `Drop`
    // clears the pointer first), and window procedures run on the window's thread. Taking our
    // own count keeps `hooks` alive if the dispatch drops the loop.
    let hooks = unsafe {
        Rc::increment_strong_count(data);
        Rc::from_raw(data)
    };
    hooks.wake.pending.store(false, Ordering::Release);
    if !hooks.in_wait.get() {
        let dispatch = hooks.dispatch.borrow().clone();
        if let Some(dispatch) = dispatch {
            dispatch();
        }
    }
    0
}

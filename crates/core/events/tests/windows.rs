// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B6 tests of work/specs/WP-1.1-14.md: the Win32 backend. Other targets run nothing.
#![cfg(windows)]

use std::cell::Cell;
use std::ptr;
use std::time::{Duration, Instant};

use syrp_events::EventLoop;
use syrp_loop_api::{MessageLoop, Task};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, HWND_MESSAGE, MSG,
    MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx, PM_REMOVE, PeekMessageW, PostMessageW,
    QS_ALLINPUT, RegisterClassExW, TranslateMessage, WM_APP, WNDCLASSEXW,
};

const WM_PEER: u32 = WM_APP + 7;
/// Makes the peer run a nested message pump, as a modal loop (move/size, MessageBox) does.
const WM_PEER_MODAL: u32 = WM_APP + 8;

thread_local! {
    static PEER_MESSAGES: Cell<u32> = const { Cell::new(0) };
    static TASKS_RUN: Cell<u32> = const { Cell::new(0) };
    static TIMERS_FIRED: Cell<u32> = const { Cell::new(0) };
    /// (tasks, timers) seen inside the nested pump.
    static SEEN_IN_PUMP: Cell<(u32, u32)> = const { Cell::new((0, 0)) };
}

/// Pumps this thread's messages for `d`, like a modal loop that is not the `EventLoop`.
fn nested_pump(d: Duration) {
    let start = Instant::now();
    // SAFETY: MSG is plain data and all-zero is valid; the calls only read and dispatch this
    // thread's own message queue.
    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        while start.elapsed() < d {
            MsgWaitForMultipleObjectsEx(0, ptr::null(), 5, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
            while PeekMessageW(&raw mut msg, ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&raw const msg);
                DispatchMessageW(&raw const msg);
            }
        }
    }
}

unsafe extern "system" fn peer_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_PEER {
        PEER_MESSAGES.set(PEER_MESSAGES.get() + 1);
        return 0;
    }
    if msg == WM_PEER_MODAL {
        let before = (TASKS_RUN.get(), TIMERS_FIRED.get());
        nested_pump(Duration::from_millis(200));
        SEEN_IN_PUMP.set((TASKS_RUN.get() - before.0, TIMERS_FIRED.get() - before.1));
        return 0;
    }
    // SAFETY: forwards the call unchanged to the default procedure.
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

/// A message-only window of a class of the test's own, standing in for a later peer.
fn peer_window() -> HWND {
    let name: Vec<u16> = "SyrpEventsTestPeer\0".encode_utf16().collect();
    // SAFETY: a null name asks for this process's module; the class struct is fully
    // initialised, `name` outlives both calls, and HWND_MESSAGE makes a message-only window.
    unsafe {
        let hinstance = GetModuleHandleW(ptr::null());
        let class = WNDCLASSEXW {
            cbSize: u32::try_from(size_of::<WNDCLASSEXW>()).unwrap(),
            lpfnWndProc: Some(peer_proc),
            hInstance: hinstance,
            lpszClassName: name.as_ptr(),
            ..WNDCLASSEXW::default()
        };
        RegisterClassExW(&raw const class); // a second test registers it again; that may fail
        let (null, zero) = (ptr::null_mut(), 0);
        let hwnd = CreateWindowExW(
            0,
            name.as_ptr(),
            ptr::null(),
            0,
            zero,
            zero,
            zero,
            zero,
            HWND_MESSAGE,
            null,
            hinstance,
            ptr::null(),
        );
        assert!(!hwnd.is_null());
        hwnd
    }
}

#[test]
fn b6_messages_for_other_windows_on_the_thread_are_dispatched() {
    let lp = EventLoop::new().unwrap();
    let peer = peer_window();
    // SAFETY: `peer` is a live window of this thread.
    assert_ne!(unsafe { PostMessageW(peer, WM_PEER, 0, 0) }, 0);
    lp.post(Task::new(|_| {}));
    assert_eq!(lp.dispatch_pending(), 1); // the task; the peer message is not a loop callback
    assert_eq!(PEER_MESSAGES.get(), 1);
    // SAFETY: our own window, destroyed on its thread.
    unsafe { DestroyWindow(peer) };
}

#[test]
fn b6_a_peer_message_ends_a_blocked_wait_and_timers_keep_their_resolution() {
    let lp = EventLoop::new().unwrap();
    let peer = peer_window();
    let fired = std::rc::Rc::new(Cell::new(0));
    let f = fired.clone();
    let _t = lp.timer(
        Duration::from_millis(2),
        Box::new(move |_| f.set(f.get() + 1)),
    );
    // SAFETY: `peer` is a live window of this thread.
    unsafe { PostMessageW(peer, WM_PEER, 0, 0) };
    lp.run_for(Duration::from_millis(100));
    assert!(PEER_MESSAGES.get() >= 1);
    // 2 ms timers over 100 ms: well above what a 15.6 ms tick would allow (at most 7).
    assert!(fired.get() >= 20, "fired {}", fired.get());
    // SAFETY: our own window, destroyed on its thread.
    unsafe { DestroyWindow(peer) };
}

#[test]
fn b6_a_nested_pump_in_a_peer_handler_still_runs_tasks_and_timers() {
    // Regression: the pump starts inside the loop's own wait (dispatch_pending below).
    let lp = EventLoop::new().unwrap();
    let peer = peer_window();
    // SAFETY: `peer` is a live window of this thread.
    assert_ne!(unsafe { PostMessageW(peer, WM_PEER_MODAL, 0, 0) }, 0);
    lp.post(Task::new(|_| TASKS_RUN.set(TASKS_RUN.get() + 1)));
    let _t = lp.timer(
        Duration::from_millis(20),
        Box::new(|_| TIMERS_FIRED.set(TIMERS_FIRED.get() + 1)),
    );
    lp.dispatch_pending();
    let (tasks, timers) = SEEN_IN_PUMP.get();
    assert_eq!(tasks, 1, "the posted task runs inside the nested pump");
    assert!(timers >= 1, "timers fire inside the nested pump ({timers})");
    // SAFETY: our own window, destroyed on its thread.
    unsafe { DestroyWindow(peer) };
}

#[test]
fn b7_vblank_fallback_runs_under_a_nested_pump_unless_driven() {
    let lp = EventLoop::new().unwrap();
    let peer = peer_window();
    let (free, driven) = (
        syrp_loop_api::PeerId::from_raw(1),
        syrp_loop_api::PeerId::from_raw(2),
    );
    let _g = lp.on_vblank(
        free,
        Box::new(|_, _| TIMERS_FIRED.set(TIMERS_FIRED.get() + 1)),
    );
    let _d = lp.vblank_driver(driven);
    let driven_count = std::rc::Rc::new(Cell::new(0));
    let c = driven_count.clone();
    let _h = lp.on_vblank(driven, Box::new(move |_, _| c.set(c.get() + 1)));
    // SAFETY: `peer` is a live window of this thread.
    assert_ne!(unsafe { PostMessageW(peer, WM_PEER_MODAL, 0, 0) }, 0);
    lp.dispatch_pending();
    assert!(
        SEEN_IN_PUMP.get().1 >= 2,
        "fallback refreshes in the pump: {:?}",
        SEEN_IN_PUMP.get()
    );
    assert_eq!(
        driven_count.get(),
        0,
        "a driven peer's fallback stays stopped"
    );
    // SAFETY: our own window, destroyed on its thread.
    unsafe { DestroyWindow(peer) };
}

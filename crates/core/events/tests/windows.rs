// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B6 tests of work/specs/WP-1.1-14.md: the Win32 backend. Other targets run nothing.
#![cfg(windows)]

use std::cell::Cell;
use std::ptr;
use std::time::Duration;

use syrp_events::EventLoop;
use syrp_loop_api::{MessageLoop, Task};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, HWND_MESSAGE, PostMessageW, RegisterClassExW,
    WM_APP, WNDCLASSEXW,
};

const WM_PEER: u32 = WM_APP + 7;

thread_local! {
    static PEER_MESSAGES: Cell<u32> = const { Cell::new(0) };
}

unsafe extern "system" fn peer_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_PEER {
        PEER_MESSAGES.set(PEER_MESSAGES.get() + 1);
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

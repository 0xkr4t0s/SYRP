// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Tests for B1, B3 and B9 of work/specs/WP-1.1-14.md. B2 is in `conformance.rs`.

use std::cell::Cell;
use std::rc::Rc;
use std::thread;
use std::time::{Duration, Instant};

use syrp_error::Diagnostic;
use syrp_events::{EventLoop, EventsError, QuitHandle};
use syrp_loop_api::{MessageLoop, Task};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

// `check` resolves only when `T` lacks `Send`: otherwise both impls apply and `_` is ambiguous.
trait AmbiguousIfSend<A> {
    fn check() {}
}
impl<T: ?Sized> AmbiguousIfSend<()> for T {}
impl<T: ?Sized + Send> AmbiguousIfSend<u8> for T {}

fn is_send_sync<T: Send + Sync>() {}

#[test]
fn b1_event_loop_is_not_send_and_quit_handle_is_send_sync() {
    <EventLoop as AmbiguousIfSend<_>>::check();
    is_send_sync::<QuitHandle>();
}

#[test]
fn b1_second_loop_on_one_thread_is_a_coded_error() {
    let lp = EventLoop::new().unwrap();
    let err = EventLoop::new().err().unwrap();
    assert_eq!(err, EventsError::LoopAlreadyOnThread);
    assert_eq!(err.code().as_str(), "SYRP-LOOP-0004");
    // Another thread may have its own loop meanwhile.
    thread::spawn(|| assert!(EventLoop::new().is_ok()))
        .join()
        .unwrap();
    drop(lp);
    assert!(EventLoop::new().is_ok());
}

#[test]
fn b1_run_returns_when_quit_from_another_thread() {
    let lp = EventLoop::new().unwrap();
    let quit = lp.quit_handle();
    let t = thread::spawn(move || {
        thread::sleep(ms(20));
        quit.quit();
    });
    lp.run();
    t.join().unwrap();
    // The quit is consumed: run_for runs its full time afterwards.
    assert!(!lp.run_for(ms(5)));
}

#[test]
fn b1_run_for_returns_after_its_duration() {
    let lp = EventLoop::new().unwrap();
    let start = Instant::now();
    assert!(!lp.run_for(ms(30)));
    assert!(start.elapsed() >= ms(30));
}

#[test]
fn b1_dispatch_pending_never_blocks() {
    let lp = EventLoop::new().unwrap();
    let _t = lp.timer(Duration::from_secs(10), Box::new(|_| {}));
    let start = Instant::now();
    assert_eq!(lp.dispatch_pending(), 0);
    lp.post(Task::new(|_| {}));
    assert_eq!(lp.dispatch_pending(), 1);
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[test]
fn b1_with_token_runs_on_the_message_thread() {
    let lp = EventLoop::new().unwrap();
    let sender = lp.task_sender();
    assert!(lp.with_token(|_mt| sender.is_message_thread()));
}

#[test]
fn b3_ten_ms_timer_over_300_ms_fires_20_to_31_times() {
    let lp = EventLoop::new().unwrap();
    let count = Rc::new(Cell::new(0));
    let c = count.clone();
    let _t = lp.timer(ms(10), Box::new(move |_| c.set(c.get() + 1)));
    lp.run_for(ms(300));
    assert!((20..=31).contains(&count.get()), "fired {}", count.get());
}

#[test]
fn b3_blocked_timer_fires_once_not_a_burst_syrp_choice() {
    // `ManualLoop::advance` would fire this five times; a real loop fires it once.
    let lp = EventLoop::new().unwrap();
    let count = Rc::new(Cell::new(0));
    let c = count.clone();
    let _t = lp.timer(ms(10), Box::new(move |_| c.set(c.get() + 1)));
    thread::sleep(ms(55));
    assert_eq!(lp.dispatch_pending(), 1);
    assert_eq!(count.get(), 1);
}

#[test]
fn b9_errors_are_catalogued_in_errors_loop_toml() {
    let catalogue = include_str!("../../../../errors/loop.toml");
    // `Os` (SYRP-LOOP-0005) is non-exhaustive and needs a failing OS call to build.
    for (err, code) in [
        (EventsError::LoopAlreadyOnThread, "SYRP-LOOP-0004"),
        (EventsError::UnsupportedTarget, "SYRP-LOOP-0006"),
    ] {
        assert_eq!(err.code().as_str(), code);
        assert!(err.to_string().starts_with(code));
        assert!(err.hint().is_some());
    }
    for code in ["SYRP-LOOP-0004", "SYRP-LOOP-0005", "SYRP-LOOP-0006"] {
        assert!(catalogue.contains(&format!("code = \"{code}\"")));
    }
}

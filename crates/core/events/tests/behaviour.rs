// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Tests for B1, B3, B4 and B9 of work/specs/WP-1.1-14.md (B2 is in `conformance.rs`).

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
fn b4_a_posted_task_wakes_a_blocked_run() {
    let lp = EventLoop::new().unwrap();
    let (sender, quit) = (lp.task_sender(), lp.quit_handle());
    let t = thread::spawn(move || {
        thread::sleep(ms(20));
        sender.post(Task::new(move |_| quit.quit()));
    });
    lp.run(); // no timers: only the wake can end the wait
    t.join().unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn b4_add_fd_calls_back_while_readable_until_the_guard_drops() {
    use std::io::{Read, Write};
    use std::os::fd::AsFd;
    use std::os::unix::net::UnixStream;
    use syrp_events::Interest;

    let lp = EventLoop::new().unwrap();
    let (mut a, b) = UnixStream::pair().unwrap();
    let reader = Rc::new(b);
    let got = Rc::new(Cell::new(0));
    let (r, g) = (reader.clone(), got.clone());
    let guard = lp
        .add_fd(
            reader.as_fd(),
            Interest::READABLE,
            Box::new(move |_mt, ready| {
                assert!(ready.is_readable());
                let mut buf = [0u8; 8];
                g.set(g.get() + (&*r).read(&mut buf).unwrap());
            }),
        )
        .unwrap();
    assert_eq!(lp.dispatch_pending(), 0);
    a.write_all(b"abc").unwrap();
    assert!(!lp.run_for(ms(20)));
    assert_eq!(got.get(), 3);
    drop(guard);
    a.write_all(b"de").unwrap();
    assert_eq!(lp.dispatch_pending(), 0);
    assert_eq!(got.get(), 3);
}

#[cfg(target_os = "linux")]
#[test]
fn b4_add_fd_reports_hangup_and_its_guard_can_drop_itself() {
    use std::cell::RefCell;
    use std::os::fd::AsFd;
    use std::os::unix::net::UnixStream;
    use syrp_events::{FdGuard, Interest};

    let lp = EventLoop::new().unwrap();
    let (a, b) = UnixStream::pair().unwrap();
    let slot: Rc<RefCell<Option<FdGuard>>> = Rc::default();
    let (s, calls) = (slot.clone(), Rc::new(Cell::new(0)));
    let c = calls.clone();
    let f: syrp_events::FdFn = Box::new(move |_mt, r| {
        assert!(r.is_hangup() && r.is_readable());
        c.set(c.get() + 1);
        drop(s.borrow_mut().take()); // level-triggered: without this it would fire every pass
    });
    *slot.borrow_mut() = Some(lp.add_fd(b.as_fd(), Interest::BOTH, f).unwrap());
    drop(a);
    assert_eq!(lp.dispatch_pending(), 1);
    assert_eq!(lp.dispatch_pending(), 0);
    assert_eq!(calls.get(), 1);
}

#[cfg(target_os = "linux")]
#[test]
fn b4_add_fd_reports_an_epoll_refusal_with_the_os_error() {
    use std::os::fd::AsFd;
    let lp = EventLoop::new().unwrap();
    let file = std::fs::File::open(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).unwrap();
    let err = lp
        .add_fd(
            file.as_fd(),
            syrp_events::Interest::READABLE,
            Box::new(|_, _| {}),
        )
        .err()
        .unwrap();
    assert_eq!(err.code().as_str(), "SYRP-LOOP-0005");
    assert!(
        err.to_string()
            .starts_with("SYRP-LOOP-0005: epoll_ctl failed")
    );
    assert!(matches!(
        err,
        EventsError::Os {
            call: "epoll_ctl",
            os_error: Some(_),
            ..
        }
    ));
}

#[test]
fn b9_errors_are_catalogued_in_errors_loop_toml() {
    let catalogue = include_str!("../../../../errors/loop.toml");
    // `Os` (SYRP-LOOP-0005) cannot be built outside the crate; b4_add_fd_reports_... covers it.
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

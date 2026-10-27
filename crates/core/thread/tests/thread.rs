// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Behaviour tests for WP-1.1-06 increment 1 (B1 and the B6 error domain).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use syrp_thread::{
    DEFAULT_DROP_TIMEOUT, Diagnostic, Stopped, Thread, current_thread_id, current_thread_name,
    sleep, yield_now,
};

const LONG: Duration = Duration::from_secs(10);

#[test]
fn b1_spawned_thread_has_name_and_id() {
    let (tx, rx) = mpsc::channel();
    let t = Thread::builder("b1-named")
        .stack_size(512 * 1024)
        .spawn(move |_| {
            tx.send((current_thread_id(), current_thread_name()))
                .unwrap()
        })
        .unwrap();
    let (id, name) = rx.recv().unwrap();
    assert_eq!(id, t.id());
    assert_ne!(id, current_thread_id());
    assert_eq!(name.as_deref(), Some("b1-named"));
    assert_eq!(t.name(), Some("b1-named"));
    assert_eq!(t.stop(LONG), Ok(Stopped::Finished));
}

#[test]
fn b1_notify_wakes_wait_once() {
    let (tx, rx) = mpsc::channel();
    let (go, gate) = mpsc::channel::<()>();
    let t = Thread::builder("b1-notify")
        .spawn(move |ctx| {
            gate.recv().unwrap();
            tx.send(ctx.wait(None)).unwrap();
            // Two notifies before the first wait collapse into one wake-up.
            tx.send(ctx.wait(Some(Duration::from_millis(50)))).unwrap();
        })
        .unwrap();
    t.notify();
    t.notify();
    go.send(()).unwrap();
    assert!(rx.recv().unwrap());
    assert!(!rx.recv().unwrap());
    assert!(t.wait_for_thread_to_exit(Some(LONG)));
}

#[test]
fn b1_wait_times_out_with_false() {
    let t = Thread::builder("b1-timeout")
        .spawn(|ctx| {
            let start = Instant::now();
            assert!(!ctx.wait(Some(Duration::from_millis(30))));
            assert!(start.elapsed() >= Duration::from_millis(30));
        })
        .unwrap();
    // Wait without signalling, which would wake the body early.
    assert!(t.wait_for_thread_to_exit(Some(LONG)));
    assert_eq!(t.stop(LONG), Ok(Stopped::Finished));
}

#[test]
fn b1_exit_signal_sets_flag_and_wakes_wait() {
    let (tx, rx) = mpsc::channel();
    let t = Thread::builder("b1-exit")
        .spawn(move |ctx| {
            assert!(!ctx.thread_should_exit());
            tx.send(()).unwrap();
            assert!(ctx.wait(None));
            assert!(ctx.thread_should_exit());
            // Once signalled, every wait returns at once.
            assert!(ctx.wait(None));
        })
        .unwrap();
    rx.recv().unwrap();
    assert!(t.is_running());
    t.signal_thread_should_exit();
    assert!(t.wait_for_thread_to_exit(Some(LONG)));
    assert!(!t.is_running());
    assert_eq!(t.stop(LONG), Ok(Stopped::Finished));
}

#[test]
fn b1_stop_reports_panic() {
    let t = Thread::builder("b1-panic")
        .spawn(|_| panic!("deliberate test panic"))
        .unwrap();
    assert_eq!(t.stop(LONG), Ok(Stopped::Panicked));
}

#[test]
fn b1_stop_times_out_and_detaches() {
    let (release, gate) = mpsc::channel::<()>();
    let done = Arc::new(AtomicUsize::new(0));
    let d = Arc::clone(&done);
    let t = Thread::builder("b1-stubborn")
        .spawn(move |_| {
            let _ = gate.recv(); // ignores the exit signal
            d.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();
    let start = Instant::now();
    let err = t.stop(Duration::from_millis(50)).unwrap_err();
    assert!(start.elapsed() < Duration::from_secs(5));
    assert_eq!(err.code().as_str(), "SYRP-THREAD-0001");
    assert!(err.to_string().contains("detached"));
    // The detached thread is still alive and finishes once released.
    release.send(()).unwrap();
    let deadline = Instant::now() + LONG;
    while done.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
        sleep(Duration::from_millis(1));
    }
    assert_eq!(done.load(Ordering::SeqCst), 1);
}

#[test]
fn b1_drop_waits_then_detaches_a_body_that_ignores_the_signal() {
    assert_eq!(DEFAULT_DROP_TIMEOUT, Duration::from_secs(5));
    let (release, gate) = mpsc::channel::<()>();
    let t = Thread::builder("b1-drop")
        .drop_timeout(Duration::from_millis(50))
        .spawn(move |_| {
            let _ = gate.recv();
        })
        .unwrap();
    let start = Instant::now();
    drop(t);
    let took = start.elapsed();
    assert!(
        took >= Duration::from_millis(50),
        "drop returned after {took:?}"
    );
    assert!(took < Duration::from_secs(5), "drop hung for {took:?}");
    release.send(()).unwrap();
}

#[test]
fn b1_drop_joins_a_cooperative_body() {
    let done = Arc::new(AtomicUsize::new(0));
    let d = Arc::clone(&done);
    let t = Thread::builder("b1-drop-coop")
        .spawn(move |ctx| {
            while !ctx.thread_should_exit() {
                ctx.wait(None);
            }
            d.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();
    drop(t);
    assert_eq!(done.load(Ordering::SeqCst), 1);
}

#[test]
fn b1_launch_and_free_functions() {
    let (tx, rx) = mpsc::channel();
    Thread::launch(move || {
        yield_now();
        sleep(Duration::from_millis(1));
        tx.send(current_thread_id()).unwrap();
    })
    .unwrap();
    assert_ne!(rx.recv_timeout(LONG).unwrap(), current_thread_id());
}

#[test]
fn b6_every_code_is_catalogued() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../errors/thread.toml");
    let catalogue = std::fs::read_to_string(path).unwrap();
    for n in 1..=2 {
        assert!(catalogue.contains(&format!("code = \"SYRP-THREAD-{n:04}\"")));
    }
}

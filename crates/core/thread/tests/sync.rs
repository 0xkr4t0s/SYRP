// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Behaviour tests for WP-1.1-06 increment 3: WaitableEvent (B3) and SpinLock (B5), on real
//! threads. The interleaving models are in `tests/loom.rs`.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

use syrp_thread::{SpinLock, WaitableEvent};

const LONG: Duration = Duration::from_secs(10);

#[test]
fn b3_wait_times_out_with_false_after_the_timeout() {
    let e = WaitableEvent::new(false);
    let start = Instant::now();
    assert!(!e.wait(Some(Duration::from_millis(30))));
    assert!(start.elapsed() >= Duration::from_millis(30));
    assert!(!WaitableEvent::new(true).wait(Some(Duration::ZERO)));
}

#[test]
fn b3_signal_before_wait_is_not_lost() {
    for manual in [false, true] {
        let e = WaitableEvent::new(manual);
        e.signal();
        assert!(e.wait(Some(Duration::ZERO)), "manual={manual}");
    }
}

#[test]
fn b3_auto_reset_clears_after_one_wait_and_signals_do_not_count() {
    let e = WaitableEvent::default();
    e.signal();
    e.signal();
    assert!(e.wait(None));
    assert!(!e.wait(Some(Duration::from_millis(5))));
}

#[test]
fn b3_auto_reset_wakes_exactly_one_of_several_waiters() {
    let e = Arc::new(WaitableEvent::new(false));
    let woken = Arc::new(AtomicUsize::new(0));
    let start = Arc::new(Barrier::new(5));
    let waiters: Vec<_> = (0..4)
        .map(|_| {
            let (e, woken, start) = (Arc::clone(&e), Arc::clone(&woken), Arc::clone(&start));
            std::thread::spawn(move || {
                start.wait();
                if e.wait(Some(Duration::from_millis(300))) {
                    woken.fetch_add(1, Ordering::SeqCst);
                }
            })
        })
        .collect();
    start.wait();
    std::thread::sleep(Duration::from_millis(20));
    e.signal();
    for w in waiters {
        w.join().unwrap();
    }
    assert_eq!(woken.load(Ordering::SeqCst), 1);
    assert!(!e.wait(Some(Duration::ZERO)));
}

#[test]
fn b3_manual_reset_wakes_every_waiter_and_stays_signalled_until_reset() {
    let e = Arc::new(WaitableEvent::new(true));
    let waiters: Vec<_> = (0..4)
        .map(|_| {
            let e = Arc::clone(&e);
            std::thread::spawn(move || e.wait(Some(LONG)))
        })
        .collect();
    std::thread::sleep(Duration::from_millis(20));
    e.signal();
    for w in waiters {
        assert!(w.join().unwrap());
    }
    assert!(e.wait(Some(Duration::ZERO)));
    e.reset();
    assert!(!e.wait(Some(Duration::from_millis(5))));
}

#[test]
fn b3_signal_wakes_a_blocked_waiter_promptly() {
    let e = Arc::new(WaitableEvent::new(false));
    let e2 = Arc::clone(&e);
    let t = std::thread::spawn(move || {
        let start = Instant::now();
        (e2.wait(None), start.elapsed())
    });
    std::thread::sleep(Duration::from_millis(20));
    e.signal();
    let (ok, waited) = t.join().unwrap();
    assert!(ok);
    assert!(waited < LONG);
}

#[test]
fn b5_try_lock_fails_while_held_and_succeeds_after_release() {
    let lock = SpinLock::new(1);
    let mut held = lock.lock();
    *held += 1;
    assert!(lock.try_lock().is_none());
    drop(held);
    assert_eq!(lock.try_lock().map(|g| *g), Some(2));
    assert_eq!(lock.into_inner(), 2);
}

#[test]
fn b5_lock_gives_mutual_exclusion_across_threads() {
    let lock = Arc::new(SpinLock::new((0u64, 0u64)));
    let threads: Vec<_> = (0..4)
        .map(|_| {
            let lock = Arc::clone(&lock);
            std::thread::spawn(move || {
                for _ in 0..10_000 {
                    let mut g = lock.lock();
                    // Two separate writes: a torn update would leave them unequal.
                    g.0 += 1;
                    std::hint::black_box(&mut *g);
                    g.1 += 1;
                }
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
    assert_eq!(*lock.lock(), (40_000, 40_000));
}

#[test]
fn b5_lock_waits_for_a_holder_that_sleeps() {
    let lock = Arc::new(SpinLock::new(()));
    let held = Arc::new(Barrier::new(2));
    let (l, h) = (Arc::clone(&lock), Arc::clone(&held));
    let holder = std::thread::spawn(move || {
        let _g = l.lock();
        h.wait();
        std::thread::sleep(Duration::from_millis(30));
    });
    held.wait();
    let start = Instant::now();
    drop(lock.lock());
    assert!(start.elapsed() >= Duration::from_millis(10));
    holder.join().unwrap();
}

#[test]
fn b5_guard_releases_on_unwind() {
    let lock = Arc::new(SpinLock::new(()));
    let l = Arc::clone(&lock);
    let r = std::thread::spawn(move || {
        let _g = l.lock();
        panic!("holder panics");
    })
    .join();
    assert!(r.is_err());
    assert!(lock.try_lock().is_some());
}

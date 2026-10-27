// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! loom models of WaitableEvent signal, wait and reset interleavings (B3) and SpinLock
//! mutual exclusion (B5). Waits in these models either cannot block (`Some(ZERO)`) or are
//! guaranteed a signal, because loom's condvar never times out. Run with:
//! `RUSTFLAGS="--cfg loom" CARGO_TARGET_DIR=target/loom cargo test -p syrp-thread --release --test loom`
#![cfg(loom)]

use std::time::Duration;

use loom::cell::UnsafeCell;
use loom::sync::Arc;
use loom::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use loom::thread;
use syrp_thread::{SpinLock, WaitableEvent};

const NOW: Option<Duration> = Some(Duration::ZERO);

#[test]
fn b3_loom_signal_from_another_thread_is_never_lost() {
    for manual in [false, true] {
        loom::model(move || {
            let e = Arc::new(WaitableEvent::new(manual));
            let e2 = Arc::clone(&e);
            let signaller = thread::spawn(move || e2.signal());
            assert!(e.wait(None));
            signaller.join().unwrap();
            assert_eq!(e.wait(NOW), manual, "auto-reset must have cleared");
        });
    }
}

#[test]
fn b3_loom_auto_reset_lets_exactly_one_waiter_through() {
    loom::model(|| {
        let e = Arc::new(WaitableEvent::new(false));
        let through = Arc::new(AtomicUsize::new(0));
        e.signal();
        let racers: Vec<_> = (0..2)
            .map(|_| {
                let (e, through) = (Arc::clone(&e), Arc::clone(&through));
                thread::spawn(move || {
                    if e.wait(NOW) {
                        through.fetch_add(1, SeqCst);
                    }
                })
            })
            .collect();
        for r in racers {
            r.join().unwrap();
        }
        assert_eq!(through.load(SeqCst), 1);
        assert!(!e.wait(NOW));
    });
}

#[test]
fn b3_loom_manual_reset_wakes_two_blocked_waiters() {
    loom::model(|| {
        let e = Arc::new(WaitableEvent::new(true));
        let waiters: Vec<_> = (0..2)
            .map(|_| {
                let e = Arc::clone(&e);
                thread::spawn(move || e.wait(None))
            })
            .collect();
        e.signal();
        for w in waiters {
            assert!(w.join().unwrap());
        }
        e.reset();
        assert!(!e.wait(NOW));
    });
}

#[test]
fn b3_loom_reset_racing_signal_leaves_a_consistent_state() {
    loom::model(|| {
        let e = Arc::new(WaitableEvent::new(true));
        let e2 = Arc::clone(&e);
        let signaller = thread::spawn(move || e2.signal());
        e.reset();
        signaller.join().unwrap();
        // Whichever of the two ran last decides the state; nothing deadlocks or panics, and
        // the event still obeys later calls.
        let _either = e.wait(NOW);
        e.reset();
        assert!(!e.wait(NOW));
        e.signal();
        assert!(e.wait(NOW));
    });
}

#[test]
fn b5_loom_spinlock_gives_mutual_exclusion() {
    loom::model(|| {
        let lock = Arc::new(SpinLock::new(()));
        let data = Arc::new(UnsafeCell::new(0usize));
        let workers: Vec<_> = (0..2)
            .map(|_| {
                let (lock, data) = (Arc::clone(&lock), Arc::clone(&data));
                thread::spawn(move || {
                    let _g = lock.lock();
                    // loom panics if these accesses are not ordered by the lock.
                    // SAFETY: the pointer is live (the Arc keeps the cell) and the lock is held.
                    data.with_mut(|p| unsafe { *p += 1 });
                })
            })
            .collect();
        for w in workers {
            w.join().unwrap();
        }
        // SAFETY: both workers have been joined, so nothing else touches the cell.
        assert_eq!(data.with(|p| unsafe { *p }), 2);
    });
}

#[test]
fn b5_loom_spinlock_guards_its_own_value_and_try_lock_never_blocks() {
    loom::model(|| {
        let lock = Arc::new(SpinLock::new(0usize));
        let l2 = Arc::clone(&lock);
        let other = thread::spawn(move || {
            if let Some(mut g) = l2.try_lock() {
                *g += 1;
            }
        });
        *lock.lock() += 10;
        other.join().unwrap();
        let v = *lock.lock();
        assert!(v == 10 || v == 11, "{v}");
    });
}

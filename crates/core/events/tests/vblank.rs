// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B7 tests of work/specs/WP-1.1-14.md: the 60 Hz VBlank fallback and `VBlankDriver`, with
//! the test as a fake display driver.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use syrp_events::EventLoop;
use syrp_loop_api::{MessageLoop, PeerId, VBlankGuard};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// Registers a VBlank callback on `peer` that records each refresh time.
fn record(lp: &EventLoop, peer: PeerId) -> (Rc<RefCell<Vec<Duration>>>, VBlankGuard) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let l = log.clone();
    let guard = lp.on_vblank(peer, Box::new(move |_mt, at| l.borrow_mut().push(at)));
    (log, guard)
}

#[test]
fn b7_fallback_runs_at_60_hz_with_times_on_the_loop_clock() {
    let lp = EventLoop::new().unwrap();
    let (log, _g) = record(&lp, PeerId::from_raw(1));
    lp.run_for(ms(300));
    let log = log.borrow();
    // 300 ms at 60 Hz is 18 refreshes; loose bounds for CI.
    assert!((10..=19).contains(&log.len()), "fired {}", log.len());
    assert!(log.windows(2).all(|w| w[1] > w[0]));
    assert!(log.iter().all(|&at| at <= lp.now()));
    // Refresh times are the scheduled deadlines: whole periods after the first.
    let period = log[1] - log[0];
    assert!(
        period >= Duration::from_nanos(16_666_667),
        "period {period:?}"
    );
}

#[test]
fn b7_a_driver_stops_its_peers_fallback_and_fires_with_its_times() {
    let lp = EventLoop::new().unwrap();
    let (a, b) = (PeerId::from_raw(1), PeerId::from_raw(2));
    let (log_a, _ga) = record(&lp, a);
    let (log_b, _gb) = record(&lp, b);
    let driver = lp.vblank_driver(a);
    assert_eq!(driver.peer(), a);
    lp.run_for(ms(100));
    assert!(log_a.borrow().is_empty());
    assert!(
        !log_b.borrow().is_empty(),
        "other peers keep their fallback"
    );
    assert_eq!(driver.fire(ms(4242)), 1);
    assert_eq!(*log_a.borrow(), [ms(4242)]);
    // A callback registered while the peer is driven waits for the driver too.
    let (late, _gl) = record(&lp, a);
    lp.run_for(ms(50));
    assert!(late.borrow().is_empty());
    assert_eq!(driver.fire(ms(5000)), 2);
    assert_eq!(*late.borrow(), [ms(5000)]);
}

#[test]
fn b7_the_fallback_resumes_after_the_last_driver_drops() {
    let lp = EventLoop::new().unwrap();
    let peer = PeerId::from_raw(9);
    let (log, _g) = record(&lp, peer);
    let (d1, d2) = (lp.vblank_driver(peer), lp.vblank_driver(peer));
    drop(d1);
    lp.run_for(ms(60));
    assert!(log.borrow().is_empty(), "one driver is still alive");
    drop(d2);
    lp.run_for(ms(100));
    assert!(log.borrow().len() >= 2, "fired {}", log.borrow().len());
}

#[test]
fn b7_a_callback_may_drop_its_own_guard_during_fire() {
    let lp = EventLoop::new().unwrap();
    let peer = PeerId::from_raw(1);
    let slot: Rc<RefCell<Option<VBlankGuard>>> = Rc::default();
    let (s, calls) = (slot.clone(), Rc::new(Cell::new(0)));
    let c = calls.clone();
    *slot.borrow_mut() = Some(lp.on_vblank(
        peer,
        Box::new(move |_mt, _at| {
            c.set(c.get() + 1);
            drop(s.borrow_mut().take());
        }),
    ));
    let driver = lp.vblank_driver(peer);
    assert_eq!(driver.fire(ms(1)), 1);
    assert_eq!(driver.fire(ms(2)), 0);
    assert_eq!(calls.get(), 1);
}

#[test]
fn b7_a_driver_outliving_its_loop_does_nothing() {
    let lp = EventLoop::new().unwrap();
    let driver = lp.vblank_driver(PeerId::from_raw(1));
    drop(lp);
    assert_eq!(driver.fire(ms(1)), 0);
}

#[test]
fn b7_a_driver_made_and_dropped_in_the_fallback_callback_keeps_the_rate() {
    // Regression: dropping the driver re-scheduled the callback while it was firing, and the
    // fallback then scheduled it again, doubling the rate for good.
    let lp = Rc::new(EventLoop::new().unwrap());
    let peer = PeerId::from_raw(1);
    let calls = Rc::new(Cell::new(0u32));
    let (c, weak) = (calls.clone(), Rc::downgrade(&lp));
    let _g = lp.on_vblank(
        peer,
        Box::new(move |_mt, _at| {
            c.set(c.get() + 1);
            if c.get() == 1 {
                let lp = weak.upgrade().unwrap();
                drop(lp.vblank_driver(peer));
            }
        }),
    );
    lp.run_for(ms(500));
    // 500 ms at 60 Hz is 30 refreshes; a doubled rate gives about 58.
    assert!((10..=40).contains(&calls.get()), "fired {}", calls.get());
}

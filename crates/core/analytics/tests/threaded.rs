// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B3–B5 on a real analytics thread. Tests wait on a condition variable for what the sink
//! observed, with a loose 60 s bound that only fails a hung test; nothing sleeps and no test
//! asserts on elapsed time.

use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use syrp_analytics::{
    Analytics, AnalyticsDestination, AnalyticsError, AnalyticsEvent, BatchSink, Diagnostic,
    ThreadedAnalyticsDestination,
};

const LOOSE: Duration = Duration::from_secs(60);
const FAST: Duration = Duration::from_millis(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Mode {
    /// Return `succeed` at once.
    #[default]
    Normal,
    /// Block until `stop_logging_events`, then fail.
    BlockUntilStop,
    /// Block until the test sets `released`, then succeed (ignores stop requests).
    BlockUntilReleased,
}

#[derive(Debug, Default)]
struct State {
    succeed: bool,
    max: usize,
    mode: Mode,
    attempts: usize,
    delivered: Vec<String>,
    batch_sizes: Vec<usize>,
    saved: Vec<AnalyticsEvent>,
    stop_calls: usize,
    stopping: bool,
    blocked: bool,
    released: bool,
    late_returns: usize,
}

#[derive(Debug, Default)]
struct Probe {
    st: Mutex<State>,
    cv: Condvar,
}

impl Probe {
    fn new(succeed: bool, max: usize, mode: Mode) -> Arc<Self> {
        let probe = Arc::new(Self::default());
        {
            let mut st = probe.lock();
            (st.succeed, st.max, st.mode) = (succeed, max, mode);
        }
        probe
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.st.lock().unwrap()
    }

    fn update(&self, f: impl FnOnce(&mut State)) {
        f(&mut self.lock());
        self.cv.notify_all();
    }

    /// Waits until `done` holds, failing the test only if that takes longer than `LOOSE`.
    fn wait_for(&self, what: &str, done: impl Fn(&State) -> bool) {
        let deadline = Instant::now() + LOOSE;
        let mut st = self.lock();
        while !done(&st) {
            let left = deadline.saturating_duration_since(Instant::now());
            assert!(!left.is_zero(), "timed out waiting for {what}: {st:?}");
            st = self.cv.wait_timeout(st, left).unwrap().0;
        }
    }
}

struct Sink(Arc<Probe>);

impl BatchSink for Sink {
    fn log_batched_events(&self, events: &[AnalyticsEvent]) -> bool {
        let p = &self.0;
        let mut st = p.lock();
        st.attempts += 1;
        st.batch_sizes.push(events.len());
        let ok = match st.mode {
            Mode::Normal => st.succeed,
            Mode::BlockUntilStop => {
                st.blocked = true;
                p.cv.notify_all();
                while !st.stopping {
                    st = p.cv.wait(st).unwrap();
                }
                false
            }
            Mode::BlockUntilReleased => {
                st.blocked = true;
                p.cv.notify_all();
                while !st.released {
                    st = p.cv.wait(st).unwrap();
                }
                st.late_returns += 1;
                true
            }
        };
        if ok {
            st.delivered
                .extend(events.iter().map(|e| e.name().to_owned()));
        }
        drop(st);
        p.cv.notify_all();
        ok
    }

    fn maximum_batch_size(&self) -> usize {
        self.0.lock().max
    }

    fn stop_logging_events(&self) {
        self.0.update(|st| {
            st.stop_calls += 1;
            st.stopping = true;
        });
    }

    fn save_unlogged_events(&self, events: &[AnalyticsEvent]) {
        self.0.update(|st| st.saved.extend_from_slice(events));
    }

    fn restore_unlogged_events(&self, events: &mut Vec<AnalyticsEvent>) {
        events.append(&mut self.0.lock().saved);
    }
}

fn log(d: &impl AnalyticsDestination, range: std::ops::Range<usize>) {
    for i in range {
        d.log_event(&AnalyticsEvent::new(format!("e{i}"), 0, 0));
    }
}

fn names(range: std::ops::Range<usize>) -> Vec<String> {
    range.map(|i| format!("e{i}")).collect()
}

fn saved_names(p: &Probe) -> Vec<String> {
    p.lock().saved.iter().map(|e| e.name().to_owned()).collect()
}

#[test]
fn b3_the_thread_sends_queued_events_in_order_in_bounded_batches() {
    let p = Probe::new(true, 2, Mode::Normal);
    let d = ThreadedAnalyticsDestination::new("analytics-test", Sink(p.clone()));
    log(&d, 0..5);
    d.start_analytics_thread(FAST).unwrap();
    p.wait_for("first five", |st| st.delivered.len() == 5);
    log(&d, 5..8);
    p.wait_for("all eight", |st| st.delivered.len() == 8);
    let report = d.stop_analytics_thread(LOOSE);
    let st = p.lock();
    assert_eq!(st.delivered, names(0..8));
    assert!(st.batch_sizes.iter().all(|&n| (1..=2).contains(&n)));
    assert_eq!((report.saved(), report.timed_out()), (0, false));
    assert_eq!(st.stop_calls, 1);
}

#[test]
fn b3_stop_calls_stop_logging_events_then_saves_every_unsent_event_and_restart_restores() {
    let p = Probe::new(false, 10, Mode::Normal);
    let d = ThreadedAnalyticsDestination::new("analytics-test", Sink(p.clone()));
    log(&d, 0..3);
    d.start_analytics_thread(FAST).unwrap();
    p.wait_for("a failed attempt", |st| st.attempts >= 1);
    log(&d, 3..5);
    let report = d.stop_analytics_thread(LOOSE);
    assert_eq!(
        (report.saved(), report.dropped(), report.timed_out()),
        (5, 0, false)
    );
    assert_eq!(saved_names(&p), names(0..5));
    assert_eq!(p.lock().stop_calls, 1);
    assert_eq!(d.queued_event_count(), 0);

    p.update(|st| {
        st.succeed = true;
        st.stopping = false;
    });
    log(&d, 5..6);
    d.start_analytics_thread(FAST).unwrap();
    p.wait_for("restored and new events", |st| st.delivered.len() == 6);
    assert_eq!(p.lock().delivered, names(0..6));
    assert_eq!(d.stop_analytics_thread(LOOSE).saved(), 0);
}

#[test]
fn b3_stop_logging_events_ends_a_batch_in_progress() {
    let p = Probe::new(false, 10, Mode::BlockUntilStop);
    let d = ThreadedAnalyticsDestination::new("analytics-test", Sink(p.clone()));
    log(&d, 0..4);
    d.start_analytics_thread(FAST).unwrap();
    p.wait_for("the sink to block", |st| st.blocked);
    let report = d.stop_analytics_thread(LOOSE);
    assert!(!report.timed_out());
    assert_eq!(saved_names(&p), names(0..4));
}

#[test]
fn b3_a_timed_out_stop_saves_everything_and_the_late_batch_cannot_touch_the_queue() {
    let p = Probe::new(true, 10, Mode::BlockUntilReleased);
    let d = ThreadedAnalyticsDestination::new("analytics-test", Sink(p.clone()));
    log(&d, 0..3);
    d.start_analytics_thread(FAST).unwrap();
    p.wait_for("the sink to block", |st| st.blocked);
    // The sink ignores stop requests, so any timeout runs out.
    let report = d.stop_analytics_thread(Duration::from_millis(1));
    assert!(report.timed_out());
    assert_eq!(saved_names(&p), names(0..3));
    let err = d.start_analytics_thread(FAST).unwrap_err();
    assert_eq!(err, AnalyticsError::AlreadyRunning);

    log(&d, 3..4);
    p.update(|st| st.released = true);
    p.wait_for("the late return", |st| st.late_returns == 1);
    // Once that thread exits a new start succeeds; spin (yielding) instead of sleeping.
    let deadline = Instant::now() + LOOSE;
    while d
        .start_analytics_thread(Duration::from_secs(3_600))
        .is_err()
    {
        assert!(Instant::now() < deadline, "old thread never exited");
        std::thread::yield_now();
    }
    // The late success removed nothing: the restored batch is back ahead of e3.
    assert_eq!(d.queued_event_count(), 4);
    assert_eq!(d.stop_analytics_thread(LOOSE).saved(), 4);
    assert_eq!(saved_names(&p), names(0..4));
}

#[test]
fn b3_drop_stops_the_thread_and_saves_unsent_events() {
    let p = Probe::new(false, 10, Mode::Normal);
    let d = ThreadedAnalyticsDestination::new("analytics-test", Sink(p.clone()));
    log(&d, 0..3);
    d.start_analytics_thread(FAST).unwrap();
    p.wait_for("a failed attempt", |st| st.attempts >= 1);
    drop(d);
    assert_eq!(saved_names(&p), names(0..3));
    assert_eq!(p.lock().stop_calls, 1);
}

#[test]
fn b3_stop_reports_events_dropped_by_a_full_queue() {
    let p = Probe::new(true, 10, Mode::Normal);
    let d = ThreadedAnalyticsDestination::with_queue_capacity("analytics-test", Sink(p.clone()), 2);
    log(&d, 0..5);
    assert_eq!(d.dropped_event_count(), 3);
    let report = d.stop_analytics_thread(LOOSE);
    assert_eq!((report.saved(), report.dropped()), (2, 3));
    assert_eq!(saved_names(&p), names(0..2));
    assert_eq!(p.lock().stop_calls, 0, "no thread was running");
    assert_eq!(d.dropped_event_count(), 0);
}

#[test]
fn b3_set_batch_period_wakes_a_waiting_thread() {
    let p = Probe::new(true, 10, Mode::Normal);
    let d = ThreadedAnalyticsDestination::new("analytics-test", Sink(p.clone()));
    log(&d, 0..1);
    d.start_analytics_thread(Duration::from_secs(3_600))
        .unwrap();
    d.set_batch_period(FAST).unwrap();
    p.wait_for("the event", |st| st.delivered.len() == 1);
}

#[test]
fn b3_a_threaded_destination_works_inside_analytics() {
    let p = Probe::new(true, 10, Mode::Normal);
    let d = ThreadedAnalyticsDestination::new("analytics-test", Sink(p.clone()));
    d.start_analytics_thread(FAST).unwrap();
    let analytics = Analytics::new();
    analytics.add_destination(Box::new(d));
    analytics.set_user_id("u");
    analytics.log_event("hello", [("k", "v")], 3);
    p.wait_for("the event", |st| st.delivered.len() == 1);
    // Removing and dropping the destination shuts its thread down.
    drop(analytics.destinations().remove(0));
    assert_eq!(p.lock().stop_calls, 1);
}

#[test]
fn b5_starting_a_running_thread_is_an_error_with_a_code() {
    let p = Probe::new(true, 10, Mode::Normal);
    let d = ThreadedAnalyticsDestination::new("analytics-test", Sink(p));
    d.start_analytics_thread(Duration::from_secs(3_600))
        .unwrap();
    let err = d.start_analytics_thread(FAST).unwrap_err();
    assert_eq!(err.code().as_str(), "SYRP-ANALYTICS-0002");
    assert!(!d.stop_analytics_thread(LOOSE).timed_out());
    d.start_analytics_thread(FAST).unwrap();
}

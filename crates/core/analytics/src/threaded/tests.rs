// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B3–B4: the batching, retry and back-off rules, driven through the step function with a
//! manual clock (no thread, no sleeping).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use super::BatchSink;
use super::core::{Batcher, MAX_BACKOFF_PERIOD_MS, Shared, Step, step};
use crate::event::AnalyticsEvent;

/// A sink whose results follow a script (then succeed), recording every call.
#[derive(Default)]
pub(super) struct FakeSink {
    max: usize,
    script: Mutex<VecDeque<bool>>,
    calls: Mutex<Vec<Vec<String>>>,
    delivered: Mutex<Vec<String>>,
    saved: Mutex<Vec<AnalyticsEvent>>,
    /// Run inside `log_batched_events`, to act as another thread would mid-batch.
    during_send: Mutex<Option<Box<dyn FnMut() + Send>>>,
}

impl FakeSink {
    fn new(max: usize) -> Self {
        Self {
            max,
            ..Self::default()
        }
    }

    fn script(self, results: &[bool]) -> Self {
        *self.script.lock().unwrap() = results.iter().copied().collect();
        self
    }

    fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().clone()
    }
}

impl BatchSink for FakeSink {
    fn log_batched_events(&self, events: &[AnalyticsEvent]) -> bool {
        let names: Vec<String> = events.iter().map(|e| e.name().to_owned()).collect();
        self.calls.lock().unwrap().push(names.clone());
        if let Some(f) = self.during_send.lock().unwrap().as_mut() {
            f();
        }
        let ok = self.script.lock().unwrap().pop_front().unwrap_or(true);
        if ok {
            self.delivered.lock().unwrap().extend(names);
        }
        ok
    }

    fn maximum_batch_size(&self) -> usize {
        self.max
    }

    fn stop_logging_events(&self) {}

    fn save_unlogged_events(&self, events: &[AnalyticsEvent]) {
        self.saved.lock().unwrap().extend_from_slice(events);
    }

    fn restore_unlogged_events(&self, events: &mut Vec<AnalyticsEvent>) {
        events.append(&mut self.saved.lock().unwrap());
    }
}

fn ev(name: &str) -> AnalyticsEvent {
    AnalyticsEvent::new(name, 0, 0)
}

fn log(shared: &Shared, names: &[&str]) {
    for n in names {
        shared.push(&ev(n));
    }
}

/// Begins a generation at time `now` with base period `base`.
fn begin(shared: &Shared, sink: &FakeSink, base: u64, now: u64) -> (u64, Batcher) {
    let generation = shared.begin(sink, base);
    let epoch = shared.lock().period_epoch;
    (generation, Batcher::new(base, epoch, now))
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn b3_first_batch_is_due_one_period_after_start() {
    let shared = Shared::new(100);
    let sink = FakeSink::new(10);
    log(&shared, &["a", "b"]);
    let (g, mut b) = begin(&shared, &sink, 100, 0);
    assert_eq!(step(&shared, &sink, &mut b, g, 0), Step::WaitUntil(100));
    assert_eq!(step(&shared, &sink, &mut b, g, 99), Step::WaitUntil(100));
    assert!(sink.calls().is_empty());
    assert_eq!(step(&shared, &sink, &mut b, g, 100), Step::WaitUntil(200));
    assert_eq!(sink.calls(), [names(&["a", "b"])]);
    assert!(shared.lock().queue.is_empty());
}

#[test]
fn b3_batches_hold_at_most_the_maximum_size_oldest_first() {
    let shared = Shared::new(100);
    let sink = FakeSink::new(2);
    log(&shared, &["0", "1", "2", "3", "4"]);
    let (g, mut b) = begin(&shared, &sink, 10, 0);
    for t in [10, 20, 30, 40] {
        step(&shared, &sink, &mut b, g, t);
    }
    assert_eq!(
        sink.calls(),
        [names(&["0", "1"]), names(&["2", "3"]), names(&["4"])]
    );
}

#[test]
fn b3_empty_queue_waits_another_period_without_calling_the_sink() {
    let shared = Shared::new(100);
    let sink = FakeSink::new(5);
    let (g, mut b) = begin(&shared, &sink, 50, 0);
    assert_eq!(step(&shared, &sink, &mut b, g, 50), Step::WaitUntil(100));
    assert!(sink.calls().is_empty());
}

#[test]
fn b3_failed_batch_is_retried_with_the_same_events_then_new_ones() {
    let shared = Shared::new(100);
    let sink = FakeSink::new(3).script(&[false]);
    log(&shared, &["a", "b"]);
    let (g, mut b) = begin(&shared, &sink, 100, 0);
    assert_eq!(step(&shared, &sink, &mut b, g, 100), Step::WaitUntil(300));
    log(&shared, &["c", "d"]);
    step(&shared, &sink, &mut b, g, 300);
    assert_eq!(sink.calls(), [names(&["a", "b"]), names(&["a", "b", "c"])]);
    assert_eq!(*sink.delivered.lock().unwrap(), names(&["a", "b", "c"]));
    assert_eq!(shared.lock().queue.len(), 1);
}

#[test]
fn b3_period_doubles_after_failures_up_to_a_cap_and_resets_after_success_syrp_choice() {
    let shared = Shared::new(100);
    let sink = FakeSink::new(1).script(&[false; 8]);
    log(&shared, &["a"]);
    let (g, mut b) = begin(&shared, &sink, 1_000, 0);
    let mut now = 1_000;
    let mut periods = Vec::new();
    for _ in 0..9 {
        let Step::WaitUntil(due) = step(&shared, &sink, &mut b, g, now) else {
            panic!("stopped")
        };
        periods.push(due - now);
        now = due;
    }
    assert_eq!(
        periods,
        [
            2_000, 4_000, 8_000, 16_000, 32_000, 60_000, 60_000, 60_000, 1_000
        ]
    );
    assert_eq!(MAX_BACKOFF_PERIOD_MS, 60_000);
}

#[test]
fn b3_base_period_above_the_cap_is_never_shortened_syrp_choice() {
    let shared = Shared::new(10);
    let sink = FakeSink::new(1).script(&[false, false]);
    log(&shared, &["a"]);
    let (g, mut b) = begin(&shared, &sink, 90_000, 0);
    step(&shared, &sink, &mut b, g, 90_000);
    assert_eq!(b.period_ms(), 90_000);
}

#[test]
fn b3_zero_maximum_batch_size_sends_one_event_syrp_choice() {
    let shared = Shared::new(10);
    let sink = FakeSink::new(0);
    log(&shared, &["a", "b"]);
    let (g, mut b) = begin(&shared, &sink, 1, 0);
    step(&shared, &sink, &mut b, g, 1);
    assert_eq!(sink.calls(), [names(&["a"])]);
}

#[test]
fn b3_full_queue_drops_new_events_and_counts_them_syrp_choice() {
    let shared = Shared::new(2);
    log(&shared, &["a", "b", "c", "d"]);
    let (events, dropped) = shared.take_unlogged();
    let kept: Vec<_> = events.iter().map(|e| e.name().to_owned()).collect();
    assert_eq!((kept, dropped), (names(&["a", "b"]), 2));
    assert_eq!(shared.lock().dropped, 0);
}

#[test]
fn b3_restored_events_go_ahead_of_queued_ones() {
    let shared = Shared::new(10);
    let sink = FakeSink::new(10);
    sink.save_unlogged_events(&[ev("old1"), ev("old2")]);
    log(&shared, &["new"]);
    let (g, mut b) = begin(&shared, &sink, 5, 0);
    step(&shared, &sink, &mut b, g, 5);
    assert_eq!(sink.calls(), [names(&["old1", "old2", "new"])]);
    assert!(sink.saved.lock().unwrap().is_empty());
}

#[test]
fn b3_a_changed_base_period_restarts_the_schedule() {
    let shared = Shared::new(10);
    let sink = FakeSink::new(10).script(&[false]);
    log(&shared, &["a"]);
    let (g, mut b) = begin(&shared, &sink, 100, 0);
    step(&shared, &sink, &mut b, g, 100);
    assert_eq!(b.period_ms(), 200);
    {
        let mut st = shared.lock();
        st.base_period_ms = 7;
        st.period_epoch += 1;
    }
    assert_eq!(step(&shared, &sink, &mut b, g, 150), Step::WaitUntil(157));
    assert_eq!(b.period_ms(), 7);
}

#[test]
fn b3_a_stop_during_a_successful_batch_still_removes_it() {
    let shared = Arc::new(Shared::new(10));
    let sink = FakeSink::new(10);
    let s2 = Arc::clone(&shared);
    *sink.during_send.lock().unwrap() = Some(Box::new(move || s2.request_stop()));
    log(&shared, &["a", "b"]);
    let (g, mut b) = begin(&shared, &sink, 10, 0);
    assert_eq!(step(&shared, &sink, &mut b, g, 10), Step::Stop);
    assert!(shared.take_unlogged().0.is_empty());
    assert_eq!(step(&shared, &sink, &mut b, g, 20), Step::Stop);
}

#[test]
fn b5_too_short_periods_are_rejected_with_a_code() {
    use crate::{AnalyticsError, Diagnostic, ThreadedAnalyticsDestination};
    use std::time::Duration;
    let d = ThreadedAnalyticsDestination::new("t", FakeSink::new(1));
    for period in [Duration::ZERO, Duration::from_micros(999)] {
        let err = d.start_analytics_thread(period).unwrap_err();
        assert_eq!(err, AnalyticsError::BatchPeriodTooShort { period });
        assert_eq!(err.code().as_str(), "SYRP-ANALYTICS-0003");
        assert!(d.set_batch_period(period).is_err());
    }
}

#[test]
fn b5_a_thread_name_with_nul_is_rejected_with_a_code() {
    use crate::{AnalyticsError, Diagnostic, ThreadedAnalyticsDestination};
    use std::time::Duration;
    let d = ThreadedAnalyticsDestination::new("bad\0name", FakeSink::new(1));
    let err = d
        .start_analytics_thread(Duration::from_secs(1))
        .unwrap_err();
    assert_eq!(err, AnalyticsError::InvalidThreadName);
    assert_eq!(err.code().as_str(), "SYRP-ANALYTICS-0004");
}

#[test]
fn b5_every_code_is_in_the_catalogue() {
    use crate::{AnalyticsError, Diagnostic};
    let catalogue = include_str!("../../../../../errors/analytics.toml");
    let codes: Vec<&str> = catalogue
        .lines()
        .filter_map(|l| l.strip_prefix("code = \""))
        .map(|l| l.trim_end_matches('"'))
        .collect();
    let errors = [
        AnalyticsError::ThreadSpawn {
            reason: String::new(),
        },
        AnalyticsError::AlreadyRunning,
        AnalyticsError::BatchPeriodTooShort {
            period: std::time::Duration::ZERO,
        },
        AnalyticsError::InvalidThreadName,
    ];
    for e in &errors {
        assert!(codes.contains(&e.code().as_str()), "{} missing", e.code());
        assert!(e.to_string().starts_with(e.code().as_str()));
        assert!(e.hint().is_some());
    }
    assert_eq!(codes.len(), errors.len());
}

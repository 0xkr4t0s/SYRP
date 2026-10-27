// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B3–B4 robustness on the manual clock: sink panics, races with a stop, and a proptest that
//! no event is lost or duplicated across failures, stops and restarts.

use std::sync::{Arc, Mutex};

use proptest::prelude::*;

use super::{FakeSink, begin, ev, log};
use crate::event::AnalyticsEvent;
use crate::threaded::BatchSink;
use crate::threaded::core::{Batcher, Shared, Step, step};

#[test]
fn b3_a_panicking_sink_counts_as_a_failure() {
    struct Panics;
    impl BatchSink for Panics {
        fn log_batched_events(&self, _: &[AnalyticsEvent]) -> bool {
            panic!("sink failure")
        }
        fn maximum_batch_size(&self) -> usize {
            4
        }
        fn stop_logging_events(&self) {}
        fn save_unlogged_events(&self, _: &[AnalyticsEvent]) {}
        fn restore_unlogged_events(&self, _: &mut Vec<AnalyticsEvent>) {}
    }
    let shared = Shared::new(10);
    log(&shared, &["a"]);
    let g = shared.begin(&Panics, 10);
    let mut b = Batcher::new(10, shared.lock().period_epoch, 0);
    assert_eq!(step(&shared, &Panics, &mut b, g, 10), Step::WaitUntil(30));
    assert_eq!(shared.lock().queue.len(), 1);
}

#[test]
fn b3_a_late_success_after_a_timed_out_stop_leaves_the_queue_alone() {
    let shared = Arc::new(Shared::new(10));
    let sink = FakeSink::new(10);
    let s2 = Arc::clone(&shared);
    let taken = Arc::new(Mutex::new(Vec::new()));
    let t2 = Arc::clone(&taken);
    *sink.during_send.lock().unwrap() = Some(Box::new(move || {
        // The stopper gave up waiting and took the queue; then a new event arrived.
        *t2.lock().unwrap() = s2.take_unlogged().0;
        s2.push(&AnalyticsEvent::new("later", 0, 0));
    }));
    log(&shared, &["a"]);
    let (g, mut b) = begin(&shared, &sink, 10, 0);
    assert_eq!(step(&shared, &sink, &mut b, g, 10), Step::Stop);
    assert_eq!(taken.lock().unwrap().len(), 1);
    let left: Vec<_> = shared
        .lock()
        .queue
        .iter()
        .map(|e| e.name().to_owned())
        .collect();
    assert_eq!(left, ["later"]);
}

/// One action in a B4 run.
#[derive(Debug, Clone)]
enum Op {
    Log(u8),
    Advance(u16),
    Restart,
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        3 => (1u8..5).prop_map(Op::Log),
        3 => (0u16..3_000).prop_map(Op::Advance),
        1 => Just(Op::Restart),
    ]
}

/// Steps the core until the next batch is in the future.
fn run_until(shared: &Shared, sink: &FakeSink, b: &mut Batcher, g: u64, now: u64) {
    while let Step::WaitUntil(due) = step(shared, sink, b, g, now) {
        if due > now {
            break;
        }
    }
}

proptest! {
    #[test]
    fn b4_no_event_is_lost_or_duplicated_across_failures_stops_and_restarts(
        failures in proptest::collection::vec(any::<bool>(), 0..40),
        max in 0usize..6,
        base in 1u64..500,
        ops in proptest::collection::vec(op(), 0..60),
    ) {
        let shared = Shared::new(usize::MAX);
        let sink = FakeSink::new(max)
            .script(&failures.iter().map(|f| !f).collect::<Vec<_>>());
        let mut logged = Vec::new();
        let mut now = 0u64;
        let (mut g, mut b) = begin(&shared, &sink, base, now);
        for op in ops {
            match op {
                Op::Log(n) => {
                    for _ in 0..n {
                        let name = format!("e{}", logged.len());
                        shared.push(&ev(&name));
                        logged.push(name);
                    }
                }
                Op::Advance(ms) => {
                    now += u64::from(ms);
                    run_until(&shared, &sink, &mut b, g, now);
                }
                Op::Restart => {
                    shared.request_stop();
                    prop_assert_eq!(step(&shared, &sink, &mut b, g, now), Step::Stop);
                    let (events, dropped) = shared.take_unlogged();
                    prop_assert_eq!(dropped, 0);
                    if !events.is_empty() {
                        sink.save_unlogged_events(&events);
                    }
                    (g, b) = begin(&shared, &sink, base, now);
                }
            }
        }
        shared.request_stop();
        let (events, _) = shared.take_unlogged();
        sink.save_unlogged_events(&events);
        let mut all = sink.delivered.lock().unwrap().clone();
        all.extend(sink.saved.lock().unwrap().iter().map(|e| e.name().to_owned()));
        prop_assert_eq!(all, logged);
        for call in sink.calls() {
            prop_assert!(!call.is_empty() && call.len() <= max.max(1));
        }
    }
}

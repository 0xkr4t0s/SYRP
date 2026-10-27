// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B1: `AnalyticsEvent` and `Analytics`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use syrp_analytics::{Analytics, AnalyticsDestination, AnalyticsEvent};
use syrp_sys::Time;

type Log = Arc<Mutex<Vec<(usize, AnalyticsEvent)>>>;

struct Recorder {
    id: usize,
    log: Log,
}

impl AnalyticsDestination for Recorder {
    fn log_event(&self, event: &AnalyticsEvent) {
        self.log.lock().unwrap().push((self.id, event.clone()));
    }
}

fn with_recorders(n: usize) -> (Analytics, Log) {
    let log = Log::default();
    let analytics = Analytics::new();
    for id in 0..n {
        analytics.add_destination(Box::new(Recorder {
            id,
            log: log.clone(),
        }));
    }
    (analytics, log)
}

#[test]
fn b1_event_has_name_type_ordered_parameters_and_user_fields() {
    let e = AnalyticsEvent::new("export", -3, 77)
        .with_parameters([("z", "1"), ("a", "2"), ("z", "3")])
        .with_user_id("u")
        .with_user_properties([("plan", "pro")]);
    assert_eq!(e.name(), "export");
    assert_eq!(e.event_type(), -3);
    assert_eq!(e.timestamp(), 77);
    let params: Vec<_> = e.parameters().collect();
    assert_eq!(params, [("z", "1"), ("a", "2"), ("z", "3")]);
    assert_eq!(e.parameter("z"), Some("1"));
    assert_eq!(e.user_id(), "u");
    assert_eq!(e.user_properties().collect::<Vec<_>>(), [("plan", "pro")]);
    let v = e.as_view();
    assert_eq!(v.parameters().next_back(), Some(("z", "3")));
    assert_eq!(v.user_property("plan"), Some("pro"));
}

#[test]
fn b1_log_event_attaches_the_current_user_id_and_properties() {
    let (analytics, log) = with_recorders(1);
    analytics.log_event("first", [("k", "v")], 4);
    analytics.set_user_id("alice");
    analytics.set_user_properties([("b", "2"), ("a", "1")]);
    analytics.log_event("second", Vec::<(String, String)>::new(), 0);
    analytics.set_user_id("bob");
    analytics.set_user_properties(Vec::<(&str, &str)>::new());
    analytics.log_event("third", [("x", "y")], 9);

    let log = log.lock().unwrap();
    let events: Vec<_> = log.iter().map(|(_, e)| e).collect();
    assert_eq!(events.len(), 3);
    assert_eq!(
        (
            events[0].name(),
            events[0].event_type(),
            events[0].user_id()
        ),
        ("first", 4, "")
    );
    assert_eq!(events[0].parameter("k"), Some("v"));
    assert_eq!(events[1].user_id(), "alice");
    assert_eq!(
        events[1].user_properties().collect::<Vec<_>>(),
        [("b", "2"), ("a", "1")]
    );
    assert_eq!(events[2].user_id(), "bob");
    assert_eq!(events[2].user_properties().len(), 0);
    assert_eq!(events[2].event_type(), 9);
}

#[test]
fn b1_timestamp_comes_from_the_millisecond_counter() {
    let (analytics, log) = with_recorders(1);
    let before = Time::millisecond_counter();
    analytics.log_event("t", [("k", "v")], 0);
    let after = Time::millisecond_counter();
    let stamp = log.lock().unwrap()[0].1.timestamp();
    // Wrap-aware: the stamp lies between the two readings.
    assert!(stamp.wrapping_sub(before) <= after.wrapping_sub(before));
}

#[test]
fn b1_every_destination_gets_each_event_in_the_order_added() {
    let (analytics, log) = with_recorders(3);
    analytics.log_event("a", [("k", "v")], 0);
    analytics.log_event("b", [("k", "v")], 0);
    let order: Vec<_> = log
        .lock()
        .unwrap()
        .iter()
        .map(|(id, e)| (*id, e.name().to_owned()))
        .collect();
    let expected: Vec<_> = ["a", "b"]
        .iter()
        .flat_map(|n| (0..3).map(move |id| (id, (*n).to_owned())))
        .collect();
    assert_eq!(order, expected);
    // One event value per call: all destinations see identical events.
    let log = log.lock().unwrap();
    assert!(log[..3].iter().all(|(_, e)| *e == log[0].1));
}

#[test]
fn b1_events_logged_while_suspended_are_dropped() {
    let (analytics, log) = with_recorders(2);
    analytics.set_suspended(true);
    assert!(analytics.is_suspended());
    analytics.log_event("lost", [("k", "v")], 0);
    assert!(log.lock().unwrap().is_empty());
    analytics.set_suspended(false);
    analytics.log_event("kept", [("k", "v")], 0);
    let names: Vec<_> = log
        .lock()
        .unwrap()
        .iter()
        .map(|(_, e)| e.name().to_owned())
        .collect();
    assert_eq!(names, ["kept", "kept"]);
}

#[test]
fn b1_destinations_guard_lists_and_removes_in_order() {
    struct Flag(Arc<AtomicBool>);
    impl AnalyticsDestination for Flag {
        fn log_event(&self, _: &AnalyticsEvent) {}
    }
    impl Drop for Flag {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    let (analytics, log) = with_recorders(2);
    let dropped = Arc::new(AtomicBool::new(false));
    analytics.add_destination(Box::new(Flag(dropped.clone())));
    {
        let mut d = analytics.destinations();
        assert_eq!((d.len(), d.iter().count()), (3, 3));
        assert!(d.get(2).is_some() && d.get(3).is_none());
        assert!(d.remove(5).is_none());
        let removed = d.remove(2).expect("index 2 exists");
        assert!(!dropped.load(Ordering::SeqCst));
        drop(removed);
        assert!(dropped.load(Ordering::SeqCst));
        assert!(d.remove(0).is_some());
    }
    analytics.log_event("after", [("k", "v")], 0);
    let ids: Vec<_> = log.lock().unwrap().iter().map(|(id, _)| *id).collect();
    assert_eq!(ids, [1]);
    assert_eq!(analytics.destinations().take_all().len(), 1);
    assert!(analytics.destinations().is_empty());
}

#[test]
fn b1_global_is_one_send_sync_instance() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Analytics>();
    assert!(std::ptr::eq(Analytics::global(), Analytics::global()));
    let handle = std::thread::spawn(|| Analytics::global() as *const Analytics as usize);
    assert_eq!(
        handle.join().unwrap(),
        Analytics::global() as *const Analytics as usize
    );
}

#[test]
fn b1_concurrent_loggers_produce_one_order_seen_by_every_destination() {
    let (analytics, log) = with_recorders(2);
    let analytics = Arc::new(analytics);
    let threads: Vec<_> = (0..4)
        .map(|t| {
            let a = analytics.clone();
            std::thread::spawn(move || {
                for i in 0..50 {
                    a.log_event(format!("{t}-{i}"), [("k", "v")], t);
                }
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
    let log = log.lock().unwrap();
    assert_eq!(log.len(), 400);
    let per = |id: usize| -> Vec<String> {
        log.iter()
            .filter(|(d, _)| *d == id)
            .map(|(_, e)| e.name().to_owned())
            .collect()
    };
    assert_eq!(per(0), per(1));
    // Each pair of entries is (destination 0, destination 1) for the same event.
    assert!(
        log.chunks(2)
            .all(|c| c[0].0 == 0 && c[1].0 == 1 && c[0].1 == c[1].1)
    );
}

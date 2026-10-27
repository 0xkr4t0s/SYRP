// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B2: a panicking destination does not stop the others. Kept in its own test binary
//! because it installs a process-wide logger.

use std::sync::{Arc, Mutex};

use syrp_analytics::{Analytics, AnalyticsDestination, AnalyticsEvent};
use syrp_sys::{Logger, set_current_logger};

struct Collect(Arc<Mutex<Vec<String>>>);
impl Logger for Collect {
    fn log(&self, message: &str) {
        self.0.lock().unwrap().push(message.to_owned());
    }
}

struct Panics;
impl AnalyticsDestination for Panics {
    fn log_event(&self, event: &AnalyticsEvent) {
        panic!("boom on {}", event.name());
    }
}

struct Count(Arc<Mutex<Vec<String>>>);
impl AnalyticsDestination for Count {
    fn log_event(&self, event: &AnalyticsEvent) {
        self.0.lock().unwrap().push(event.name().to_owned());
    }
}

#[test]
fn b2_destination_trait_objects_are_send_sync() {
    fn assert_send_sync<T: Send + Sync + ?Sized>() {}
    assert_send_sync::<dyn AnalyticsDestination>();
}

#[test]
fn b2_a_panicking_destination_is_logged_and_the_others_still_run() {
    let messages = Arc::new(Mutex::new(Vec::new()));
    let previous = set_current_logger(Some(Box::new(Collect(messages.clone()))));

    let seen = Arc::new(Mutex::new(Vec::new()));
    let analytics = Analytics::new();
    analytics.add_destination(Box::new(Count(seen.clone())));
    analytics.add_destination(Box::new(Panics));
    analytics.add_destination(Box::new(Count(seen.clone())));
    analytics.log_event("one", [("k", "v")], 0);
    analytics.log_event("two", [("k", "v")], 0);

    set_current_logger(previous);
    assert_eq!(*seen.lock().unwrap(), ["one", "one", "two", "two"]);
    let messages = messages.lock().unwrap();
    assert_eq!(messages.len(), 2);
    assert!(messages[0].contains("destination 1") && messages[0].contains("boom on one"));
    assert!(messages[1].contains("boom on two"));
    // The lock was not poisoned: the instance still works.
    analytics.set_user_id("still fine");
}

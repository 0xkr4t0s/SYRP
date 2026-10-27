// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Headless analytics for Syrp (master plan §5.2, L2 Headless extras), the core of JUCE's
//! `juce_analytics` module.
//!
//! - [`Analytics`]: holds the user id, user properties and destinations, and passes each
//!   logged event to every destination. [`Analytics::global`] is the process-wide instance.
//! - [`AnalyticsDestination`]: the trait a destination implements.
//! - [`AnalyticsEvent`]: one logged event, read through accessors or an
//!   [`AnalyticsEventView`].
//!
//! No destination here talks to the network; those arrive in Wave 5.1.
//!
//! ```
//! use std::sync::{Arc, Mutex};
//! use syrp_analytics::{Analytics, AnalyticsDestination, AnalyticsEvent};
//!
//! struct Count(Arc<Mutex<usize>>);
//! impl AnalyticsDestination for Count {
//!     fn log_event(&self, _: &AnalyticsEvent) {
//!         *self.0.lock().unwrap() += 1;
//!     }
//! }
//! let n = Arc::new(Mutex::new(0));
//! let analytics = Analytics::new();
//! analytics.add_destination(Box::new(Count(n.clone())));
//! analytics.log_event("launch", [("version", "1.0")], 0);
//! analytics.set_suspended(true);
//! analytics.log_event("ignored", [("k", "v")], 0);
//! assert_eq!(*n.lock().unwrap(), 1);
//! ```

mod analytics;
mod event;

pub use analytics::{Analytics, AnalyticsDestination, DestinationIter, Destinations};
pub use event::{AnalyticsEvent, AnalyticsEventView, StringPairs};

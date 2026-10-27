// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`Analytics`], the [`AnalyticsDestination`] trait and the [`Destinations`] guard.

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use syrp_sys::Time;

use crate::event::{AnalyticsEvent, collect_pairs};

/// Somewhere analytics events are sent. Implement it and add it with
/// [`Analytics::add_destination`].
///
/// [`log_event`](Self::log_event) runs on the thread that called
/// [`Analytics::log_event`], while that `Analytics` holds its lock, so it should return
/// quickly (for example by queueing the event for another thread) and must not
/// call back into the same `Analytics`: that deadlocks. A panic in it is caught and logged,
/// and the remaining destinations still receive the event.
///
/// ```
/// use std::sync::Mutex;
/// use syrp_analytics::{AnalyticsDestination, AnalyticsEvent};
///
/// #[derive(Default)]
/// struct Names(Mutex<Vec<String>>);
/// impl AnalyticsDestination for Names {
///     fn log_event(&self, event: &AnalyticsEvent) {
///         self.0.lock().unwrap().push(event.name().to_owned());
///     }
/// }
/// let d = Names::default();
/// d.log_event(&AnalyticsEvent::new("hi", 0, 0));
/// assert_eq!(d.0.lock().unwrap()[0], "hi");
/// ```
#[doc(alias = "juce::AnalyticsDestination")]
pub trait AnalyticsDestination: Send + Sync {
    /// Receives one event.
    ///
    /// ```
    /// # use syrp_analytics::{AnalyticsDestination, AnalyticsEvent};
    /// struct Nowhere;
    /// impl AnalyticsDestination for Nowhere {
    ///     fn log_event(&self, _: &AnalyticsEvent) {}
    /// }
    /// Nowhere.log_event(&AnalyticsEvent::new("x", 0, 0));
    /// ```
    #[doc(alias = "logEvent")]
    fn log_event(&self, event: &AnalyticsEvent);
}

/// Collects events and passes each one to every destination added to it.
///
/// `Send + Sync`; every method takes `&self` and may be called from any thread. Use
/// [`Analytics::global`] for one process-wide instance, or [`Analytics::new`] for an
/// independent one.
///
/// ```
/// use std::sync::{Arc, Mutex};
/// use syrp_analytics::{Analytics, AnalyticsDestination, AnalyticsEvent};
///
/// struct Shared(Arc<Mutex<Vec<AnalyticsEvent>>>);
/// impl AnalyticsDestination for Shared {
///     fn log_event(&self, e: &AnalyticsEvent) {
///         self.0.lock().unwrap().push(e.clone());
///     }
/// }
/// let seen = Arc::new(Mutex::new(Vec::new()));
/// let analytics = Analytics::new();
/// analytics.add_destination(Box::new(Shared(seen.clone())));
/// analytics.set_user_id("user-7");
/// analytics.log_event("opened", [("file", "a.wav")], 0);
/// let seen = seen.lock().unwrap();
/// assert_eq!(seen[0].user_id(), "user-7");
/// assert_eq!(seen[0].parameter("file"), Some("a.wav"));
/// ```
#[doc(alias = "juce::Analytics")]
#[derive(Debug, Default)]
pub struct Analytics {
    inner: Mutex<Inner>,
    suspended: AtomicBool,
}

#[derive(Default)]
struct Inner {
    user_id: String,
    user_properties: Vec<(String, String)>,
    destinations: Vec<Box<dyn AnalyticsDestination>>,
}

impl std::fmt::Debug for Inner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Inner")
            .field("user_id", &self.user_id)
            .field("user_properties", &self.user_properties)
            .field("destinations", &self.destinations.len())
            .finish()
    }
}

static GLOBAL: Analytics = Analytics::new();

impl Analytics {
    /// An instance with no destinations, an empty user id, no user properties, not
    /// suspended.
    ///
    /// ```
    /// # use syrp_analytics::Analytics;
    /// assert!(Analytics::new().destinations().is_empty());
    /// ```
    #[must_use]
    pub const fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                user_id: String::new(),
                user_properties: Vec::new(),
                destinations: Vec::new(),
            }),
            suspended: AtomicBool::new(false),
        }
    }

    /// The process-wide instance, the counterpart of JUCE's `Analytics` singleton. It is
    /// never dropped, so destinations still in it at exit are not dropped either: remove
    /// them through [`destinations`](Self::destinations) to shut them down.
    ///
    /// ```
    /// # use syrp_analytics::Analytics;
    /// assert!(std::ptr::eq(Analytics::global(), Analytics::global()));
    /// ```
    #[doc(alias = "getInstance")]
    #[must_use]
    pub fn global() -> &'static Analytics {
        &GLOBAL
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Sets the user id attached to every event logged from now on.
    ///
    /// ```
    /// # use syrp_analytics::Analytics;
    /// Analytics::new().set_user_id("abc");
    /// ```
    #[doc(alias = "setUserId")]
    pub fn set_user_id(&self, user_id: impl Into<String>) {
        self.lock().user_id = user_id.into();
    }

    /// Replaces the user properties attached to every event logged from now on. Their order
    /// is kept.
    ///
    /// ```
    /// # use syrp_analytics::Analytics;
    /// Analytics::new().set_user_properties([("tier", "free")]);
    /// ```
    #[doc(alias = "setUserProperties")]
    pub fn set_user_properties<I, K, V>(&self, properties: I)
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        let properties = collect_pairs(properties);
        self.lock().user_properties = properties;
    }

    /// Adds a destination after the existing ones. The `Analytics` owns it from now on.
    ///
    /// ```
    /// # use syrp_analytics::{Analytics, AnalyticsDestination, AnalyticsEvent};
    /// struct Nowhere;
    /// impl AnalyticsDestination for Nowhere {
    ///     fn log_event(&self, _: &AnalyticsEvent) {}
    /// }
    /// let a = Analytics::new();
    /// a.add_destination(Box::new(Nowhere));
    /// assert_eq!(a.destinations().len(), 1);
    /// ```
    #[doc(alias = "addDestination")]
    pub fn add_destination(&self, destination: Box<dyn AnalyticsDestination>) {
        self.lock().destinations.push(destination);
    }

    /// Locks the destination list and returns a guard to inspect it or remove destinations
    /// (removing a threaded destination and dropping it shuts it down). Events logged on
    /// other threads wait while the guard is alive; calling another method of this
    /// `Analytics` on the same thread while holding it deadlocks.
    ///
    /// ```
    /// # use syrp_analytics::{Analytics, AnalyticsDestination, AnalyticsEvent};
    /// # struct Nowhere;
    /// # impl AnalyticsDestination for Nowhere { fn log_event(&self, _: &AnalyticsEvent) {} }
    /// let a = Analytics::new();
    /// a.add_destination(Box::new(Nowhere));
    /// let removed = a.destinations().remove(0);
    /// assert!(removed.is_some());
    /// assert!(a.destinations().is_empty());
    /// ```
    #[doc(alias = "getDestinations")]
    #[must_use]
    pub fn destinations(&self) -> Destinations<'_> {
        Destinations { guard: self.lock() }
    }

    /// Builds one event, timestamped with [`Time::millisecond_counter`] and carrying the
    /// current user id and user properties, and passes it to every destination in the order
    /// they were added. Does nothing while suspended. Calls from several threads are
    /// serialised, so every destination sees events in the same order.
    ///
    /// ```
    /// # use syrp_analytics::Analytics;
    /// let a = Analytics::new();
    /// a.log_event("tapped", [("button", "ok")], 1);
    /// a.log_event("idle", Vec::<(String, String)>::new(), 0);
    /// ```
    #[doc(alias = "logEvent")]
    pub fn log_event<I, K, V>(&self, name: impl Into<String>, parameters: I, event_type: i32)
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        if self.is_suspended() {
            return;
        }
        let event = AnalyticsEvent::new(name, event_type, Time::millisecond_counter())
            .with_parameters(parameters);
        let inner = self.lock();
        let event = event
            .with_user_id(inner.user_id.clone())
            .with_user_properties(inner.user_properties.iter().cloned());
        for (index, destination) in inner.destinations.iter().enumerate() {
            if let Err(payload) = catch_unwind(AssertUnwindSafe(|| destination.log_event(&event))) {
                syrp_sys::write_to_log(&format!(
                    "syrp-analytics: destination {index} panicked while logging event \"{}\": {}",
                    event.name(),
                    panic_message(payload.as_ref())
                ));
            }
        }
    }

    /// Suspends (`true`) or resumes (`false`) logging. Events logged while suspended are
    /// dropped, not queued.
    ///
    /// ```
    /// # use syrp_analytics::Analytics;
    /// let a = Analytics::new();
    /// a.set_suspended(true);
    /// assert!(a.is_suspended());
    /// ```
    #[doc(alias = "setSuspended")]
    pub fn set_suspended(&self, suspended: bool) {
        self.suspended.store(suspended, Ordering::SeqCst);
    }

    /// Whether logging is suspended.
    ///
    /// ```
    /// # use syrp_analytics::Analytics;
    /// assert!(!Analytics::new().is_suspended());
    /// ```
    #[must_use]
    pub fn is_suspended(&self) -> bool {
        self.suspended.load(Ordering::SeqCst)
    }
}

/// The text of a panic payload, when it is a string.
pub(crate) fn panic_message(payload: &(dyn Any + Send)) -> &str {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s
    } else {
        "non-string panic payload"
    }
}

/// The locked destination list of an [`Analytics`], from [`Analytics::destinations`].
///
/// ```
/// # use syrp_analytics::{Analytics, AnalyticsDestination, AnalyticsEvent};
/// # struct Nowhere;
/// # impl AnalyticsDestination for Nowhere { fn log_event(&self, _: &AnalyticsEvent) {} }
/// let a = Analytics::new();
/// a.add_destination(Box::new(Nowhere));
/// let d = a.destinations();
/// assert_eq!(d.iter().count(), d.len());
/// ```
pub struct Destinations<'a> {
    guard: MutexGuard<'a, Inner>,
}

impl std::fmt::Debug for Destinations<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Destinations")
            .field("len", &self.len())
            .finish()
    }
}

impl Destinations<'_> {
    /// How many destinations there are.
    ///
    /// ```
    /// # use syrp_analytics::Analytics;
    /// assert_eq!(Analytics::new().destinations().len(), 0);
    /// ```
    #[must_use]
    pub fn len(&self) -> usize {
        self.guard.destinations.len()
    }

    /// Whether there are none.
    ///
    /// ```
    /// # use syrp_analytics::Analytics;
    /// assert!(Analytics::new().destinations().is_empty());
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.guard.destinations.is_empty()
    }

    /// The destination at `index`, in the order they were added.
    ///
    /// ```
    /// # use syrp_analytics::Analytics;
    /// assert!(Analytics::new().destinations().get(0).is_none());
    /// ```
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&dyn AnalyticsDestination> {
        self.guard.destinations.get(index).map(|d| &**d)
    }

    /// The destinations, in the order they were added.
    ///
    /// ```
    /// # use syrp_analytics::Analytics;
    /// assert_eq!(Analytics::new().destinations().iter().len(), 0);
    /// ```
    #[must_use]
    pub fn iter(&self) -> DestinationIter<'_> {
        DestinationIter {
            inner: self.guard.destinations.iter(),
        }
    }

    /// Removes and returns the destination at `index`, keeping the others in order. Dropping
    /// the returned box shuts the destination down.
    ///
    /// ```
    /// # use syrp_analytics::Analytics;
    /// assert!(Analytics::new().destinations().remove(3).is_none());
    /// ```
    pub fn remove(&mut self, index: usize) -> Option<Box<dyn AnalyticsDestination>> {
        (index < self.len()).then(|| self.guard.destinations.remove(index))
    }

    /// Removes every destination and returns them in the order they were added.
    ///
    /// ```
    /// # use syrp_analytics::Analytics;
    /// assert!(Analytics::new().destinations().take_all().is_empty());
    /// ```
    pub fn take_all(&mut self) -> Vec<Box<dyn AnalyticsDestination>> {
        std::mem::take(&mut self.guard.destinations)
    }
}

/// An iterator over the destinations of a [`Destinations`] guard.
///
/// ```
/// # use syrp_analytics::Analytics;
/// let a = Analytics::new();
/// let d = a.destinations();
/// let mut it = d.iter();
/// assert!(it.next().is_none());
/// ```
pub struct DestinationIter<'a> {
    inner: std::slice::Iter<'a, Box<dyn AnalyticsDestination>>,
}

impl std::fmt::Debug for DestinationIter<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DestinationIter")
            .field("remaining", &self.inner.len())
            .finish()
    }
}

impl<'a> Iterator for DestinationIter<'a> {
    type Item = &'a dyn AnalyticsDestination;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|d| &**d)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl ExactSizeIterator for DestinationIter<'_> {}
impl std::iter::FusedIterator for DestinationIter<'_> {}

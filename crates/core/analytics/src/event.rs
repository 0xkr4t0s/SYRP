// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`AnalyticsEvent`], its borrowed view and the ordered string-pair iterator.

use std::iter::FusedIterator;

/// One event to be logged: a name, a numeric type, a timestamp, ordered parameters, and the
/// user id and user properties that were current when it was logged.
///
/// Built with [`AnalyticsEvent::new`] and the `with_*` methods; read through accessors or a
/// borrowed [`AnalyticsEventView`]. [`Analytics::log_event`](crate::Analytics::log_event)
/// builds these for you; constructing one directly is for destinations that restore saved
/// events and for tests.
///
/// ```
/// use syrp_analytics::AnalyticsEvent;
/// let e = AnalyticsEvent::new("play", 2, 1_000)
///     .with_parameters([("track", "intro")])
///     .with_user_id("u-1");
/// assert_eq!(e.name(), "play");
/// assert_eq!(e.parameter("track"), Some("intro"));
/// assert_eq!(e.user_id(), "u-1");
/// ```
#[doc(alias = "AnalyticsDestination::AnalyticsEvent")]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AnalyticsEvent {
    name: String,
    event_type: i32,
    timestamp: u32,
    parameters: Vec<(String, String)>,
    user_id: String,
    user_properties: Vec<(String, String)>,
}

impl AnalyticsEvent {
    /// An event with no parameters, an empty user id and no user properties. `timestamp` is
    /// in milliseconds of [`syrp_sys::Time::millisecond_counter`].
    ///
    /// ```
    /// # use syrp_analytics::AnalyticsEvent;
    /// let e = AnalyticsEvent::new("start", 0, 42);
    /// assert_eq!((e.event_type(), e.timestamp()), (0, 42));
    /// assert_eq!(e.parameters().len(), 0);
    /// ```
    #[must_use]
    pub fn new(name: impl Into<String>, event_type: i32, timestamp: u32) -> Self {
        Self {
            name: name.into(),
            event_type,
            timestamp,
            parameters: Vec::new(),
            user_id: String::new(),
            user_properties: Vec::new(),
        }
    }

    /// Replaces the parameters with `pairs`, kept in the order given (duplicates included).
    ///
    /// ```
    /// # use syrp_analytics::AnalyticsEvent;
    /// let e = AnalyticsEvent::new("x", 0, 0).with_parameters([("b", "1"), ("a", "2")]);
    /// assert_eq!(e.parameters().collect::<Vec<_>>(), [("b", "1"), ("a", "2")]);
    /// ```
    #[must_use]
    pub fn with_parameters<I, K, V>(mut self, pairs: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        self.parameters = collect_pairs(pairs);
        self
    }

    /// Replaces the user id.
    ///
    /// ```
    /// # use syrp_analytics::AnalyticsEvent;
    /// assert_eq!(AnalyticsEvent::new("x", 0, 0).with_user_id("me").user_id(), "me");
    /// ```
    #[must_use]
    pub fn with_user_id(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = user_id.into();
        self
    }

    /// Replaces the user properties with `pairs`, kept in the order given.
    ///
    /// ```
    /// # use syrp_analytics::AnalyticsEvent;
    /// let e = AnalyticsEvent::new("x", 0, 0).with_user_properties([("plan", "pro")]);
    /// assert_eq!(e.user_property("plan"), Some("pro"));
    /// ```
    #[must_use]
    pub fn with_user_properties<I, K, V>(mut self, pairs: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        self.user_properties = collect_pairs(pairs);
        self
    }

    /// A borrowed, `Copy` view of this event.
    ///
    /// ```
    /// # use syrp_analytics::AnalyticsEvent;
    /// let e = AnalyticsEvent::new("x", 0, 0);
    /// let v = e.as_view();
    /// assert_eq!(v.name(), "x");
    /// ```
    #[must_use]
    pub fn as_view(&self) -> AnalyticsEventView<'_> {
        AnalyticsEventView { event: self }
    }

    /// The event name.
    ///
    /// ```
    /// # use syrp_analytics::AnalyticsEvent;
    /// assert_eq!(AnalyticsEvent::new("save", 0, 0).name(), "save");
    /// ```
    #[must_use]
    pub fn name(&self) -> &str {
        self.as_view().name()
    }

    /// The event type given when it was logged (`0` unless the caller chose another).
    ///
    /// ```
    /// # use syrp_analytics::AnalyticsEvent;
    /// assert_eq!(AnalyticsEvent::new("x", 7, 0).event_type(), 7);
    /// ```
    #[doc(alias = "eventType")]
    #[must_use]
    pub fn event_type(&self) -> i32 {
        self.event_type
    }

    /// When the event was logged, in milliseconds of
    /// [`syrp_sys::Time::millisecond_counter`] (which wraps after about 49.7 days).
    ///
    /// ```
    /// # use syrp_analytics::AnalyticsEvent;
    /// assert_eq!(AnalyticsEvent::new("x", 0, 99).timestamp(), 99);
    /// ```
    #[must_use]
    pub fn timestamp(&self) -> u32 {
        self.timestamp
    }

    /// The parameters, in order.
    ///
    /// ```
    /// # use syrp_analytics::AnalyticsEvent;
    /// let e = AnalyticsEvent::new("x", 0, 0).with_parameters([("k", "v")]);
    /// assert_eq!(e.parameters().next(), Some(("k", "v")));
    /// ```
    #[must_use]
    pub fn parameters(&self) -> StringPairs<'_> {
        self.as_view().parameters()
    }

    /// The value of the first parameter named `key`.
    ///
    /// ```
    /// # use syrp_analytics::AnalyticsEvent;
    /// let e = AnalyticsEvent::new("x", 0, 0).with_parameters([("k", "v")]);
    /// assert_eq!(e.parameter("k"), Some("v"));
    /// assert_eq!(e.parameter("missing"), None);
    /// ```
    #[must_use]
    pub fn parameter(&self, key: &str) -> Option<&str> {
        self.as_view().parameter(key)
    }

    /// The user id that was set when the event was logged (empty when none was).
    ///
    /// ```
    /// # use syrp_analytics::AnalyticsEvent;
    /// assert_eq!(AnalyticsEvent::new("x", 0, 0).user_id(), "");
    /// ```
    #[doc(alias = "userID")]
    #[must_use]
    pub fn user_id(&self) -> &str {
        self.as_view().user_id()
    }

    /// The user properties that were set when the event was logged, in order.
    ///
    /// ```
    /// # use syrp_analytics::AnalyticsEvent;
    /// let e = AnalyticsEvent::new("x", 0, 0).with_user_properties([("a", "1")]);
    /// assert_eq!(e.user_properties().count(), 1);
    /// ```
    #[must_use]
    pub fn user_properties(&self) -> StringPairs<'_> {
        self.as_view().user_properties()
    }

    /// The value of the first user property named `key`.
    ///
    /// ```
    /// # use syrp_analytics::AnalyticsEvent;
    /// let e = AnalyticsEvent::new("x", 0, 0).with_user_properties([("a", "1")]);
    /// assert_eq!(e.user_property("a"), Some("1"));
    /// ```
    #[must_use]
    pub fn user_property(&self, key: &str) -> Option<&str> {
        self.as_view().user_property(key)
    }
}

/// A borrowed, `Copy` view of an [`AnalyticsEvent`]. Accessors take the view by value and
/// return data borrowed for the event's lifetime.
///
/// ```
/// use syrp_analytics::{AnalyticsEvent, AnalyticsEventView};
/// let e = AnalyticsEvent::new("x", 1, 2).with_user_id("u");
/// let v: AnalyticsEventView<'_> = e.as_view();
/// let id: &str = v.user_id();
/// assert_eq!((v.name(), v.event_type(), v.timestamp(), id), ("x", 1, 2, "u"));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnalyticsEventView<'a> {
    event: &'a AnalyticsEvent,
}

impl<'a> AnalyticsEventView<'a> {
    /// The event name.
    ///
    /// ```
    /// # use syrp_analytics::AnalyticsEvent;
    /// assert_eq!(AnalyticsEvent::new("n", 0, 0).as_view().name(), "n");
    /// ```
    #[must_use]
    pub fn name(self) -> &'a str {
        &self.event.name
    }

    /// The event type.
    ///
    /// ```
    /// # use syrp_analytics::AnalyticsEvent;
    /// assert_eq!(AnalyticsEvent::new("n", 3, 0).as_view().event_type(), 3);
    /// ```
    #[must_use]
    pub fn event_type(self) -> i32 {
        self.event.event_type
    }

    /// The timestamp in milliseconds of [`syrp_sys::Time::millisecond_counter`].
    ///
    /// ```
    /// # use syrp_analytics::AnalyticsEvent;
    /// assert_eq!(AnalyticsEvent::new("n", 0, 5).as_view().timestamp(), 5);
    /// ```
    #[must_use]
    pub fn timestamp(self) -> u32 {
        self.event.timestamp
    }

    /// The parameters, in order.
    ///
    /// ```
    /// # use syrp_analytics::AnalyticsEvent;
    /// let e = AnalyticsEvent::new("n", 0, 0).with_parameters([("a", "b")]);
    /// assert_eq!(e.as_view().parameters().len(), 1);
    /// ```
    #[must_use]
    pub fn parameters(self) -> StringPairs<'a> {
        StringPairs {
            inner: self.event.parameters.iter(),
        }
    }

    /// The value of the first parameter named `key`.
    ///
    /// ```
    /// # use syrp_analytics::AnalyticsEvent;
    /// let e = AnalyticsEvent::new("n", 0, 0).with_parameters([("a", "b")]);
    /// assert_eq!(e.as_view().parameter("a"), Some("b"));
    /// ```
    #[must_use]
    pub fn parameter(self, key: &str) -> Option<&'a str> {
        find(&self.event.parameters, key)
    }

    /// The user id.
    ///
    /// ```
    /// # use syrp_analytics::AnalyticsEvent;
    /// assert_eq!(AnalyticsEvent::new("n", 0, 0).with_user_id("u").as_view().user_id(), "u");
    /// ```
    #[must_use]
    pub fn user_id(self) -> &'a str {
        &self.event.user_id
    }

    /// The user properties, in order.
    ///
    /// ```
    /// # use syrp_analytics::AnalyticsEvent;
    /// let e = AnalyticsEvent::new("n", 0, 0).with_user_properties([("a", "b")]);
    /// assert_eq!(e.as_view().user_properties().next(), Some(("a", "b")));
    /// ```
    #[must_use]
    pub fn user_properties(self) -> StringPairs<'a> {
        StringPairs {
            inner: self.event.user_properties.iter(),
        }
    }

    /// The value of the first user property named `key`.
    ///
    /// ```
    /// # use syrp_analytics::AnalyticsEvent;
    /// let e = AnalyticsEvent::new("n", 0, 0).with_user_properties([("a", "b")]);
    /// assert_eq!(e.as_view().user_property("a"), Some("b"));
    /// ```
    #[must_use]
    pub fn user_property(self, key: &str) -> Option<&'a str> {
        find(&self.event.user_properties, key)
    }
}

/// An iterator over ordered `(key, value)` string pairs, from
/// [`AnalyticsEvent::parameters`] or [`AnalyticsEvent::user_properties`].
///
/// ```
/// # use syrp_analytics::AnalyticsEvent;
/// let e = AnalyticsEvent::new("x", 0, 0).with_parameters([("a", "1"), ("b", "2")]);
/// let keys: Vec<&str> = e.parameters().map(|(k, _)| k).collect();
/// assert_eq!(keys, ["a", "b"]);
/// ```
#[derive(Debug, Clone)]
pub struct StringPairs<'a> {
    inner: std::slice::Iter<'a, (String, String)>,
}

impl<'a> Iterator for StringPairs<'a> {
    type Item = (&'a str, &'a str);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl DoubleEndedIterator for StringPairs<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner
            .next_back()
            .map(|(k, v)| (k.as_str(), v.as_str()))
    }
}

impl ExactSizeIterator for StringPairs<'_> {}
impl FusedIterator for StringPairs<'_> {}

/// Collects caller pairs into owned strings, keeping their order.
pub(crate) fn collect_pairs<I, K, V>(pairs: I) -> Vec<(String, String)>
where
    I: IntoIterator<Item = (K, V)>,
    K: Into<String>,
    V: Into<String>,
{
    pairs
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect()
}

fn find<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

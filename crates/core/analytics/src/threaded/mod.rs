// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`ThreadedAnalyticsDestination`], its [`BatchSink`] and [`StopReport`].

mod core;
#[cfg(test)]
mod tests;

use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use self::core::{Batcher, Shared, Step, step};
use crate::analytics::AnalyticsDestination;
use crate::error::AnalyticsError;
use crate::event::AnalyticsEvent;

/// The longest period back-off grows to after failed batches, unless the base period is
/// longer (Syrp's choice: JUCE's documentation leaves back-off to the subclass).
///
/// ```
/// assert_eq!(syrp_analytics::MAX_BACKOFF_PERIOD.as_secs(), 60);
/// ```
pub const MAX_BACKOFF_PERIOD: Duration = Duration::from_millis(self::core::MAX_BACKOFF_PERIOD_MS);

/// How long dropping a [`ThreadedAnalyticsDestination`] waits for its thread (Syrp's choice).
pub const DROP_STOP_TIMEOUT: Duration = Duration::from_secs(2);

/// How many events a [`ThreadedAnalyticsDestination::new`] queue holds (Syrp's choice).
pub const DEFAULT_QUEUE_CAPACITY: usize = 10_000;

/// Where a [`ThreadedAnalyticsDestination`] sends batches, and how it saves and restores the
/// events it could not send. These are the overridable methods of JUCE's
/// `ThreadedAnalyticsDestination`.
///
/// Every method takes `&self`: [`stop_logging_events`](Self::stop_logging_events) runs on the
/// stopping thread while [`log_batched_events`](Self::log_batched_events) may be running on
/// the analytics thread, and must be able to make it return early.
///
/// ```
/// use std::sync::Mutex;
/// use syrp_analytics::{AnalyticsEvent, BatchSink};
///
/// #[derive(Default)]
/// struct Memory {
///     sent: Mutex<Vec<AnalyticsEvent>>,
///     saved: Mutex<Vec<AnalyticsEvent>>,
/// }
/// impl BatchSink for Memory {
///     fn log_batched_events(&self, events: &[AnalyticsEvent]) -> bool {
///         self.sent.lock().unwrap().extend_from_slice(events);
///         true
///     }
///     fn maximum_batch_size(&self) -> usize { 20 }
///     fn stop_logging_events(&self) {}
///     fn save_unlogged_events(&self, events: &[AnalyticsEvent]) {
///         self.saved.lock().unwrap().extend_from_slice(events);
///     }
///     fn restore_unlogged_events(&self, events: &mut Vec<AnalyticsEvent>) {
///         events.append(&mut self.saved.lock().unwrap());
///     }
/// }
/// assert!(Memory::default().log_batched_events(&[]));
/// ```
pub trait BatchSink: Send + Sync + 'static {
    /// Sends `events` (oldest first, never more than
    /// [`maximum_batch_size`](Self::maximum_batch_size)) and returns `true` when they were
    /// delivered. On `false` the next call starts with the same events, in the same order.
    /// Runs on the analytics thread.
    ///
    /// ```
    /// # use syrp_analytics::{AnalyticsEvent, BatchSink};
    /// # struct S;
    /// # impl BatchSink for S {
    /// #     fn log_batched_events(&self, e: &[AnalyticsEvent]) -> bool { !e.is_empty() }
    /// #     fn maximum_batch_size(&self) -> usize { 1 }
    /// #     fn stop_logging_events(&self) {}
    /// #     fn save_unlogged_events(&self, _: &[AnalyticsEvent]) {}
    /// #     fn restore_unlogged_events(&self, _: &mut Vec<AnalyticsEvent>) {}
    /// # }
    /// assert!(S.log_batched_events(&[AnalyticsEvent::new("x", 0, 0)]));
    /// ```
    #[doc(alias = "logBatchedEvents")]
    fn log_batched_events(&self, events: &[AnalyticsEvent]) -> bool;

    /// The most events one batch may hold. `0` is treated as `1` (Syrp's choice).
    ///
    /// ```
    /// # use syrp_analytics::{AnalyticsEvent, BatchSink};
    /// # struct S; impl BatchSink for S { fn log_batched_events(&self, _: &[AnalyticsEvent]) -> bool { true }
    /// # fn maximum_batch_size(&self) -> usize { 5 } fn stop_logging_events(&self) {} fn save_unlogged_events(&self, _: &[AnalyticsEvent]) {}
    /// # fn restore_unlogged_events(&self, _: &mut Vec<AnalyticsEvent>) {} }
    /// assert_eq!(S.maximum_batch_size(), 5);
    /// ```
    #[doc(alias = "getMaximumBatchSize")]
    fn maximum_batch_size(&self) -> usize;

    /// Called on the stopping thread when a stop begins: make any running
    /// [`log_batched_events`](Self::log_batched_events) call return as soon as possible.
    /// No batch is started after it.
    ///
    /// ```
    /// # use syrp_analytics::{AnalyticsEvent, BatchSink};
    /// # struct S; impl BatchSink for S { fn log_batched_events(&self, _: &[AnalyticsEvent]) -> bool { true }
    /// # fn maximum_batch_size(&self) -> usize { 1 } fn stop_logging_events(&self) {} fn save_unlogged_events(&self, _: &[AnalyticsEvent]) {}
    /// # fn restore_unlogged_events(&self, _: &mut Vec<AnalyticsEvent>) {} }
    /// S.stop_logging_events();
    /// ```
    #[doc(alias = "stopLoggingEvents")]
    fn stop_logging_events(&self);

    /// Keeps `events`, which were never delivered, so a later
    /// [`restore_unlogged_events`](Self::restore_unlogged_events) can return them. Called by a
    /// stop (and by drop) only when there is at least one event; they are oldest first.
    ///
    /// ```
    /// # use syrp_analytics::{AnalyticsEvent, BatchSink};
    /// # struct S; impl BatchSink for S { fn log_batched_events(&self, _: &[AnalyticsEvent]) -> bool { true }
    /// # fn maximum_batch_size(&self) -> usize { 1 } fn stop_logging_events(&self) {} fn save_unlogged_events(&self, _: &[AnalyticsEvent]) {}
    /// # fn restore_unlogged_events(&self, _: &mut Vec<AnalyticsEvent>) {} }
    /// S.save_unlogged_events(&[AnalyticsEvent::new("x", 0, 0)]);
    /// ```
    #[doc(alias = "saveUnloggedEvents")]
    fn save_unlogged_events(&self, events: &[AnalyticsEvent]);

    /// Appends previously saved events to `events`, oldest first, and forgets them (so they
    /// are not restored twice). Called on the starting thread by
    /// [`ThreadedAnalyticsDestination::start_analytics_thread`] before the thread starts;
    /// the restored events go ahead of any queued ones.
    ///
    /// ```
    /// # use syrp_analytics::{AnalyticsEvent, BatchSink};
    /// # struct S; impl BatchSink for S { fn log_batched_events(&self, _: &[AnalyticsEvent]) -> bool { true }
    /// # fn maximum_batch_size(&self) -> usize { 1 } fn stop_logging_events(&self) {} fn save_unlogged_events(&self, _: &[AnalyticsEvent]) {}
    /// # fn restore_unlogged_events(&self, _: &mut Vec<AnalyticsEvent>) {} }
    /// let mut v = Vec::new();
    /// S.restore_unlogged_events(&mut v);
    /// assert!(v.is_empty());
    /// ```
    #[doc(alias = "restoreUnloggedEvents")]
    fn restore_unlogged_events(&self, events: &mut Vec<AnalyticsEvent>);
}

/// What [`ThreadedAnalyticsDestination::stop_analytics_thread`] did.
///
/// ```
/// # use std::time::Duration;
/// # use syrp_analytics::{AnalyticsEvent, BatchSink, ThreadedAnalyticsDestination};
/// # struct S; impl BatchSink for S { fn log_batched_events(&self, _: &[AnalyticsEvent]) -> bool { true }
/// # fn maximum_batch_size(&self) -> usize { 1 } fn stop_logging_events(&self) {} fn save_unlogged_events(&self, _: &[AnalyticsEvent]) {}
/// # fn restore_unlogged_events(&self, _: &mut Vec<AnalyticsEvent>) {} }
/// let d = ThreadedAnalyticsDestination::new("analytics", S);
/// let report = d.stop_analytics_thread(Duration::from_secs(1));
/// assert_eq!((report.saved(), report.dropped(), report.timed_out()), (0, 0, false));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StopReport {
    saved: usize,
    dropped: u64,
    timed_out: bool,
}

impl StopReport {
    /// How many unsent events were passed to [`BatchSink::save_unlogged_events`].
    ///
    /// ```
    /// assert_eq!(syrp_analytics::StopReport::default().saved(), 0);
    /// ```
    #[must_use]
    pub fn saved(self) -> usize {
        self.saved
    }

    /// How many events were refused because the queue was full, since the previous stop.
    ///
    /// ```
    /// assert_eq!(syrp_analytics::StopReport::default().dropped(), 0);
    /// ```
    #[must_use]
    pub fn dropped(self) -> u64 {
        self.dropped
    }

    /// Whether the thread was still running when the timeout ran out. It is left to finish
    /// on its own (threads cannot be killed safely); a batch it was sending is among the
    /// saved events and may also arrive at the sink, so it can be delivered twice.
    ///
    /// ```
    /// assert!(!syrp_analytics::StopReport::default().timed_out());
    /// ```
    #[must_use]
    pub fn timed_out(self) -> bool {
        self.timed_out
    }
}

/// An [`AnalyticsDestination`] that queues events and sends them in batches to a
/// [`BatchSink`] on its own thread, so logging never waits for the sink.
///
/// - [`log_event`](AnalyticsDestination::log_event) appends to a bounded queue
///   ([`DEFAULT_QUEUE_CAPACITY`] unless set with
///   [`with_queue_capacity`](Self::with_queue_capacity)); when it is full the new event is
///   dropped and counted (Syrp's choice). Events logged before the thread starts wait.
/// - [`start_analytics_thread`](Self::start_analytics_thread) restores saved events, then
///   once per period sends up to [`BatchSink::maximum_batch_size`] events, oldest first.
/// - A failed batch stays at the front and is retried; each failure doubles the period, up
///   to [`MAX_BACKOFF_PERIOD`] (or the base period if longer), and a success
///   resets it (Syrp's choice: JUCE's documentation leaves back-off to the subclass).
/// - [`stop_analytics_thread`](Self::stop_analytics_thread) and drop hand every unsent
///   event to [`BatchSink::save_unlogged_events`].
///
/// ```
/// use std::sync::mpsc;
/// use std::time::Duration;
/// use syrp_analytics::{AnalyticsDestination, AnalyticsEvent, BatchSink, ThreadedAnalyticsDestination};
///
/// struct Channel(mpsc::SyncSender<usize>);
/// impl BatchSink for Channel {
///     fn log_batched_events(&self, events: &[AnalyticsEvent]) -> bool {
///         self.0.send(events.len()).is_ok()
///     }
///     fn maximum_batch_size(&self) -> usize { 10 }
///     fn stop_logging_events(&self) {}
///     fn save_unlogged_events(&self, _: &[AnalyticsEvent]) {}
///     fn restore_unlogged_events(&self, _: &mut Vec<AnalyticsEvent>) {}
/// }
/// let (tx, rx) = mpsc::sync_channel(16);
/// let d = ThreadedAnalyticsDestination::new("analytics", Channel(tx));
/// d.log_event(&AnalyticsEvent::new("a", 0, 0));
/// d.log_event(&AnalyticsEvent::new("b", 0, 0));
/// d.start_analytics_thread(Duration::from_millis(1))?;
/// assert_eq!(rx.recv_timeout(Duration::from_secs(30)), Ok(2));
/// let report = d.stop_analytics_thread(Duration::from_secs(30));
/// assert_eq!(report.saved(), 0);
/// # Ok::<(), syrp_analytics::AnalyticsError>(())
/// ```
#[doc(alias = "juce::ThreadedAnalyticsDestination")]
pub struct ThreadedAnalyticsDestination<S: BatchSink> {
    name: String,
    sink: Arc<S>,
    shared: Arc<Shared>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl<S: BatchSink> std::fmt::Debug for ThreadedAnalyticsDestination<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThreadedAnalyticsDestination")
            .field("name", &self.name)
            .field("queued", &self.queued_event_count())
            .finish_non_exhaustive()
    }
}

fn period_ms(period: Duration) -> Result<u64, AnalyticsError> {
    match u64::try_from(period.as_millis()) {
        Ok(0) => Err(AnalyticsError::BatchPeriodTooShort { period }),
        Ok(ms) => Ok(ms),
        Err(_) => Ok(u64::MAX),
    }
}

impl<S: BatchSink> ThreadedAnalyticsDestination<S> {
    /// A stopped destination with an empty queue of [`DEFAULT_QUEUE_CAPACITY`] events.
    /// `thread_name` names the analytics thread.
    ///
    /// ```
    /// # use syrp_analytics::{AnalyticsEvent, BatchSink, ThreadedAnalyticsDestination};
    /// # struct S; impl BatchSink for S { fn log_batched_events(&self, _: &[AnalyticsEvent]) -> bool { true }
    /// # fn maximum_batch_size(&self) -> usize { 1 } fn stop_logging_events(&self) {} fn save_unlogged_events(&self, _: &[AnalyticsEvent]) {}
    /// # fn restore_unlogged_events(&self, _: &mut Vec<AnalyticsEvent>) {} }
    /// let d = ThreadedAnalyticsDestination::new("Analytics thread", S);
    /// assert_eq!(d.queued_event_count(), 0);
    /// ```
    #[must_use]
    pub fn new(thread_name: impl Into<String>, sink: S) -> Self {
        Self::with_queue_capacity(thread_name, sink, DEFAULT_QUEUE_CAPACITY)
    }

    /// Like [`new`](Self::new) with a queue of `capacity` events. Restored events are always
    /// accepted, even past the capacity.
    ///
    /// ```
    /// # use syrp_analytics::{AnalyticsDestination, AnalyticsEvent, BatchSink, ThreadedAnalyticsDestination};
    /// # struct S; impl BatchSink for S { fn log_batched_events(&self, _: &[AnalyticsEvent]) -> bool { true }
    /// # fn maximum_batch_size(&self) -> usize { 1 } fn stop_logging_events(&self) {} fn save_unlogged_events(&self, _: &[AnalyticsEvent]) {}
    /// # fn restore_unlogged_events(&self, _: &mut Vec<AnalyticsEvent>) {} }
    /// let d = ThreadedAnalyticsDestination::with_queue_capacity("a", S, 1);
    /// d.log_event(&AnalyticsEvent::new("kept", 0, 0));
    /// d.log_event(&AnalyticsEvent::new("dropped", 0, 0));
    /// assert_eq!((d.queued_event_count(), d.dropped_event_count()), (1, 1));
    /// ```
    #[must_use]
    pub fn with_queue_capacity(thread_name: impl Into<String>, sink: S, capacity: usize) -> Self {
        Self {
            name: thread_name.into(),
            sink: Arc::new(sink),
            shared: Arc::new(Shared::new(capacity)),
            thread: Mutex::new(None),
        }
    }

    /// The sink.
    ///
    /// ```
    /// # use syrp_analytics::{AnalyticsEvent, BatchSink, ThreadedAnalyticsDestination};
    /// # struct S; impl BatchSink for S { fn log_batched_events(&self, _: &[AnalyticsEvent]) -> bool { true }
    /// # fn maximum_batch_size(&self) -> usize { 3 } fn stop_logging_events(&self) {} fn save_unlogged_events(&self, _: &[AnalyticsEvent]) {}
    /// # fn restore_unlogged_events(&self, _: &mut Vec<AnalyticsEvent>) {} }
    /// let d = ThreadedAnalyticsDestination::new("a", S);
    /// assert_eq!(d.sink().maximum_batch_size(), 3);
    /// ```
    #[must_use]
    pub fn sink(&self) -> &S {
        &self.sink
    }

    /// Events waiting to be sent, including a batch being sent right now.
    ///
    /// ```
    /// # use syrp_analytics::{AnalyticsEvent, BatchSink, ThreadedAnalyticsDestination};
    /// # struct S; impl BatchSink for S { fn log_batched_events(&self, _: &[AnalyticsEvent]) -> bool { true }
    /// # fn maximum_batch_size(&self) -> usize { 1 } fn stop_logging_events(&self) {} fn save_unlogged_events(&self, _: &[AnalyticsEvent]) {}
    /// # fn restore_unlogged_events(&self, _: &mut Vec<AnalyticsEvent>) {} }
    /// assert_eq!(ThreadedAnalyticsDestination::new("a", S).queued_event_count(), 0);
    /// ```
    #[must_use]
    pub fn queued_event_count(&self) -> usize {
        self.shared.lock().queue.len()
    }

    /// Events refused because the queue was full, since the last stop.
    ///
    /// ```
    /// # use syrp_analytics::{AnalyticsEvent, BatchSink, ThreadedAnalyticsDestination};
    /// # struct S; impl BatchSink for S { fn log_batched_events(&self, _: &[AnalyticsEvent]) -> bool { true }
    /// # fn maximum_batch_size(&self) -> usize { 1 } fn stop_logging_events(&self) {} fn save_unlogged_events(&self, _: &[AnalyticsEvent]) {}
    /// # fn restore_unlogged_events(&self, _: &mut Vec<AnalyticsEvent>) {} }
    /// assert_eq!(ThreadedAnalyticsDestination::new("a", S).dropped_event_count(), 0);
    /// ```
    #[must_use]
    pub fn dropped_event_count(&self) -> u64 {
        self.shared.lock().dropped
    }

    /// Restores saved events ([`BatchSink::restore_unlogged_events`], on this thread), then
    /// starts the analytics thread. The first batch is due one period after the start.
    ///
    /// # Errors
    /// - [`AnalyticsError::BatchPeriodTooShort`] (`SYRP-ANALYTICS-0003`): under 1 ms.
    /// - [`AnalyticsError::InvalidThreadName`] (`SYRP-ANALYTICS-0004`): the name has a NUL.
    /// - [`AnalyticsError::AlreadyRunning`] (`SYRP-ANALYTICS-0002`): a thread from an earlier
    ///   start has not exited, including one a timed-out stop left running.
    /// - [`AnalyticsError::ThreadSpawn`] (`SYRP-ANALYTICS-0001`): the OS refused the thread;
    ///   the queue, restored events included, is kept.
    ///
    /// ```
    /// # use std::time::Duration;
    /// # use syrp_analytics::{AnalyticsEvent, BatchSink, ThreadedAnalyticsDestination};
    /// # struct S; impl BatchSink for S { fn log_batched_events(&self, _: &[AnalyticsEvent]) -> bool { true }
    /// # fn maximum_batch_size(&self) -> usize { 1 } fn stop_logging_events(&self) {} fn save_unlogged_events(&self, _: &[AnalyticsEvent]) {}
    /// # fn restore_unlogged_events(&self, _: &mut Vec<AnalyticsEvent>) {} }
    /// let d = ThreadedAnalyticsDestination::new("a", S);
    /// d.start_analytics_thread(Duration::from_millis(500))?;
    /// assert!(d.start_analytics_thread(Duration::from_millis(500)).is_err());
    /// # Ok::<(), syrp_analytics::AnalyticsError>(())
    /// ```
    #[doc(alias = "startAnalyticsThread")]
    pub fn start_analytics_thread(
        &self,
        initial_batch_period: Duration,
    ) -> Result<(), AnalyticsError> {
        let base_ms = period_ms(initial_batch_period)?;
        if self.name.contains('\0') {
            return Err(AnalyticsError::InvalidThreadName);
        }
        let mut thread = self.thread.lock().unwrap_or_else(PoisonError::into_inner);
        {
            let st = self.shared.lock();
            if st.active != 0 || thread.as_ref().is_some_and(|t| !t.is_finished()) {
                return Err(AnalyticsError::AlreadyRunning);
            }
        }
        if let Some(old) = thread.take() {
            let _ = old.join();
        }
        let generation = self.shared.begin(&*self.sink, base_ms);
        let (shared, sink) = (Arc::clone(&self.shared), Arc::clone(&self.sink));
        let spawned = std::thread::Builder::new()
            .name(self.name.clone())
            .spawn(move || run(&shared, &*sink, generation));
        match spawned {
            Ok(handle) => {
                *thread = Some(handle);
                Ok(())
            }
            Err(e) => {
                self.shared.request_stop();
                self.shared.mark_finished(generation);
                Err(AnalyticsError::ThreadSpawn {
                    reason: e.to_string(),
                })
            }
        }
    }

    /// Changes the base batch period; any back-off is cleared and the next batch is due one
    /// new period from now. Any thread. Takes effect on the next start if stopped.
    ///
    /// # Errors
    /// [`AnalyticsError::BatchPeriodTooShort`] (`SYRP-ANALYTICS-0003`) for under 1 ms.
    ///
    /// ```
    /// # use std::time::Duration;
    /// # use syrp_analytics::{AnalyticsEvent, BatchSink, ThreadedAnalyticsDestination};
    /// # struct S; impl BatchSink for S { fn log_batched_events(&self, _: &[AnalyticsEvent]) -> bool { true }
    /// # fn maximum_batch_size(&self) -> usize { 1 } fn stop_logging_events(&self) {} fn save_unlogged_events(&self, _: &[AnalyticsEvent]) {}
    /// # fn restore_unlogged_events(&self, _: &mut Vec<AnalyticsEvent>) {} }
    /// let d = ThreadedAnalyticsDestination::new("a", S);
    /// d.set_batch_period(Duration::from_secs(5))?;
    /// assert!(d.set_batch_period(Duration::ZERO).is_err());
    /// # Ok::<(), syrp_analytics::AnalyticsError>(())
    /// ```
    #[doc(alias = "setBatchPeriod")]
    pub fn set_batch_period(&self, period: Duration) -> Result<(), AnalyticsError> {
        let ms = period_ms(period)?;
        let mut st = self.shared.lock();
        st.base_period_ms = ms;
        st.period_epoch += 1;
        drop(st);
        self.shared.wake.notify_all();
        Ok(())
    }

    /// Stops the thread: wakes it, calls [`BatchSink::stop_logging_events`] (if a thread
    /// was running), waits up to `timeout` for it to exit, then passes every unsent event,
    /// oldest first, to [`BatchSink::save_unlogged_events`] (if there is any). Safe to call
    /// when stopped; it then only saves what was queued.
    ///
    /// ```
    /// # use std::time::Duration;
    /// # use syrp_analytics::{AnalyticsDestination, AnalyticsEvent, BatchSink, ThreadedAnalyticsDestination};
    /// # struct S; impl BatchSink for S { fn log_batched_events(&self, _: &[AnalyticsEvent]) -> bool { true }
    /// # fn maximum_batch_size(&self) -> usize { 1 } fn stop_logging_events(&self) {} fn save_unlogged_events(&self, _: &[AnalyticsEvent]) {}
    /// # fn restore_unlogged_events(&self, _: &mut Vec<AnalyticsEvent>) {} }
    /// let d = ThreadedAnalyticsDestination::new("a", S);
    /// d.log_event(&AnalyticsEvent::new("unsent", 0, 0));
    /// assert_eq!(d.stop_analytics_thread(Duration::from_secs(1)).saved(), 1);
    /// ```
    #[doc(alias = "stopAnalyticsThread")]
    pub fn stop_analytics_thread(&self, timeout: Duration) -> StopReport {
        let mut thread = self.thread.lock().unwrap_or_else(PoisonError::into_inner);
        let generation = {
            let st = self.shared.lock();
            st.active
        };
        let mut timed_out = false;
        if generation != 0 {
            self.shared.request_stop();
            self.sink.stop_logging_events();
            let deadline = Instant::now().checked_add(timeout);
            let mut st = self.shared.lock();
            while st.finished < generation {
                let left = deadline.map_or(Duration::MAX, |d| {
                    d.saturating_duration_since(Instant::now())
                });
                if left.is_zero() {
                    timed_out = true;
                    break;
                }
                st = self
                    .shared
                    .wake
                    .wait_timeout(st, left)
                    .unwrap_or_else(PoisonError::into_inner)
                    .0;
            }
        }
        if !timed_out && let Some(handle) = thread.take() {
            let _ = handle.join();
        }
        let (events, dropped) = self.shared.take_unlogged();
        if !events.is_empty() {
            self.sink.save_unlogged_events(&events);
        }
        StopReport {
            saved: events.len(),
            dropped,
            timed_out,
        }
    }
}

impl<S: BatchSink> AnalyticsDestination for ThreadedAnalyticsDestination<S> {
    /// Queues a copy of `event` for the analytics thread; never waits for the sink.
    fn log_event(&self, event: &AnalyticsEvent) {
        self.shared.push(event);
    }
}

impl<S: BatchSink> Drop for ThreadedAnalyticsDestination<S> {
    /// Stops the thread with a [`DROP_STOP_TIMEOUT`] timeout and saves unsent events.
    fn drop(&mut self) {
        let _ = self.stop_analytics_thread(DROP_STOP_TIMEOUT);
    }
}

/// The analytics thread: steps the core on a real clock and sleeps on the condition
/// variable between steps, waking early for a stop or a period change.
fn run<S: BatchSink + ?Sized>(shared: &Shared, sink: &S, generation: u64) {
    /// Marks the generation finished even if the loop unwinds, so a stop never waits on a
    /// thread that is gone.
    struct Finish<'a>(&'a Shared, u64);
    impl Drop for Finish<'_> {
        fn drop(&mut self) {
            self.0.mark_finished(self.1);
        }
    }
    let _finish = Finish(shared, generation);
    let origin = Instant::now();
    let now = || u64::try_from(origin.elapsed().as_millis()).unwrap_or(u64::MAX);
    let mut batcher = {
        let st = shared.lock();
        Batcher::new(st.base_period_ms, st.period_epoch, now())
    };
    while let Step::WaitUntil(due) = step(shared, sink, &mut batcher, generation, now()) {
        let mut st = shared.lock();
        loop {
            let t = now();
            if st.active != generation || st.period_epoch != batcher.epoch() || t >= due {
                break;
            }
            st = shared
                .wake
                .wait_timeout(st, Duration::from_millis(due - t))
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }
}

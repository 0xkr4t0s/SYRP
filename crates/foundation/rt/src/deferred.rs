// SPDX-License-Identifier: MIT OR Apache-2.0
//! Deferred drop: the audio thread hands boxed values to a [`Collector`], which runs their
//! destructors (and frees their memory) on a non-RT thread.

use crate::RtSafe;
use crate::spsc::{self, Consumer, Producer};

/// A type-erased value waiting to be dropped.
type Garbage = Box<dyn Send>;

/// Owns the receiving side of every [`DeferredDrop`] handle it made, and drops what they send.
///
/// Lives on a non-RT thread (typically the message thread or a timer). Creating handles and
/// [`collect`](Self::collect)ing allocate and free; that is the point.
///
/// ```
/// use syrp_rt::Collector;
///
/// let mut collector = Collector::new();
/// let mut defer = collector.handle(16); // move this to the audio thread
/// defer.defer(Box::new(vec![0.0_f32; 512])).unwrap(); // no free here
/// assert_eq!(collector.collect(), 1); // the Vec is freed here
/// ```
#[derive(Default)]
#[doc(alias = "basedrop")]
pub struct Collector {
    queues: Vec<Consumer<Garbage>>,
}

impl Collector {
    /// Creates a collector with no handles.
    ///
    /// ```
    /// assert_eq!(syrp_rt::Collector::new().collect(), 0);
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a handle that can hold up to `capacity` values between two
    /// [`collect`](Self::collect) calls. Allocates; call it off the audio thread.
    ///
    /// ```
    /// let mut c = syrp_rt::Collector::new();
    /// let defer = c.handle(8);
    /// assert_eq!(defer.capacity(), 8);
    /// ```
    pub fn handle(&mut self, capacity: usize) -> DeferredDrop {
        let (tx, rx) = spsc::channel(capacity);
        self.queues.push(rx);
        DeferredDrop { tx }
    }

    /// Drops every value handed over so far, through any of this collector's handles, and
    /// returns how many it dropped. Never blocks: values handed over while it runs are either
    /// dropped now or on the next call. Queues of handles that have been dropped are freed
    /// once drained, so creating and dropping handles does not leak.
    ///
    /// ```
    /// let mut c = syrp_rt::Collector::new();
    /// let mut d = c.handle(2);
    /// d.defer(Box::new(1_u8)).unwrap();
    /// d.defer(Box::new(String::from("old preset"))).unwrap();
    /// assert_eq!(c.collect(), 2);
    /// assert_eq!(c.collect(), 0);
    /// ```
    pub fn collect(&mut self) -> usize {
        let mut dropped = 0;
        self.queues.retain_mut(|queue| {
            while let Some(garbage) = queue.try_pop() {
                drop(garbage);
                dropped += 1;
            }
            // Dropping an abandoned queue frees its ring here, off the audio thread; the ring's
            // own drop takes anything pushed after the drain above.
            !queue.is_abandoned()
        });
        dropped
    }
}

impl core::fmt::Debug for Collector {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Collector")
            .field("handles", &self.queues.len())
            .finish()
    }
}

/// The audio thread's side of a [`Collector`]: hands it boxed values to drop later.
///
/// [`defer`](Self::defer) and [`is_full`](Self::is_full) are RT-safe. One handle has one
/// sender; make one handle per thread with [`Collector::handle`]. Dropping the handle after its
/// collector is gone drops any values still queued, so drop it off the audio thread.
///
/// ```
/// let mut c = syrp_rt::Collector::new();
/// let mut defer = c.handle(1);
/// let old_table = Box::new([0.0_f32; 1024]);
/// assert!(defer.defer(old_table).is_ok());
/// ```
pub struct DeferredDrop {
    tx: Producer<Garbage>,
}

impl DeferredDrop {
    /// Hands `value` to the collector, or gives it back as `Err(value)` if this handle already
    /// holds `capacity` uncollected values. Never allocates or frees: the box is moved, not
    /// copied, and only its pointer enters the queue.
    ///
    /// ```
    /// let mut c = syrp_rt::Collector::new();
    /// let mut d = c.handle(1);
    /// assert!(d.defer(Box::new(1)).is_ok());
    /// assert_eq!(d.defer(Box::new(2)), Err(Box::new(2))); // full until collected
    /// ```
    pub fn defer<T: Send + 'static>(&mut self, value: Box<T>) -> Result<(), Box<T>> {
        // Checked first so the typed box can be returned: once coerced to `Box<dyn Send>` it
        // could not be given back as `Box<T>`. This handle is the only producer, so the queue
        // cannot fill up between the check and the push.
        if self.tx.is_full() {
            return Err(value);
        }
        let pushed = self.tx.try_push(value);
        debug_assert!(pushed.is_ok(), "single producer: the queue was not full");
        Ok(())
    }

    /// Whether [`defer`](Self::defer) would fail right now (until the next collect).
    ///
    /// ```
    /// let mut c = syrp_rt::Collector::new();
    /// assert!(c.handle(0).is_full());
    /// ```
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.tx.is_full()
    }

    /// How many values this handle can hold between collects.
    ///
    /// ```
    /// assert_eq!(syrp_rt::Collector::new().handle(4).capacity(), 4);
    /// ```
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.tx.capacity()
    }
}

impl core::fmt::Debug for DeferredDrop {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DeferredDrop")
            .field("capacity", &self.capacity())
            .finish()
    }
}

// SAFETY: `defer` and `is_full` only move a box pointer into a preallocated SPSC ring (see
// `Producer`); no destructor or deallocation runs on the calling thread.
unsafe impl RtSafe for DeferredDrop {}

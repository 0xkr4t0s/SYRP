// SPDX-License-Identifier: MIT OR Apache-2.0
//! `RtHandoff<T>`: publish a new configuration to an audio thread without locks, allocation or
//! deallocation on that thread.

use crate::sync::{Arc, AtomicPtr};
use crate::{DeferredDrop, RtSafe};
use core::marker::PhantomData;
use core::ptr;
use core::sync::atomic::Ordering::{AcqRel, Acquire};

/// The single slot between writer and reader: null, or a published value not yet taken.
struct Slot<T> {
    pending: AtomicPtr<T>,
    /// The slot owns a `T`, so it is `Send` only when `T` is (`AtomicPtr` alone is always).
    _owns: PhantomData<T>,
}

// SAFETY: through a shared slot, values of `T` only move between threads by atomic pointer
// swaps that hand over sole ownership; no `&T` is ever shared. That needs `T: Send`, not `Sync`.
unsafe impl<T: Send> Sync for Slot<T> {}

impl<T> Drop for Slot<T> {
    fn drop(&mut self) {
        let p = self.pending.swap(ptr::null_mut(), Acquire);
        if !p.is_null() {
            // SAFETY: a non-null `pending` came from `Box::into_raw` in `publish` and was never
            // taken by the reader (taking it swaps in null), so this is its only owner.
            drop(unsafe { Box::from_raw(p) });
        }
    }
}

/// The writer (non-RT) side of a single-slot handoff to an audio thread.
///
/// [`publish`](Self::publish) replaces whatever is waiting in the slot. The
/// [`HandoffReader`] on the audio thread picks up the latest value on its next
/// [`latest`](HandoffReader::latest) call and sends the value it replaces to a
/// [`DeferredDrop`], so the old value is freed by the [`Collector`](crate::Collector), never
/// on the audio thread.
///
/// ```
/// use syrp_rt::{Collector, RtHandoff};
///
/// let mut collector = Collector::new();
/// let (writer, mut reader) = RtHandoff::new(Box::new(vec![1.0_f32]), collector.handle(4));
/// writer.publish(Box::new(vec![0.5, 0.25]));
/// // On the audio thread:
/// assert_eq!(reader.latest(), &[0.5, 0.25]);
/// // Later, on the message thread:
/// assert_eq!(collector.collect(), 1); // frees the first Vec
/// ```
pub struct RtHandoff<T> {
    slot: Arc<Slot<T>>,
}

/// The reader (RT) side of an [`RtHandoff`].
///
/// [`latest`](Self::latest) and [`current`](Self::current) are RT-safe. Dropping the reader
/// drops the value it holds, so drop it off the audio thread.
///
/// ```
/// # use syrp_rt::{Collector, RtHandoff};
/// let mut c = Collector::new();
/// let (_writer, reader) = RtHandoff::new(Box::new(440.0_f64), c.handle(2));
/// assert_eq!(*reader.current(), 440.0);
/// ```
pub struct HandoffReader<T> {
    slot: Arc<Slot<T>>,
    current: Box<T>,
    defer: DeferredDrop,
}

impl<T: Send + 'static> RtHandoff<T> {
    /// Creates a handoff whose reader starts with `initial`. Replaced values go to `defer`;
    /// its capacity bounds how many swaps the reader can make between two collects.
    ///
    /// ```
    /// # use syrp_rt::{Collector, RtHandoff};
    /// let mut c = Collector::new();
    /// let (_w, mut r) = RtHandoff::new(Box::new("init"), c.handle(1));
    /// assert_eq!(*r.latest(), "init");
    /// ```
    pub fn new(initial: Box<T>, defer: DeferredDrop) -> (Self, HandoffReader<T>) {
        let slot = Arc::new(Slot {
            pending: AtomicPtr::new(ptr::null_mut()),
            _owns: PhantomData,
        });
        (
            Self {
                slot: Arc::clone(&slot),
            },
            HandoffReader {
                slot,
                current: initial,
                defer,
            },
        )
    }

    /// Makes `value` the latest value. If an earlier published value was never picked up by
    /// the reader, it is dropped here, on the writer's thread.
    ///
    /// ```
    /// # use syrp_rt::{Collector, RtHandoff};
    /// let mut c = Collector::new();
    /// let (w, mut r) = RtHandoff::new(Box::new(0), c.handle(1));
    /// w.publish(Box::new(1));
    /// w.publish(Box::new(2)); // 1 was never read: dropped here
    /// assert_eq!(*r.latest(), 2);
    /// ```
    pub fn publish(&self, value: Box<T>) {
        let old = self.slot.pending.swap(Box::into_raw(value), AcqRel);
        if !old.is_null() {
            // SAFETY: the swap took `old` out of the slot, so the reader can no longer take
            // it; it came from `Box::into_raw` in an earlier `publish`.
            drop(unsafe { Box::from_raw(old) });
        }
    }
}

impl<T: Send + 'static> HandoffReader<T> {
    /// Returns the most recently published value, switching to it if it is new.
    ///
    /// The value it replaces goes to the [`DeferredDrop`]. If that handle is full, the reader
    /// keeps its current value and leaves the new one in the slot for a later call, so nothing
    /// is ever freed here. Lock-free and wait-free: one atomic swap at most.
    ///
    /// ```
    /// # use syrp_rt::{Collector, RtHandoff};
    /// let mut c = Collector::new();
    /// let (w, mut r) = RtHandoff::new(Box::new(1), c.handle(1));
    /// w.publish(Box::new(2));
    /// assert_eq!(*r.latest(), 2); // the 1 is now in the full DeferredDrop
    /// w.publish(Box::new(3));
    /// assert_eq!(*r.latest(), 2); // cannot defer the 2 yet
    /// c.collect();
    /// assert_eq!(*r.latest(), 3);
    /// ```
    pub fn latest(&mut self) -> &T {
        if !self.defer.is_full() {
            let p = self.slot.pending.swap(ptr::null_mut(), AcqRel);
            if !p.is_null() {
                // SAFETY: the swap took sole ownership of `p`, which came from
                // `Box::into_raw` in `publish` (Acquire pairs with its AcqRel swap).
                let new = unsafe { Box::from_raw(p) };
                let old = core::mem::replace(&mut self.current, new);
                // Cannot fail: the handle was not full and this reader is its only sender.
                let deferred = self.defer.defer(old);
                debug_assert!(deferred.is_ok());
            }
        }
        &self.current
    }

    /// Returns the value the reader holds now, without looking for a newer one.
    ///
    /// ```
    /// # use syrp_rt::{Collector, RtHandoff};
    /// let mut c = Collector::new();
    /// let (w, r) = RtHandoff::new(Box::new('a'), c.handle(1));
    /// w.publish(Box::new('b'));
    /// assert_eq!(*r.current(), 'a');
    /// ```
    #[must_use]
    pub fn current(&self) -> &T {
        &self.current
    }
}

impl<T> core::fmt::Debug for RtHandoff<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RtHandoff").finish_non_exhaustive()
    }
}

impl<T> core::fmt::Debug for HandoffReader<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("HandoffReader").finish_non_exhaustive()
    }
}

// SAFETY: `latest` is one atomic swap, a pointer move and a `DeferredDrop::defer`, which is
// RT-safe; `current` is a field read. Neither frees memory (see the `DeferredDrop` impl).
unsafe impl<T: Send + 'static> RtSafe for HandoffReader<T> {}

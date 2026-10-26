// SPDX-License-Identifier: MIT OR Apache-2.0
//! A bounded, lock-free, single-producer single-consumer queue.
//!
//! [`channel`] allocates the ring once; after that [`Producer::try_push`] and
//! [`Consumer::try_pop`] never allocate, lock or block, so either end may live on an audio
//! thread. Values come out in the order they went in.
//!
//! ```
//! let (mut tx, mut rx) = syrp_rt::spsc::channel::<u32>(2);
//! assert_eq!(tx.try_push(1), Ok(()));
//! assert_eq!(tx.try_push(2), Ok(()));
//! assert_eq!(tx.try_push(3), Err(3)); // full: the value comes back
//! assert_eq!(rx.try_pop(), Some(1));
//! assert_eq!(rx.try_pop(), Some(2));
//! assert_eq!(rx.try_pop(), None); // empty
//! ```
//!
//! The implementation is in-crate rather than a wrapper around `rtrb`, so that the loom models
//! (`--cfg loom`) check the exact code that ships.

use crate::RtSafe;
use crate::sync::{Arc, AtomicUsize, UnsafeCell};
use core::mem::MaybeUninit;
use core::sync::atomic::Ordering::{Acquire, Relaxed, Release};

/// Shared ring storage.
///
/// `head` (written only by the consumer) and `tail` (written only by the producer) are
/// positions in `0..2 * capacity`. Counting to twice the capacity tells a full ring
/// (`len == capacity`) from an empty one (`head == tail`) without wasting a slot, and wrapping
/// at `2 * capacity` instead of at `usize::MAX` keeps `position % capacity` continuous.
struct Ring<T> {
    slots: Box<[UnsafeCell<MaybeUninit<T>>]>,
    head: AtomicUsize,
    tail: AtomicUsize,
}

// SAFETY: a slot is accessed by one side at a time. The producer writes slot `tail` only while
// it is outside the readable range `head..tail`, then publishes it with a Release store of
// `tail`; the consumer reads slot `head` only after an Acquire load of `tail` shows it is in
// range, then hands it back with a Release store of `head`, which the producer Acquire-loads
// before reusing it. Values of `T` cross threads, so `T: Send` is required.
unsafe impl<T: Send> Send for Ring<T> {}
// SAFETY: see the `Send` impl; shared access goes only through the protocol described there.
unsafe impl<T: Send> Sync for Ring<T> {}

impl<T> Ring<T> {
    fn capacity(&self) -> usize {
        self.slots.len()
    }

    /// Number of readable values between positions `head` and `tail`.
    fn distance(&self, head: usize, tail: usize) -> usize {
        if tail >= head {
            tail - head
        } else {
            2 * self.capacity() - (head - tail)
        }
    }

    fn next(&self, position: usize) -> usize {
        if position + 1 == 2 * self.capacity() {
            0
        } else {
            position + 1
        }
    }

    fn slot(&self, position: usize) -> &UnsafeCell<MaybeUninit<T>> {
        let cap = self.capacity();
        &self.slots[if position >= cap {
            position - cap
        } else {
            position
        }]
    }
}

impl<T> Drop for Ring<T> {
    fn drop(&mut self) {
        let (mut head, tail) = (self.head.load(Acquire), self.tail.load(Acquire));
        while head != tail {
            // SAFETY: both ends are gone (this is the last `Arc`), and every position in
            // `head..tail` holds a value that was written and never read.
            self.slot(head)
                .with_mut(|p| unsafe { (*p).assume_init_drop() });
            head = self.next(head);
        }
    }
}

/// Creates a queue that holds up to `capacity` values and returns its two ends.
///
/// This is the only call that allocates. A `capacity` of zero gives a queue that is always
/// full and always empty.
///
/// # Panics
/// If `capacity` is greater than `usize::MAX / 2`.
///
/// ```
/// let (tx, rx) = syrp_rt::spsc::channel::<f32>(64);
/// assert_eq!(tx.capacity(), 64);
/// assert_eq!(rx.capacity(), 64);
/// ```
#[must_use]
pub fn channel<T>(capacity: usize) -> (Producer<T>, Consumer<T>) {
    assert!(
        capacity <= usize::MAX / 2,
        "spsc capacity {capacity} is too large"
    );
    let ring = Arc::new(Ring {
        slots: (0..capacity)
            .map(|_| UnsafeCell::new(MaybeUninit::uninit()))
            .collect(),
        head: AtomicUsize::new(0),
        tail: AtomicUsize::new(0),
    });
    (
        Producer {
            ring: Arc::clone(&ring),
        },
        Consumer { ring },
    )
}

/// The sending end of a [`channel`]. `Send` but not `Clone`: there is one producer.
///
/// Its methods are RT-safe. Dropping it frees the ring (and any values still queued) if the
/// [`Consumer`] is already gone.
///
/// ```
/// let (mut tx, _rx) = syrp_rt::spsc::channel(1);
/// assert!(!tx.is_full());
/// tx.try_push("note on").unwrap();
/// assert!(tx.is_full());
/// ```
pub struct Producer<T> {
    ring: Arc<Ring<T>>,
}

impl<T> Producer<T> {
    /// Appends `value`, or gives it back as `Err(value)` if the queue is full.
    ///
    /// ```
    /// let (mut tx, _rx) = syrp_rt::spsc::channel(1);
    /// assert_eq!(tx.try_push(7), Ok(()));
    /// assert_eq!(tx.try_push(8), Err(8));
    /// ```
    pub fn try_push(&mut self, value: T) -> Result<(), T> {
        let ring = &*self.ring;
        let tail = ring.tail.load(Relaxed); // only this side writes `tail`
        // A zero-capacity ring has distance 0 == capacity, so it is always full here.
        if ring.distance(ring.head.load(Acquire), tail) == ring.capacity() {
            return Err(value);
        }
        // SAFETY: the queue is not full, so slot `tail` is outside `head..tail`: the consumer
        // has finished with it (Acquire above) and will not touch it until `tail` moves.
        ring.slot(tail).with_mut(|p| unsafe { (*p).write(value) });
        ring.tail.store(ring.next(tail), Release);
        Ok(())
    }

    /// Whether [`try_push`](Self::try_push) would fail right now. Only the consumer can change
    /// the answer from `true` to `false`.
    ///
    /// ```
    /// let (tx, _rx) = syrp_rt::spsc::channel::<u8>(0);
    /// assert!(tx.is_full());
    /// ```
    #[must_use]
    pub fn is_full(&self) -> bool {
        let ring = &*self.ring;
        ring.distance(ring.head.load(Acquire), ring.tail.load(Relaxed)) == ring.capacity()
    }

    /// The fixed number of values the queue can hold.
    ///
    /// ```
    /// assert_eq!(syrp_rt::spsc::channel::<u8>(3).0.capacity(), 3);
    /// ```
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.ring.capacity()
    }
}

/// The receiving end of a [`channel`]. `Send` but not `Clone`: there is one consumer.
///
/// Its methods are RT-safe. Dropping it frees the ring (and any values still queued) if the
/// [`Producer`] is already gone.
///
/// ```
/// let (mut tx, mut rx) = syrp_rt::spsc::channel(4);
/// tx.try_push(0.5_f32).unwrap();
/// assert!(!rx.is_empty());
/// assert_eq!(rx.try_pop(), Some(0.5));
/// ```
pub struct Consumer<T> {
    ring: Arc<Ring<T>>,
}

impl<T> Consumer<T> {
    /// Removes the oldest value, or returns `None` if the queue is empty.
    ///
    /// ```
    /// let (_tx, mut rx) = syrp_rt::spsc::channel::<i32>(4);
    /// assert_eq!(rx.try_pop(), None);
    /// ```
    pub fn try_pop(&mut self) -> Option<T> {
        let ring = &*self.ring;
        let head = ring.head.load(Relaxed); // only this side writes `head`
        if head == ring.tail.load(Acquire) {
            return None;
        }
        // SAFETY: `head != tail`, so slot `head` holds a value the producer published with a
        // Release store of `tail` (Acquire above). The producer does not reuse the slot until
        // `head` moves past it, so this is the only access, and it is read exactly once.
        let value = ring
            .slot(head)
            .with_mut(|p| unsafe { (*p).assume_init_read() });
        ring.head.store(ring.next(head), Release);
        Some(value)
    }

    /// Whether [`try_pop`](Self::try_pop) would return `None` right now. Only the producer can
    /// change the answer from `true` to `false`.
    ///
    /// ```
    /// let (_tx, rx) = syrp_rt::spsc::channel::<u8>(1);
    /// assert!(rx.is_empty());
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ring.head.load(Relaxed) == self.ring.tail.load(Acquire)
    }

    /// Whether the [`Producer`] has been dropped (this end holds the last reference).
    pub(crate) fn is_abandoned(&self) -> bool {
        Arc::strong_count(&self.ring) == 1
    }

    /// The fixed number of values the queue can hold.
    ///
    /// ```
    /// assert_eq!(syrp_rt::spsc::channel::<u8>(3).1.capacity(), 3);
    /// ```
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.ring.capacity()
    }
}

impl<T> core::fmt::Debug for Producer<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Producer")
            .field("capacity", &self.capacity())
            .finish_non_exhaustive()
    }
}

impl<T> core::fmt::Debug for Consumer<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Consumer")
            .field("capacity", &self.capacity())
            .finish_non_exhaustive()
    }
}

// SAFETY: `try_push` and `is_full` are a few atomic loads and stores plus a write into storage
// allocated by `channel`; moving the handle moves one pointer. Values pushed are `RtSafe`.
unsafe impl<T: RtSafe> RtSafe for Producer<T> {}
// SAFETY: as for `Producer`: atomic loads and stores and a read from preallocated storage.
unsafe impl<T: RtSafe> RtSafe for Consumer<T> {}

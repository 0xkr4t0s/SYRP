// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`SpinLock`]: a lock for very short critical sections that never sleeps in the kernel.

use std::fmt;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::Ordering;

use crate::sync::{AtomicBool, UnsafeCell, spin_loop, yield_now};

/// Busy-wait rounds before `lock` starts yielding. Each round spins twice as long as the one
/// before (1, 2, 4 ... 64 pause instructions). loom models skip straight to yielding.
const SPIN_ROUNDS: u32 = if cfg!(loom) { 0 } else { 7 };

/// A mutual-exclusion lock that spins instead of sleeping, protecting a `T`.
///
/// [`lock`](Self::lock) spins with exponential backoff, then yields the CPU between attempts;
/// it never blocks in the kernel, so the holder must keep the section short.
/// [`try_lock`](Self::try_lock) is a single atomic compare-and-swap and makes no system call.
/// The lock is released when the [`SpinLockGuard`] drops. It is not re-entrant: locking it
/// again on the same thread spins for ever. For anything longer than a few instructions,
/// prefer [`std::sync::Mutex`].
///
/// ```
/// use std::sync::Arc;
/// use syrp_thread::SpinLock;
/// let total = Arc::new(SpinLock::new(0u64));
/// let handles: Vec<_> = (0..4).map(|_| {
///     let total = Arc::clone(&total);
///     std::thread::spawn(move || for _ in 0..1000 { *total.lock() += 1 })
/// }).collect();
/// for h in handles { h.join().unwrap(); }
/// assert_eq!(*total.lock(), 4000);
/// ```
#[doc(alias = "juce::SpinLock")]
pub struct SpinLock<T = ()> {
    locked: AtomicBool,
    value: UnsafeCell<T>,
}

// SAFETY: the lock hands out access to `value` to one thread at a time (acquire on lock,
// release on unlock), so sharing the lock only needs `T` to be movable between threads.
unsafe impl<T: Send> Sync for SpinLock<T> {}

impl<T: Default> Default for SpinLock<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T> fmt::Debug for SpinLock<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SpinLock")
            .field("locked", &self.locked.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl<T> SpinLock<T> {
    /// Makes an unlocked lock around `value`. Use `SpinLock::new(())` for a bare lock.
    ///
    /// ```
    /// let lock = syrp_thread::SpinLock::new(());
    /// drop(lock.lock());
    /// ```
    #[cfg(not(loom))]
    pub const fn new(value: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            value: UnsafeCell::new(value),
        }
    }

    /// Makes an unlocked lock around `value` (loom build: not `const`).
    #[cfg(loom)]
    pub fn new(value: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            value: UnsafeCell::new(value),
        }
    }

    /// Takes the lock, spinning with backoff and then yielding until it is free.
    ///
    /// ```
    /// let lock = syrp_thread::SpinLock::new(vec![1]);
    /// lock.lock().push(2);
    /// assert_eq!(*lock.lock(), [1, 2]);
    /// ```
    #[doc(alias = "enter")]
    // Under loom `SPIN_ROUNDS` is 0, so the spin branch is dead on purpose.
    #[cfg_attr(loom, allow(clippy::absurd_extreme_comparisons))]
    pub fn lock(&self) -> SpinLockGuard<'_, T> {
        loop {
            if let Some(guard) = self.try_lock() {
                return guard;
            }
            // Wait for the lock to look free with plain loads, so waiters do not keep the
            // cache line bouncing with failed compare-and-swaps.
            let mut round = 0;
            while self.locked.load(Ordering::Relaxed) {
                if round < SPIN_ROUNDS {
                    for _ in 0..(1u32 << round) {
                        spin_loop();
                    }
                    round += 1;
                } else {
                    yield_now();
                }
            }
        }
    }

    /// Takes the lock if it is free, without waiting and without a system call.
    ///
    /// ```
    /// let lock = syrp_thread::SpinLock::new(());
    /// let held = lock.try_lock().unwrap();
    /// assert!(lock.try_lock().is_none());
    /// drop(held);
    /// assert!(lock.try_lock().is_some());
    /// ```
    #[doc(alias = "tryEnter")]
    #[must_use = "the lock is released as soon as the guard drops"]
    pub fn try_lock(&self) -> Option<SpinLockGuard<'_, T>> {
        self.locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
            // Lazily: a guard built on failure would unlock someone else's lock when dropped.
            .then(|| SpinLockGuard { lock: self })
    }

    /// Consumes the lock and returns the value.
    ///
    /// ```
    /// assert_eq!(syrp_thread::SpinLock::new(7).into_inner(), 7);
    /// ```
    pub fn into_inner(self) -> T {
        self.value.into_inner()
    }
}

/// Holds a [`SpinLock`] and gives access to its value; dropping it releases the lock.
///
/// ```
/// let lock = syrp_thread::SpinLock::new(String::new());
/// {
///     let mut s = lock.lock();
///     s.push_str("held");
/// } // released here
/// assert_eq!(*lock.lock(), "held");
/// ```
#[doc(alias = "juce::SpinLock::ScopedLockType")]
#[must_use = "the lock is released as soon as the guard drops"]
pub struct SpinLockGuard<'a, T> {
    lock: &'a SpinLock<T>,
}

impl<T> Deref for SpinLockGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: this guard holds the lock, so no other thread touches `value` until it
        // drops, and the returned borrow cannot outlive the guard.
        self.lock.value.with_mut(|p| unsafe { &*p })
    }
}

impl<T> DerefMut for SpinLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: as in `deref`; `&mut self` also rules out other borrows through this guard.
        self.lock.value.with_mut(|p| unsafe { &mut *p })
    }
}

impl<T> Drop for SpinLockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
    }
}

impl<T: fmt::Debug> fmt::Debug for SpinLockGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`WaitableEvent`]: a flag that threads can wait on, in auto-reset or manual-reset mode.

use std::sync::PoisonError;
use std::time::{Duration, Instant};

use crate::sync::{Condvar, Mutex};

/// A signal that one thread raises and others wait for.
///
/// An auto-reset event (`WaitableEvent::new(false)`, also the [`Default`]) lets exactly one
/// waiter through per signal and clears itself as that waiter returns. A manual-reset event
/// (`new(true)`) lets every waiter through until [`reset`](Self::reset). A signal raised
/// before anyone waits is kept, not lost. Signals do not count: signalling twice before a wait
/// lets one auto-reset waiter through, not two.
///
/// ```
/// use std::sync::Arc;
/// use syrp_thread::WaitableEvent;
/// let ready = Arc::new(WaitableEvent::new(false));
/// let r = Arc::clone(&ready);
/// let t = std::thread::spawn(move || r.wait(None));
/// ready.signal();
/// assert!(t.join().unwrap());
/// ```
#[doc(alias = "juce::WaitableEvent")]
#[derive(Debug)]
pub struct WaitableEvent {
    manual_reset: bool,
    signalled: Mutex<bool>,
    cond: Condvar,
}

impl Default for WaitableEvent {
    /// An auto-reset event, not signalled.
    fn default() -> Self {
        Self::new(false)
    }
}

impl WaitableEvent {
    /// Makes an event that is not signalled. With `manual_reset` false, each successful
    /// [`wait`](Self::wait) clears the signal; with true, only [`reset`](Self::reset) does.
    ///
    /// ```
    /// let e = syrp_thread::WaitableEvent::new(true);
    /// e.signal();
    /// assert!(e.wait(None) && e.wait(None)); // manual-reset stays signalled
    /// ```
    pub fn new(manual_reset: bool) -> Self {
        Self {
            manual_reset,
            signalled: Mutex::new(false),
            cond: Condvar::new(),
        }
    }

    /// Waits until the event is signalled, or until `timeout` passes (`None` waits for ever).
    /// Returns `true` if the event was signalled and `false` on timeout. `Some(Duration::ZERO)`
    /// only checks, without blocking.
    ///
    /// ```
    /// use std::time::Duration;
    /// let e = syrp_thread::WaitableEvent::new(false);
    /// assert!(!e.wait(Some(Duration::from_millis(1))));
    /// e.signal();
    /// assert!(e.wait(Some(Duration::ZERO)));
    /// assert!(!e.wait(Some(Duration::ZERO))); // auto-reset cleared it
    /// ```
    #[must_use = "false means the wait timed out"]
    pub fn wait(&self, timeout: Option<Duration>) -> bool {
        // A timeout too large for `Instant` is the same as no timeout.
        let deadline = timeout.and_then(|t| Instant::now().checked_add(t));
        let mut signalled = self
            .signalled
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        loop {
            if *signalled {
                if !self.manual_reset {
                    *signalled = false;
                }
                return true;
            }
            signalled = match (timeout, deadline) {
                (Some(_), Some(d)) => {
                    let left = d.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return false;
                    }
                    let waited = self.cond.wait_timeout(signalled, left);
                    waited.unwrap_or_else(PoisonError::into_inner).0
                }
                _ => self
                    .cond
                    .wait(signalled)
                    .unwrap_or_else(PoisonError::into_inner),
            };
        }
    }

    /// Signals the event: wakes one waiter (auto-reset) or all waiters (manual-reset). With
    /// nobody waiting, the signal stays until the next wait (or [`reset`](Self::reset)).
    ///
    /// ```
    /// let e = syrp_thread::WaitableEvent::default();
    /// e.signal(); // nobody waiting yet: kept
    /// assert!(e.wait(None));
    /// ```
    pub fn signal(&self) {
        let mut signalled = self
            .signalled
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *signalled = true;
        if self.manual_reset {
            self.cond.notify_all();
        } else {
            self.cond.notify_one();
        }
    }

    /// Clears the signal, so later waits block until the next [`signal`](Self::signal).
    ///
    /// ```
    /// use std::time::Duration;
    /// let e = syrp_thread::WaitableEvent::new(true);
    /// e.signal();
    /// e.reset();
    /// assert!(!e.wait(Some(Duration::ZERO)));
    /// ```
    pub fn reset(&self) {
        *self
            .signalled
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = false;
    }
}

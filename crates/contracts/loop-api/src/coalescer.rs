// SPDX-License-Identifier: MIT OR Apache-2.0
//! `Coalescer`: collapse many triggers into one message-thread callback.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use crate::{MessageThread, Task, TaskSender};

type Callback = Box<dyn FnMut(&MessageThread<'_>) + Send>;

struct Inner {
    pending: AtomicBool,
    callback: Mutex<Callback>,
    sender: TaskSender,
}

/// Collapses many [`trigger`](Coalescer::trigger) calls into one callback on the message
/// thread: JUCE's AsyncUpdater.
///
/// `trigger` is cheap and callable from any thread (the coalescer is `Send + Sync`; share it
/// with an `Arc`). The first trigger posts one task; later triggers do nothing until that task
/// has run the callback. A trigger made during the callback schedules one more run. Dropping
/// the coalescer cancels a pending update.
/// ```
/// use std::sync::Arc;
/// use std::sync::atomic::{AtomicUsize, Ordering};
/// use syrp_loop_api::{Coalescer, ManualLoop, MessageLoop};
/// let lp = ManualLoop::new();
/// let calls = Arc::new(AtomicUsize::new(0));
/// let c = calls.clone();
/// let up = Coalescer::new(lp.task_sender(), move |_| _ = c.fetch_add(1, Ordering::Relaxed));
/// (0..100).for_each(|_| up.trigger());
/// lp.run_until_idle().unwrap();
/// assert_eq!(calls.load(Ordering::Relaxed), 1);
/// ```
#[doc(alias = "AsyncUpdater")]
pub struct Coalescer {
    inner: Arc<Inner>,
}

impl Coalescer {
    /// A coalescer that posts through `sender` and runs `f` on the message thread.
    /// ```
    /// # use syrp_loop_api::{Coalescer, ManualLoop, MessageLoop};
    /// let c = Coalescer::new(ManualLoop::new().task_sender(), |_mt| println!("update"));
    /// assert!(!c.is_pending());
    /// ```
    pub fn new(sender: TaskSender, f: impl FnMut(&MessageThread<'_>) + Send + 'static) -> Self {
        let (pending, callback) = (AtomicBool::new(false), Mutex::new(Box::new(f) as Callback));
        Coalescer {
            inner: Arc::new(Inner {
                pending,
                callback,
                sender,
            }),
        }
    }

    /// Requests an update. Any thread; never blocks and never runs the callback directly.
    /// ```
    /// # use syrp_loop_api::{Coalescer, ManualLoop, MessageLoop};
    /// let lp = ManualLoop::new();
    /// let c = Coalescer::new(lp.task_sender(), |_| {});
    /// (c.trigger(), c.trigger());
    /// assert_eq!(lp.pending_tasks(), 1);
    /// ```
    #[doc(alias = "triggerAsyncUpdate")]
    pub fn trigger(&self) {
        post_if_idle(&self.inner);
    }

    /// Whether an update is requested and has not run yet.
    /// ```
    /// # use syrp_loop_api::{Coalescer, ManualLoop, MessageLoop};
    /// let lp = ManualLoop::new();
    /// let c = Coalescer::new(lp.task_sender(), |_| {});
    /// c.trigger();
    /// assert!(c.is_pending());
    /// ```
    #[doc(alias = "isUpdatePending")]
    #[must_use]
    pub fn is_pending(&self) -> bool {
        self.inner.pending.load(Ordering::Acquire)
    }

    /// Withdraws a pending update; the already-posted task then does nothing.
    /// ```
    /// # use syrp_loop_api::{Coalescer, ManualLoop, MessageLoop};
    /// let lp = ManualLoop::new();
    /// let c = Coalescer::new(lp.task_sender(), |_| panic!("cancelled"));
    /// c.trigger();
    /// c.cancel();
    /// lp.run_until_idle().unwrap();
    /// ```
    #[doc(alias = "cancelPendingUpdate")]
    pub fn cancel(&self) {
        self.inner.pending.store(false, Ordering::Release);
    }
}

impl Drop for Coalescer {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Posts the update task unless one is already pending.
fn post_if_idle(inner: &Arc<Inner>) {
    if inner.pending.swap(true, Ordering::AcqRel) {
        return;
    }
    // A weak reference: a dropped coalescer must not be kept alive by its queued task.
    let weak = Arc::downgrade(inner);
    inner.sender.post(Task::new(move |mt| {
        if let Some(inner) = weak.upgrade() {
            run_if_pending(&inner, mt);
        }
    }));
}

/// Runs the callback if an update is still pending (it may have been cancelled).
fn run_if_pending(inner: &Inner, mt: &MessageThread<'_>) {
    if inner.pending.swap(false, Ordering::AcqRel) {
        (inner
            .callback
            .lock()
            .unwrap_or_else(PoisonError::into_inner))(mt);
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0
//! `ManualLoop`: the deterministic loop with a virtual clock.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

use crate::{
    LoopError, MessageLoop, MessageThread, PeerId, Task, TaskSender, TimerFn, TimerGuard, VBlankFn,
    VBlankGuard,
};

const DEFAULT_TASK_LIMIT: usize = 10_000;
/// Shorter timer periods are raised to this, so a zero period cannot stall `advance`.
const MIN_PERIOD: Duration = Duration::from_millis(1);

type Queue = Mutex<VecDeque<Task>>;

/// A registered timer (`kind` = period) or VBlank callback (`kind` = peer). `f` is `None`
/// while the callback runs.
struct Entry<K, F> {
    id: u64,
    kind: K,
    due: Duration,
    f: Option<F>,
}

type Entries<K, F> = RefCell<Vec<Entry<K, F>>>;

struct Inner {
    queue: Arc<Queue>,
    sender: TaskSender,
    now: Cell<Duration>,
    task_limit: Cell<usize>,
    next_id: Cell<u64>,
    timers: Entries<Duration, TimerFn>,
    vblanks: Entries<PeerId, VBlankFn>,
}

/// A deterministic [`MessageLoop`] for tests, driven by hand on a virtual clock.
///
/// Nothing happens on its own: [`advance`](ManualLoop::advance) moves the clock and fires due
/// timers, [`run_until_idle`](ManualLoop::run_until_idle) runs posted tasks and
/// [`vblank`](ManualLoop::vblank) simulates a display refresh. The creating thread is its
/// message thread; the loop is `!Send`. Clones share one loop. Timer periods below 1 ms are
/// raised to 1 ms.
/// ```
/// use std::time::Duration;
/// use syrp_loop_api::{ManualLoop, MessageLoop, Task};
/// let lp = ManualLoop::new();
/// let _t = lp.timer(Duration::from_millis(30), Box::new(|_mt| {}));
/// assert_eq!(lp.advance(Duration::from_millis(100)), 3);
/// lp.post(Task::new(|_mt| {}));
/// assert_eq!(lp.run_until_idle(), Ok(1));
/// ```
#[derive(Clone)]
pub struct ManualLoop {
    inner: Rc<Inner>,
}

impl ManualLoop {
    /// A loop at virtual time zero, owned by the calling thread.
    /// ```
    /// assert_eq!(syrp_loop_api::ManualLoop::new().now(), std::time::Duration::ZERO);
    /// ```
    #[must_use]
    pub fn new() -> Self {
        let queue = Arc::<Queue>::default();
        let weak = Arc::downgrade(&queue);
        // Once the loop is gone a task is dropped, which wakes any post_and_wait caller.
        let sender = TaskSender::new(thread::current().id(), move |task| {
            if let Some(queue) = weak.upgrade() {
                lock(&queue).push_back(task);
            }
        });
        let inner = Inner {
            queue,
            sender,
            now: Cell::default(),
            task_limit: Cell::new(DEFAULT_TASK_LIMIT),
            next_id: Cell::default(),
            timers: RefCell::default(),
            vblanks: RefCell::default(),
        };
        ManualLoop {
            inner: Rc::new(inner),
        }
    }

    /// The virtual time since the loop was created.
    /// ```
    /// # use std::time::Duration;
    /// let lp = syrp_loop_api::ManualLoop::new();
    /// lp.advance(Duration::from_secs(2));
    /// assert_eq!(lp.now(), Duration::from_secs(2));
    /// ```
    #[must_use]
    pub fn now(&self) -> Duration {
        self.inner.now.get()
    }

    /// Sets how many tasks one [`run_until_idle`](ManualLoop::run_until_idle) call may run
    /// (default 10,000).
    /// ```
    /// syrp_loop_api::ManualLoop::new().set_task_limit(100);
    /// ```
    pub fn set_task_limit(&self, limit: usize) {
        self.inner.task_limit.set(limit);
    }

    /// The number of posted tasks waiting to run.
    /// ```
    /// assert_eq!(syrp_loop_api::ManualLoop::new().pending_tasks(), 0);
    /// ```
    #[must_use]
    pub fn pending_tasks(&self) -> usize {
        lock(&self.inner.queue).len()
    }

    /// Runs `f` with a message-thread token. Safe because the loop never leaves its thread.
    /// ```
    /// assert_eq!(syrp_loop_api::ManualLoop::new().with_token(|_mt| 42), 42);
    /// ```
    pub fn with_token<R>(&self, f: impl FnOnce(&MessageThread<'_>) -> R) -> R {
        f(&MessageThread::new())
    }

    /// Runs posted tasks in order until the queue is empty, including tasks posted meanwhile,
    /// and returns how many ran. Does not move the clock.
    ///
    /// # Errors
    /// [`LoopError::TaskLimitReached`] when the task limit is reached with more queued. The rest stay queued.
    /// ```
    /// use syrp_loop_api::{LoopError, ManualLoop, MessageLoop, Task};
    /// let lp = ManualLoop::new();
    /// lp.set_task_limit(1);
    /// lp.post(Task::new(|_| {}));
    /// lp.post(Task::new(|_| {}));
    /// assert_eq!(lp.run_until_idle(), Err(LoopError::TaskLimitReached));
    /// assert_eq!(lp.pending_tasks(), 1);
    /// ```
    pub fn run_until_idle(&self) -> Result<usize, LoopError> {
        let mt = MessageThread::new();
        let mut ran = 0;
        // The lock is released before each task runs, so tasks can post more work.
        loop {
            let Some(task) = lock(&self.inner.queue).pop_front() else {
                return Ok(ran);
            };
            if ran >= self.inner.task_limit.get() {
                lock(&self.inner.queue).push_front(task);
                return Err(LoopError::TaskLimitReached);
            }
            task.run(&mt);
            ran += 1;
        }
    }

    /// Moves the virtual clock forward by `by`, firing due timers in time order (ties in
    /// creation order) with [`now`](ManualLoop::now) at each due time. A timer created in a
    /// callback first falls due one period later. Returns the number fired; runs no tasks.
    /// ```
    /// # use std::time::Duration; use syrp_loop_api::{ManualLoop, MessageLoop};
    /// let lp = ManualLoop::new();
    /// let _t = lp.timer(Duration::from_millis(30), Box::new(|_| {}));
    /// assert_eq!(lp.advance(Duration::from_millis(100)), 3);
    /// assert_eq!(lp.advance(Duration::from_millis(20)), 1); // the one at 120 ms
    /// ```
    pub fn advance(&self, by: Duration) -> usize {
        let target = self.now() + by;
        let mt = MessageThread::new();
        let mut fired = 0;
        loop {
            let next = (self.inner.timers.borrow().iter())
                .filter(|t| t.f.is_some() && t.due <= target)
                .min_by_key(|t| (t.due, t.id))
                .map(|t| t.id);
            let Some(id) = next else { break };
            let Some((due, mut f)) = take(&self.inner.timers, id) else {
                break;
            };
            self.inner.now.set(due);
            f(&mt);
            fired += 1;
            put_back(&self.inner.timers, id, f, |t| t.due = due + t.kind);
        }
        self.inner.now.set(target);
        fired
    }

    /// Simulates one display refresh of `peer`: calls its VBlank callbacks in attach order at
    /// the current virtual time. Ones attached meanwhile wait for the next refresh. Returns
    /// the number called.
    /// ```
    /// # use syrp_loop_api::{ManualLoop, MessageLoop, PeerId};
    /// let lp = ManualLoop::new();
    /// let _v = lp.on_vblank(PeerId::from_raw(4), Box::new(|_, _| {}));
    /// assert_eq!(lp.vblank(PeerId::from_raw(5)), 0);
    /// ```
    pub fn vblank(&self, peer: PeerId) -> usize {
        let ids: Vec<u64> = (self.inner.vblanks.borrow().iter())
            .filter(|v| v.kind == peer)
            .map(|v| v.id)
            .collect();
        let (mt, now) = (MessageThread::new(), self.now());
        let mut called = 0;
        for id in ids {
            let Some((_, mut f)) = take(&self.inner.vblanks, id) else {
                continue;
            };
            f(&mt, now);
            called += 1;
            put_back(&self.inner.vblanks, id, f, |_| {});
        }
        called
    }

    fn register<K, F>(
        &self,
        list: fn(&Inner) -> &Entries<K, F>,
        kind: K,
        due: Duration,
        f: F,
    ) -> Box<dyn FnOnce()>
    where
        K: 'static,
        F: 'static,
    {
        let id = self.inner.next_id.get();
        self.inner.next_id.set(id + 1);
        list(&self.inner).borrow_mut().push(Entry {
            id,
            kind,
            due,
            f: Some(f),
        });
        let weak = Rc::downgrade(&self.inner);
        Box::new(move || {
            let Some(inner) = weak.upgrade() else { return };
            let removed = {
                let mut entries = list(&inner).borrow_mut();
                let index = entries.iter().position(|e| e.id == id);
                index.map(|i| entries.remove(i))
            };
            // Dropping a callback may drop other guards, which borrow the list again.
            drop(removed);
        })
    }
}

/// Takes an idle entry's callback out so it can run without a borrow held.
fn take<K, F>(list: &Entries<K, F>, id: u64) -> Option<(Duration, F)> {
    let mut entries = list.borrow_mut();
    let entry = entries.iter_mut().find(|e| e.id == id)?;
    Some((entry.due, entry.f.take()?))
}

/// Returns a callback after it ran, unless its guard was dropped meanwhile.
fn put_back<K, F>(list: &Entries<K, F>, id: u64, f: F, update: impl FnOnce(&mut Entry<K, F>)) {
    let mut entries = list.borrow_mut();
    if let Some(entry) = entries.iter_mut().find(|e| e.id == id) {
        update(entry);
        entry.f = Some(f);
    } else {
        drop(entries); // drop the cancelled callback outside the borrow
        drop(f);
    }
}

impl Default for ManualLoop {
    fn default() -> Self {
        Self::new()
    }
}

impl MessageLoop for ManualLoop {
    fn task_sender(&self) -> TaskSender {
        self.inner.sender.clone()
    }

    fn timer(&self, period: Duration, f: TimerFn) -> TimerGuard {
        let period = period.max(MIN_PERIOD);
        TimerGuard::new(self.register(|i| &i.timers, period, self.now() + period, f))
    }

    fn on_vblank(&self, peer: PeerId, f: VBlankFn) -> VBlankGuard {
        VBlankGuard::new(self.register(|i| &i.vblanks, peer, Duration::ZERO, f))
    }
}

/// Locks the queue, ignoring poisoning: a panicking task never leaves it half-updated.
fn lock(queue: &Queue) -> MutexGuard<'_, VecDeque<Task>> {
    queue.lock().unwrap_or_else(PoisonError::into_inner)
}

// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The platform-neutral core of [`EventLoop`]: the task queue, the timer schedule and the
//! dispatch pass. The `os` module supplies the waker and the blocking wait.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use syrp_loop_api::{
    CallbackGuard, MessageLoop, MessageThread, PeerId, TaskSender, TimerFn, TimerGuard, VBlankFn,
    VBlankGuard,
};

use crate::EventsError;
use crate::os;

/// Shorter timer periods are raised to this, as in `ManualLoop`.
const MIN_PERIOD: Duration = Duration::from_millis(1);
/// The fallback VBlank rate (60 Hz) used until a peer drives refreshes itself.
const FALLBACK_REFRESH: Duration = Duration::from_nanos(16_666_667);

thread_local! {
    /// Whether the current thread already has an `EventLoop` (at most one per thread).
    static HAS_LOOP: Cell<bool> = const { Cell::new(false) };
}

/// The any-thread half of a loop: posted tasks, the quit flag and the waker.
pub(crate) struct Shared {
    queue: Mutex<VecDeque<syrp_loop_api::Task>>,
    quit: AtomicBool,
    pub(crate) waker: os::Waker,
}

impl Shared {
    fn queue(&self) -> MutexGuard<'_, VecDeque<syrp_loop_api::Task>> {
        // A panicking task runs outside the lock, so the queue is never half-updated.
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

enum Callback {
    Timer(TimerFn),
    VBlank(VBlankFn),
}

/// A registered timer. `f` is `None` while the callback runs.
struct Slot {
    period: Duration,
    f: Option<Callback>,
}

/// Timers by id, plus the schedule ordered by (due time, creation id).
#[derive(Default)]
struct Timers {
    slots: HashMap<u64, Slot>,
    order: BTreeSet<(Duration, u64)>,
}

pub(crate) struct Inner {
    shared: Arc<Shared>,
    sender: TaskSender,
    poller: os::Poller,
    epoch: Instant,
    timers: RefCell<Timers>,
    next_id: Cell<u64>,
}

/// The owned message loop of the calling thread (master plan §5.5, kind 1).
///
/// Create it on the thread that becomes its message thread; it is `!Send` and there is at most
/// one per thread. It implements [`MessageLoop`]: other threads post through
/// [`task_sender`](MessageLoop::task_sender) and stop it through a [`QuitHandle`]. Timers run
/// on the monotonic clock with absolute deadlines. A timer that falls behind (the thread was
/// blocked) fires once and keeps its phase: unlike `ManualLoop::advance`, it never fires a
/// burst to catch up.
///
/// Backends are interim for now: a `kqueue` on macOS and a condition variable on Linux and
/// Windows. The `epoll`, `CFRunLoop` and Win32 backends follow.
/// ```
/// use std::time::Duration;
/// use syrp_events::EventLoop;
/// use syrp_loop_api::{MessageLoop, Task};
/// let lp = EventLoop::new()?;
/// let quit = lp.quit_handle();
/// lp.post(Task::new(move |_mt| quit.quit()));
/// lp.run();
/// # Ok::<(), syrp_events::EventsError>(())
/// ```
#[doc(alias = "MessageManager")]
pub struct EventLoop {
    inner: Rc<Inner>,
}

/// A `Send + Sync` handle that stops [`EventLoop::run`] from any thread.
///
/// A quit requested while the loop is not running ends the next `run` or `run_for` at once.
/// ```
/// let lp = syrp_events::EventLoop::new()?;
/// let quit = lp.quit_handle();
/// std::thread::spawn(move || quit.quit()).join().unwrap();
/// lp.run(); // returns at once: the quit was already requested
/// # Ok::<(), syrp_events::EventsError>(())
/// ```
#[derive(Clone)]
pub struct QuitHandle {
    shared: std::sync::Weak<Shared>,
}

impl QuitHandle {
    /// Asks the loop to return from `run` or `run_for`; does nothing if the loop is gone.
    /// ```
    /// let lp = syrp_events::EventLoop::new()?;
    /// lp.quit_handle().quit();
    /// assert!(lp.run_for(std::time::Duration::from_secs(5)));
    /// # Ok::<(), syrp_events::EventsError>(())
    /// ```
    pub fn quit(&self) {
        if let Some(shared) = self.shared.upgrade() {
            shared.quit.store(true, Ordering::SeqCst);
            shared.waker.wake();
        }
    }
}

impl EventLoop {
    /// Creates the calling thread's loop; the calling thread becomes its message thread.
    ///
    /// # Errors
    /// [`EventsError::LoopAlreadyOnThread`] if this thread has a loop already,
    /// [`EventsError::Os`] if an OS call fails, and [`EventsError::UnsupportedTarget`] on
    /// targets without an owned loop.
    /// ```
    /// use syrp_events::{EventLoop, EventsError};
    /// let lp = EventLoop::new()?;
    /// assert!(matches!(EventLoop::new(), Err(EventsError::LoopAlreadyOnThread)));
    /// drop(lp);
    /// let _again = EventLoop::new()?; // allowed once the first is gone
    /// # Ok::<(), EventsError>(())
    /// ```
    pub fn new() -> Result<Self, EventsError> {
        if HAS_LOOP.get() {
            return Err(EventsError::LoopAlreadyOnThread);
        }
        let shared = Arc::new(Shared {
            queue: Mutex::default(),
            quit: AtomicBool::new(false),
            waker: os::Waker::new()?,
        });
        let poller = os::Poller::new(&shared.waker)?;
        let weak = Arc::downgrade(&shared);
        // Once the loop is gone a task is dropped unrun, which wakes any post_and_wait caller.
        let sender = TaskSender::new(thread::current().id(), move |task| {
            if let Some(shared) = weak.upgrade() {
                shared.queue().push_back(task);
                shared.waker.wake();
            }
        });
        HAS_LOOP.set(true);
        let inner = Inner {
            shared,
            sender,
            poller,
            epoch: Instant::now(),
            timers: RefCell::default(),
            next_id: Cell::new(0),
        };
        Ok(EventLoop {
            inner: Rc::new(inner),
        })
    }

    /// Time since the loop was created, on the monotonic clock its timers use.
    /// ```
    /// let lp = syrp_events::EventLoop::new()?;
    /// assert!(lp.now() < std::time::Duration::from_secs(60));
    /// # Ok::<(), syrp_events::EventsError>(())
    /// ```
    #[must_use]
    pub fn now(&self) -> Duration {
        self.inner.epoch.elapsed()
    }

    /// A handle that stops `run` and `run_for` from any thread.
    /// ```
    /// fn is_send_sync<T: Send + Sync>(_: &T) {}
    /// is_send_sync(&syrp_events::EventLoop::new()?.quit_handle());
    /// # Ok::<(), syrp_events::EventsError>(())
    /// ```
    #[must_use]
    pub fn quit_handle(&self) -> QuitHandle {
        QuitHandle {
            shared: Arc::downgrade(&self.inner.shared),
        }
    }

    /// Runs tasks, timers and other sources until a [`QuitHandle`] asks it to stop.
    /// ```
    /// use syrp_loop_api::{MessageLoop, Task};
    /// let lp = syrp_events::EventLoop::new()?;
    /// let quit = lp.quit_handle();
    /// let _t = lp.timer(std::time::Duration::from_millis(5), Box::new(move |_| quit.quit()));
    /// lp.run();
    /// # Ok::<(), syrp_events::EventsError>(())
    /// ```
    pub fn run(&self) {
        while !self.take_quit() {
            self.pass(None);
        }
    }

    /// Runs the loop for `duration`, or until a quit; returns whether a quit ended it.
    /// ```
    /// let lp = syrp_events::EventLoop::new()?;
    /// assert!(!lp.run_for(std::time::Duration::from_millis(5)));
    /// # Ok::<(), syrp_events::EventsError>(())
    /// ```
    pub fn run_for(&self, duration: Duration) -> bool {
        let deadline = self.now() + duration;
        loop {
            if self.take_quit() {
                return true;
            }
            if self.now() >= deadline {
                return false;
            }
            self.pass(Some(deadline));
        }
    }

    /// One pass that never blocks: ready OS sources, the tasks queued when it starts, then the
    /// due timers. Returns how many callbacks ran. For embedding in a foreign loop.
    /// ```
    /// use syrp_loop_api::{MessageLoop, Task};
    /// let lp = syrp_events::EventLoop::new()?;
    /// lp.post(Task::new(|_mt| {}));
    /// assert_eq!(lp.dispatch_pending(), 1);
    /// # Ok::<(), syrp_events::EventsError>(())
    /// ```
    pub fn dispatch_pending(&self) -> usize {
        self.pass(Some(Duration::ZERO))
    }

    /// Runs `f` with a message-thread token.
    ///
    /// # Panics
    /// If called off the loop's thread (the `!Send` loop makes that impossible in safe code).
    /// ```
    /// assert_eq!(syrp_events::EventLoop::new()?.with_token(|_mt| 42), 42);
    /// # Ok::<(), syrp_events::EventsError>(())
    /// ```
    pub fn with_token<R>(&self, f: impl FnOnce(&MessageThread<'_>) -> R) -> R {
        assert!(
            self.inner.sender.is_message_thread(),
            "not the loop's thread"
        );
        // SAFETY: we are this loop's implementation, on its message thread (checked above),
        // and the token is dropped when `f` returns.
        let mt = unsafe { MessageThread::new_unchecked() };
        f(&mt)
    }

    fn take_quit(&self) -> bool {
        self.inner.shared.quit.swap(false, Ordering::SeqCst)
    }

    /// One dispatch pass. `deadline` (loop time) bounds the wait; `None` may block until a
    /// wake, a ready source or the next timer.
    fn pass(&self, deadline: Option<Duration>) -> usize {
        // SAFETY: only the loop calls this, on its message thread (the loop is `!Send`), and
        // the token lives only for this pass.
        let mt = unsafe { MessageThread::new_unchecked() };
        let timeout = self.timeout(deadline);
        let shared = &self.inner.shared;
        let mut ran = self.inner.poller.wait(&shared.waker, timeout, &mt);
        // Only the tasks queued now; ones posted meanwhile wait for the next pass.
        let queued = shared.queue().len();
        for _ in 0..queued {
            let Some(task) = shared.queue().pop_front() else {
                break;
            };
            task.run(&mt);
            ran += 1;
        }
        ran + self.fire_timers(&mt)
    }

    fn timeout(&self, deadline: Option<Duration>) -> Option<Duration> {
        let shared = &self.inner.shared;
        if shared.quit.load(Ordering::SeqCst) || !shared.queue().is_empty() {
            return Some(Duration::ZERO);
        }
        let next_timer = self
            .inner
            .timers
            .borrow()
            .order
            .first()
            .map(|&(due, _)| due);
        let now = self.now();
        next_timer
            .into_iter()
            .chain(deadline)
            .min()
            .map(|t| t.saturating_sub(now))
    }

    /// Fires every timer due at the start of the pass once, in (due, creation) order.
    fn fire_timers(&self, mt: &MessageThread<'_>) -> usize {
        let now = self.now();
        let timers = &self.inner.timers;
        let mut fired = 0;
        loop {
            let (due, id, period, mut f) = {
                let mut t = timers.borrow_mut();
                let Some(&(due, id)) = t.order.first().filter(|&&(due, _)| due <= now) else {
                    break;
                };
                t.order.pop_first();
                let Some(slot) = t.slots.get_mut(&id) else {
                    continue;
                };
                let Some(f) = slot.f.take() else { continue };
                (due, id, slot.period, f)
            };
            match &mut f {
                Callback::Timer(f) => f(mt),
                Callback::VBlank(f) => f(mt, due),
            }
            fired += 1;
            let mut t = timers.borrow_mut();
            if let Some(slot) = t.slots.get_mut(&id) {
                slot.f = Some(f);
                t.order.insert((next_due(due, period, now), id));
            } else {
                drop(t); // the guard was dropped in the callback; drop it outside the borrow
                drop(f);
            }
        }
        fired
    }

    fn add_timer(&self, period: Duration, f: Callback) -> CallbackGuard {
        let period = period.max(MIN_PERIOD);
        let id = self.inner.next_id.get();
        self.inner.next_id.set(id + 1);
        let due = self.now() + period;
        {
            let mut t = self.inner.timers.borrow_mut();
            t.slots.insert(id, Slot { period, f: Some(f) });
            t.order.insert((due, id));
        }
        let weak: Weak<Inner> = Rc::downgrade(&self.inner);
        CallbackGuard::new(move || {
            let Some(inner) = weak.upgrade() else { return };
            let removed = {
                let mut t = inner.timers.borrow_mut();
                t.order.retain(|&(_, i)| i != id);
                t.slots.remove(&id)
            };
            // Dropping a callback may drop other guards, which borrow the timers again.
            drop(removed);
        })
    }
}

/// The next deadline after `due` that is later than `now`, keeping the timer's phase: a late
/// timer skips the periods it missed instead of firing a burst.
pub(crate) fn next_due(due: Duration, period: Duration, now: Duration) -> Duration {
    let missed = now.saturating_sub(due).as_nanos() / period.as_nanos();
    let step = period.as_nanos().saturating_mul(missed + 1);
    due.saturating_add(Duration::from_nanos(
        u64::try_from(step).unwrap_or(u64::MAX),
    ))
}

impl MessageLoop for EventLoop {
    fn task_sender(&self) -> TaskSender {
        self.inner.sender.clone()
    }

    fn timer(&self, period: Duration, f: TimerFn) -> TimerGuard {
        self.add_timer(period, Callback::Timer(f))
    }

    /// Until a peer drives refreshes itself, a 60 Hz fallback timer calls `f` with each
    /// refresh time on the loop's timer clock ([`EventLoop::now`]).
    fn on_vblank(&self, _peer: PeerId, f: VBlankFn) -> VBlankGuard {
        self.add_timer(FALLBACK_REFRESH, Callback::VBlank(f))
    }
}

impl Drop for EventLoop {
    fn drop(&mut self) {
        HAS_LOOP.set(false);
    }
}

#[cfg(test)]
mod tests {
    use super::next_due;
    use std::time::Duration;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn b3_late_timer_keeps_phase_syrp_choice() {
        // On time: the next deadline is one period on.
        assert_eq!(next_due(ms(10), ms(10), ms(10)), ms(20));
        // 55 ms late on a 10 ms timer due at 10: fire once now, next at 70 (phase kept),
        // not at 65 (restarted) and not five times to catch up.
        assert_eq!(next_due(ms(10), ms(10), ms(65)), ms(70));
        assert_eq!(next_due(ms(10), ms(10), ms(70)), ms(80));
    }
}

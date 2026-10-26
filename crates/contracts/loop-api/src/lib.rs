// SPDX-License-Identifier: MIT OR Apache-2.0
//! The Syrp message-loop contract (master plan §5.5).
//!
//! - [`MessageThread`]: a zero-sized token proving that code runs on the message thread.
//! - [`MessageLoop`]: post a [`Task`] (from any thread through a [`TaskSender`]), start a
//!   timer, attach a VBlank callback, or block in `post_and_wait` from another thread.
//! - [`Coalescer`]: many triggers, one callback (JUCE's AsyncUpdater).
//! - [`ManualLoop`]: a deterministic loop with a virtual clock, used by every test.

use std::fmt;
use std::marker::PhantomData;
use std::sync::{Arc, mpsc};
use std::thread::{self, ThreadId};
use std::time::Duration;

use syrp_error::{Diagnostic, ErrorCode};

mod coalescer;
mod manual;

pub use coalescer::Coalescer;
pub use manual::ManualLoop;

/// Proof that the current code runs on the message thread.
///
/// Zero-sized, `!Send` and `!Sync`. Loops pass `&MessageThread` to every task, timer and
/// VBlank callback; safe code cannot make one any other way.
///
/// Tokens are per thread, not per process: several loops (parallel tests, an app and a hosted
/// plugin) can hand out tokens on different threads at once. Message-thread state must be
/// `!Send`, thread-local or owned per loop; a token never justifies unsynchronised access to
/// process-global state.
/// ```compile_fail
/// let mt = syrp_loop_api::MessageThread { _p: std::marker::PhantomData }; // private
/// ```
/// ```compile_fail
/// fn needs_send<T: Send>(_: T) {}
/// syrp_loop_api::ManualLoop::new().with_token(|mt| needs_send(mt)); // !Sync
/// ```
#[doc(alias = "MessageManager")]
#[derive(Debug)]
pub struct MessageThread<'a> {
    _p: PhantomData<(&'a (), *const ())>,
}

impl MessageThread<'_> {
    pub(crate) fn new() -> Self {
        MessageThread { _p: PhantomData }
    }

    /// Creates a token for a loop implemented outside this crate.
    ///
    /// # Safety
    /// The caller must be a [`MessageLoop`] implementation on its message thread, and the token
    /// must not outlive the callback it was made for. Tokens are per thread, not per process
    /// (other loops may hold one at once): a token never makes unsynchronised global access sound.
    /// ```
    /// // SAFETY: this doctest plays the part of a loop on its own message thread.
    /// let _mt = unsafe { syrp_loop_api::MessageThread::new_unchecked() };
    /// ```
    #[must_use]
    pub unsafe fn new_unchecked() -> Self {
        Self::new()
    }
}

/// A unit of work posted to the message loop from any thread.
/// ```
/// use syrp_loop_api::{ManualLoop, MessageLoop, Task};
/// let lp = ManualLoop::new();
/// lp.post(Task::new(|_mt| println!("on the message thread")));
/// assert_eq!(lp.run_until_idle(), Ok(1));
/// ```
#[doc(alias = "callAsync")]
pub struct Task {
    f: Box<dyn FnOnce(&MessageThread<'_>) + Send>,
}

impl Task {
    /// Wraps a closure that runs once, on the message thread.
    /// ```
    /// let _task = syrp_loop_api::Task::new(|_mt| {});
    /// ```
    pub fn new(f: impl FnOnce(&MessageThread<'_>) + Send + 'static) -> Self {
        Task { f: Box::new(f) }
    }

    /// Runs the task. Loop implementations call this with their token.
    /// ```
    /// let task = syrp_loop_api::Task::new(|_mt| println!("ran"));
    /// syrp_loop_api::ManualLoop::new().with_token(|mt| task.run(mt));
    /// ```
    pub fn run(self, mt: &MessageThread<'_>) {
        (self.f)(mt);
    }
}

/// A timer callback: runs on the message thread every period until its guard is dropped.
pub type TimerFn = Box<dyn FnMut(&MessageThread<'_>)>;

/// A VBlank callback: runs on the message thread once per display refresh of its peer, with
/// the refresh time on the same monotonic clock and epoch as the loop's timers.
pub type VBlankFn = Box<dyn FnMut(&MessageThread<'_>, Duration)>;

/// Identifies a native window peer (see `syrp-peer-api`).
/// ```
/// assert_eq!(syrp_loop_api::PeerId::from_raw(7).to_raw(), 7);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PeerId(u64);

impl PeerId {
    /// Wraps a raw peer number.
    /// ```
    /// let _id = syrp_loop_api::PeerId::from_raw(1);
    /// ```
    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        PeerId(raw)
    }

    /// The raw peer number.
    /// ```
    /// assert_eq!(syrp_loop_api::PeerId::from_raw(3).to_raw(), 3);
    /// ```
    #[must_use]
    pub const fn to_raw(self) -> u64 {
        self.0
    }
}

/// Keeps a timer or VBlank callback registered; dropping the guard cancels it at once.
/// ```
/// use std::time::Duration;
/// use syrp_loop_api::{ManualLoop, MessageLoop};
/// let lp = ManualLoop::new();
/// drop(lp.timer(Duration::from_millis(10), Box::new(|_| {})));
/// assert_eq!(lp.advance(Duration::from_millis(50)), 0);
/// ```
#[doc(alias = "Timer")]
#[doc(alias = "VBlankAttachment")]
#[must_use = "dropping the guard cancels the callback at once"]
pub struct CallbackGuard {
    cancel: Option<Box<dyn FnOnce()>>,
}

impl CallbackGuard {
    /// A guard that calls `cancel` when dropped. For loop implementations.
    /// ```
    /// drop(syrp_loop_api::CallbackGuard::new(|| println!("cancelled")));
    /// ```
    pub fn new(cancel: impl FnOnce() + 'static) -> Self {
        let cancel: Box<dyn FnOnce()> = Box::new(cancel);
        CallbackGuard {
            cancel: Some(cancel),
        }
    }
}

impl Drop for CallbackGuard {
    fn drop(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel();
        }
    }
}

/// Returned by [`MessageLoop::timer`]; dropping it stops the timer.
pub type TimerGuard = CallbackGuard;

/// Returned by [`MessageLoop::on_vblank`]; dropping it detaches the callback.
pub type VBlankGuard = CallbackGuard;

/// A cloneable, `Send + Sync` handle that posts [`Task`]s to one loop from any thread.
/// ```
/// use syrp_loop_api::{ManualLoop, MessageLoop, Task};
/// let lp = ManualLoop::new();
/// let sender = lp.task_sender();
/// std::thread::spawn(move || sender.post(Task::new(|_mt| {}))).join().unwrap();
/// assert_eq!(lp.run_until_idle(), Ok(1));
/// ```
#[derive(Clone)]
pub struct TaskSender {
    post: Arc<dyn Fn(Task) + Send + Sync>,
    message_thread: ThreadId,
}

impl TaskSender {
    /// A sender for a loop whose message thread is `message_thread`. `post` enqueues a task,
    /// or drops it if the loop has shut down. For loop implementations.
    /// ```
    /// let s = syrp_loop_api::TaskSender::new(std::thread::current().id(), |task| drop(task));
    /// assert!(s.is_message_thread());
    /// ```
    pub fn new(message_thread: ThreadId, post: impl Fn(Task) + Send + Sync + 'static) -> Self {
        let post: Arc<dyn Fn(Task) + Send + Sync> = Arc::new(post);
        TaskSender {
            post,
            message_thread,
        }
    }

    /// Queues `task` to run later on the message thread; never runs it inline.
    /// ```
    /// # use syrp_loop_api::{ManualLoop, MessageLoop, Task};
    /// ManualLoop::new().task_sender().post(Task::new(|_| {}));
    /// ```
    pub fn post(&self, task: Task) {
        (self.post)(task);
    }

    /// Whether the calling thread is this loop's message thread.
    /// ```
    /// # use syrp_loop_api::{ManualLoop, MessageLoop};
    /// let sender = ManualLoop::new().task_sender();
    /// assert!(!std::thread::spawn(move || sender.is_message_thread()).join().unwrap());
    /// ```
    #[must_use]
    pub fn is_message_thread(&self) -> bool {
        thread::current().id() == self.message_thread
    }

    /// Runs `f` on the message thread and blocks until it returns its result.
    ///
    /// # Errors
    /// [`LoopError::OnMessageThread`] on the message thread itself (it would wait for itself);
    /// [`LoopError::Closed`] if the loop drops the task unrun or the task panics.
    /// ```
    /// use syrp_loop_api::{LoopError, ManualLoop, MessageLoop};
    /// let sender = ManualLoop::new().task_sender();
    /// assert_eq!(sender.post_and_wait(|_| 1), Err(LoopError::OnMessageThread));
    /// ```
    #[doc(alias = "callSync")]
    pub fn post_and_wait<R, F>(&self, f: F) -> Result<R, LoopError>
    where
        F: FnOnce(&MessageThread<'_>) -> R + Send + 'static,
        R: Send + 'static,
    {
        if self.is_message_thread() {
            return Err(LoopError::OnMessageThread);
        }
        let (tx, rx) = mpsc::sync_channel(1);
        // If the waiter has gone, nobody needs the result.
        self.post(Task::new(move |mt| drop(tx.send(f(mt)))));
        rx.recv().map_err(|_| LoopError::Closed)
    }
}

/// A message loop: owned (apps), hosted (plugins) or [`ManualLoop`] (tests).
///
/// Loops are usually `!Send` and live on their message thread; other threads reach them via
/// [`task_sender`](MessageLoop::task_sender). Dyn-compatible except for `post_and_wait`.
/// ```
/// use syrp_loop_api::{ManualLoop, MessageLoop, Task};
/// fn schedule(lp: &dyn MessageLoop) {
///     lp.post(Task::new(|_mt| println!("later")));
/// }
/// let lp = ManualLoop::new();
/// schedule(&lp);
/// assert_eq!(lp.run_until_idle(), Ok(1));
/// ```
#[doc(alias = "MessageManager")]
pub trait MessageLoop {
    /// A handle for posting tasks from any thread.
    /// ```
    /// # use syrp_loop_api::{ManualLoop, MessageLoop};
    /// let _sender = ManualLoop::new().task_sender();
    /// ```
    fn task_sender(&self) -> TaskSender;

    /// Queues `task` to run later on the message thread (default: via the task sender).
    /// ```
    /// # use syrp_loop_api::{ManualLoop, MessageLoop, Task};
    /// ManualLoop::new().post(Task::new(|_| {}));
    /// ```
    #[doc(alias = "callAsync")]
    fn post(&self, task: Task) {
        self.task_sender().post(task);
    }

    /// Calls `f` every `period` on the message thread until the guard is dropped.
    /// ```
    /// # use std::time::Duration; use syrp_loop_api::{ManualLoop, MessageLoop};
    /// let lp = ManualLoop::new();
    /// let _t = lp.timer(Duration::from_millis(30), Box::new(|_mt| {}));
    /// assert_eq!(lp.advance(Duration::from_millis(100)), 3);
    /// ```
    #[doc(alias = "Timer")]
    #[doc(alias = "MultiTimer")]
    fn timer(&self, period: Duration, f: TimerFn) -> TimerGuard;

    /// Calls `f` on every display refresh of `peer` until the guard is dropped.
    /// ```
    /// # use syrp_loop_api::{ManualLoop, MessageLoop, PeerId};
    /// let lp = ManualLoop::new();
    /// let _v = lp.on_vblank(PeerId::from_raw(1), Box::new(|_mt, _at| {}));
    /// assert_eq!(lp.vblank(PeerId::from_raw(1)), 1);
    /// ```
    #[doc(alias = "VBlankAttachment")]
    fn on_vblank(&self, peer: PeerId, f: VBlankFn) -> VBlankGuard;

    /// Runs `f` on the message thread and waits; forwards to [`TaskSender::post_and_wait`].
    ///
    /// # Errors
    /// As [`TaskSender::post_and_wait`].
    /// ```
    /// # use syrp_loop_api::{LoopError, ManualLoop, MessageLoop};
    /// let lp = ManualLoop::new();
    /// assert_eq!(lp.post_and_wait(|_| ()), Err(LoopError::OnMessageThread));
    /// ```
    #[doc(alias = "callSync")]
    fn post_and_wait<R, F>(&self, f: F) -> Result<R, LoopError>
    where
        Self: Sized,
        F: FnOnce(&MessageThread<'_>) -> R + Send + 'static,
        R: Send + 'static,
    {
        self.task_sender().post_and_wait(f)
    }
}

/// Errors from message-loop operations, catalogued in `errors/loop.toml`.
/// ```
/// use syrp_error::Diagnostic;
/// let code = syrp_loop_api::LoopError::TaskLimitReached.code();
/// assert_eq!(code.as_str(), "SYRP-LOOP-0002");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LoopError {
    /// `post_and_wait` was called on the message thread (`SYRP-LOOP-0001`).
    OnMessageThread,
    /// `ManualLoop::run_until_idle` hit its task limit with work still queued (`SYRP-LOOP-0002`).
    TaskLimitReached,
    /// The loop dropped a waited-on task, or the task panicked (`SYRP-LOOP-0003`).
    Closed,
}

impl fmt::Display for LoopError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            LoopError::OnMessageThread => "post_and_wait was called on the message thread",
            LoopError::TaskLimitReached => "run_until_idle reached its task limit",
            LoopError::Closed => "the message loop closed before the task ran",
        })
    }
}

impl std::error::Error for LoopError {}

impl Diagnostic for LoopError {
    fn code(&self) -> ErrorCode {
        ErrorCode::new(match self {
            LoopError::OnMessageThread => "SYRP-LOOP-0001",
            LoopError::TaskLimitReached => "SYRP-LOOP-0002",
            LoopError::Closed => "SYRP-LOOP-0003",
        })
    }
}

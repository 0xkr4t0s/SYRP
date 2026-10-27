// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Named threads with cooperative exit signalling, notification and timed stop.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{JoinHandle, ThreadId};
use std::time::{Duration, Instant};

use crate::ThreadError;

/// How long dropping a [`ThreadHandle`] waits for the thread to exit before detaching it,
/// unless [`ThreadBuilder::drop_timeout`] chose another value.
///
/// ```
/// assert_eq!(syrp_thread::DEFAULT_DROP_TIMEOUT.as_secs(), 5);
/// ```
pub const DEFAULT_DROP_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
struct Flags {
    notified: bool,
    finished: bool,
}

/// State shared by a handle and its thread.
#[derive(Debug)]
struct Shared {
    should_exit: AtomicBool,
    flags: Mutex<Flags>,
    cond: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Flags> {
        self.flags.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Waits until `done(flags)` holds or the deadline passes; returns whether it held.
    fn wait_until(
        &self,
        timeout: Option<Duration>,
        mut done: impl FnMut(&mut Flags) -> bool,
    ) -> bool {
        let deadline = timeout.and_then(|t| Instant::now().checked_add(t));
        let mut flags = self.lock();
        loop {
            if done(&mut flags) {
                return true;
            }
            flags = match (timeout, deadline) {
                (None, _) | (Some(_), None) => self
                    .cond
                    .wait(flags)
                    .unwrap_or_else(PoisonError::into_inner),
                (Some(_), Some(d)) => {
                    let Some(left) = d.checked_duration_since(Instant::now()) else {
                        return false;
                    };
                    let waited = self.cond.wait_timeout(flags, left);
                    waited.unwrap_or_else(PoisonError::into_inner).0
                }
            };
        }
    }

    fn signal_exit(&self) {
        self.should_exit.store(true, Ordering::Release);
        let _flags = self.lock();
        self.cond.notify_all();
    }
}

/// Marks the thread finished when its body returns or unwinds.
struct FinishGuard(Arc<Shared>);

impl Drop for FinishGuard {
    fn drop(&mut self) {
        self.0.lock().finished = true;
        self.0.cond.notify_all();
    }
}

/// Entry points for creating threads. This type has no values.
///
/// ```
/// use syrp_thread::Thread;
/// let t = Thread::builder("worker").spawn(|ctx| {
///     while !ctx.thread_should_exit() {
///         ctx.wait(None);
///     }
/// })
/// .unwrap();
/// t.stop(std::time::Duration::from_secs(1)).unwrap();
/// ```
#[doc(alias = "juce::Thread")]
#[derive(Debug)]
pub enum Thread {}

impl Thread {
    /// Starts describing a thread called `name` (the OS may truncate it; Linux keeps 15 bytes).
    ///
    /// ```
    /// let b = syrp_thread::Thread::builder("io");
    /// # drop(b);
    /// ```
    pub fn builder(name: impl Into<String>) -> ThreadBuilder {
        ThreadBuilder {
            name: name.into(),
            stack_size: None,
            drop_timeout: DEFAULT_DROP_TIMEOUT,
        }
    }

    /// Runs `f` on a new unnamed thread and forgets it. Fails only if the OS cannot create
    /// the thread (`SYRP-THREAD-0002`).
    ///
    /// ```
    /// let (tx, rx) = std::sync::mpsc::channel();
    /// syrp_thread::Thread::launch(move || tx.send(42).unwrap()).unwrap();
    /// assert_eq!(rx.recv().unwrap(), 42);
    /// ```
    pub fn launch<F>(f: F) -> Result<(), ThreadError>
    where
        F: FnOnce() + Send + 'static,
    {
        std::thread::Builder::new()
            .spawn(f)
            .map(drop)
            .map_err(|e| ThreadError::SpawnFailed {
                os_error: e.raw_os_error(),
            })
    }
}

/// Configures a thread before [`spawn`](Self::spawn). Made by [`Thread::builder`].
///
/// ```
/// use syrp_thread::Thread;
/// let t = Thread::builder("small")
///     .stack_size(256 * 1024)
///     .spawn(|_| {})
///     .unwrap();
/// t.stop(std::time::Duration::from_secs(1)).unwrap();
/// ```
#[derive(Debug, Clone)]
#[must_use]
pub struct ThreadBuilder {
    name: String,
    stack_size: Option<usize>,
    drop_timeout: Duration,
}

impl ThreadBuilder {
    /// Sets the stack size in bytes (the OS may round it up).
    ///
    /// ```
    /// # use syrp_thread::Thread;
    /// let t = Thread::builder("s").stack_size(1 << 20).spawn(|_| {}).unwrap();
    /// # drop(t);
    /// ```
    pub fn stack_size(mut self, bytes: usize) -> Self {
        self.stack_size = Some(bytes);
        self
    }

    /// How long dropping the handle waits for the thread before detaching it
    /// (default [`DEFAULT_DROP_TIMEOUT`]).
    ///
    /// ```
    /// # use std::time::Duration;
    /// # use syrp_thread::Thread;
    /// let t = Thread::builder("d").drop_timeout(Duration::from_millis(10)).spawn(|_| {}).unwrap();
    /// # drop(t);
    /// ```
    pub fn drop_timeout(mut self, timeout: Duration) -> Self {
        self.drop_timeout = timeout;
        self
    }

    /// Starts the thread running `f`. Fails with `SYRP-THREAD-0002` if the OS cannot create
    /// the thread.
    ///
    /// ```
    /// # use syrp_thread::Thread;
    /// let t = Thread::builder("hello").spawn(|ctx| assert!(!ctx.thread_should_exit())).unwrap();
    /// assert_eq!(t.name(), Some("hello"));
    /// ```
    #[doc(alias = "startThread")]
    pub fn spawn<F>(self, f: F) -> Result<ThreadHandle, ThreadError>
    where
        F: FnOnce(&ThreadContext) + Send + 'static,
    {
        let shared = Arc::new(Shared {
            should_exit: AtomicBool::new(false),
            flags: Mutex::new(Flags {
                notified: false,
                finished: false,
            }),
            cond: Condvar::new(),
        });
        let ctx = ThreadContext {
            shared: Arc::clone(&shared),
        };
        let mut builder = std::thread::Builder::new().name(self.name);
        if let Some(bytes) = self.stack_size {
            builder = builder.stack_size(bytes);
        }
        let join = builder
            .spawn(move || {
                let _finish = FinishGuard(Arc::clone(&ctx.shared));
                f(&ctx);
            })
            .map_err(|e| ThreadError::SpawnFailed {
                os_error: e.raw_os_error(),
            })?;
        Ok(ThreadHandle {
            id: join.thread().id(),
            join: Some(join),
            shared,
            drop_timeout: self.drop_timeout,
        })
    }
}

/// What a thread body sees: the exit flag and a notifiable wait.
///
/// ```
/// use std::time::Duration;
/// use syrp_thread::Thread;
/// let t = Thread::builder("ctx").spawn(|ctx| {
///     assert!(!ctx.wait(Some(Duration::from_millis(1)))); // nobody notified
/// }).unwrap();
/// assert!(t.wait_for_thread_to_exit(None));
/// assert_eq!(t.stop(Duration::from_secs(1)), Ok(syrp_thread::Stopped::Finished));
/// ```
#[derive(Debug)]
pub struct ThreadContext {
    shared: Arc<Shared>,
}

impl ThreadContext {
    /// Whether the owner asked this thread to exit. Bodies should poll it and return.
    ///
    /// ```
    /// # use syrp_thread::Thread;
    /// let t = Thread::builder("poll").spawn(|ctx| while !ctx.thread_should_exit() {
    ///     ctx.wait(None);
    /// }).unwrap();
    /// t.signal_thread_should_exit();
    /// t.stop(std::time::Duration::from_secs(1)).unwrap();
    /// ```
    #[doc(alias = "threadShouldExit")]
    #[must_use]
    pub fn thread_should_exit(&self) -> bool {
        self.shared.should_exit.load(Ordering::Acquire)
    }

    /// Sleeps until [`ThreadHandle::notify`] is called, the exit signal is raised, or
    /// `timeout` passes (`None` waits indefinitely). Returns `true` when woken by a
    /// notification (which it consumes) or the exit signal, and `false` on timeout. Once the
    /// exit signal is raised, every call returns `true` at once.
    ///
    /// ```
    /// # use std::time::Duration;
    /// # use syrp_thread::Thread;
    /// let t = Thread::builder("w").spawn(|ctx| assert!(ctx.wait(None))).unwrap();
    /// t.notify();
    /// assert!(matches!(t.stop(Duration::from_secs(1)), Ok(syrp_thread::Stopped::Finished)));
    /// ```
    pub fn wait(&self, timeout: Option<Duration>) -> bool {
        let exit = &self.shared.should_exit;
        self.shared.wait_until(timeout, |flags| {
            exit.load(Ordering::Acquire) || std::mem::take(&mut flags.notified)
        })
    }
}

/// How a stopped thread ended.
///
/// ```
/// # use std::time::Duration;
/// use syrp_thread::{Stopped, Thread};
/// let t = Thread::builder("ok").spawn(|_| {}).unwrap();
/// assert_eq!(t.stop(Duration::from_secs(1)), Ok(Stopped::Finished));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Stopped {
    /// The body returned normally.
    Finished,
    /// The body panicked.
    Panicked,
}

/// Owns a running thread made by [`ThreadBuilder::spawn`].
///
/// Rust cannot kill a thread. [`stop`](Self::stop) signals exit and waits; on timeout it
/// returns `SYRP-THREAD-0001` and leaves the thread running, detached. Dropping the handle
/// does the same with the builder's drop timeout ([`DEFAULT_DROP_TIMEOUT`], 5 s), so a body
/// that ignores the signal never hangs the dropping thread for longer than that.
///
/// ```
/// use std::time::Duration;
/// use syrp_thread::Thread;
/// let t = Thread::builder("h").spawn(|ctx| { ctx.wait(None); }).unwrap();
/// assert!(t.is_running());
/// t.notify();
/// assert!(t.wait_for_thread_to_exit(Some(Duration::from_secs(5))));
/// assert!(!t.is_running());
/// ```
#[derive(Debug)]
pub struct ThreadHandle {
    shared: Arc<Shared>,
    join: Option<JoinHandle<()>>,
    id: ThreadId,
    drop_timeout: Duration,
}

impl ThreadHandle {
    /// Raises the exit flag and wakes [`ThreadContext::wait`].
    ///
    /// ```
    /// # use syrp_thread::Thread;
    /// let t = Thread::builder("x").spawn(|ctx| while !ctx.thread_should_exit() { ctx.wait(None); }).unwrap();
    /// t.signal_thread_should_exit();
    /// assert!(t.wait_for_thread_to_exit(None));
    /// ```
    #[doc(alias = "signalThreadShouldExit")]
    pub fn signal_thread_should_exit(&self) {
        self.shared.signal_exit();
    }

    /// Wakes one pending or future [`ThreadContext::wait`]. Notifications do not queue: several
    /// before a wait wake it once.
    ///
    /// ```
    /// # use syrp_thread::Thread;
    /// let t = Thread::builder("n").spawn(|ctx| { ctx.wait(None); }).unwrap();
    /// t.notify();
    /// assert!(t.wait_for_thread_to_exit(None));
    /// ```
    pub fn notify(&self) {
        self.shared.lock().notified = true;
        self.shared.cond.notify_all();
    }

    /// Whether the body has not yet returned.
    ///
    /// ```
    /// # use syrp_thread::Thread;
    /// let t = Thread::builder("r").spawn(|_| {}).unwrap();
    /// t.wait_for_thread_to_exit(None);
    /// assert!(!t.is_running());
    /// ```
    #[doc(alias = "isThreadRunning")]
    #[must_use]
    pub fn is_running(&self) -> bool {
        !self.shared.lock().finished
    }

    /// Waits until the body returns or `timeout` passes (`None` waits indefinitely), without
    /// signalling it. Returns whether it returned.
    ///
    /// ```
    /// # use std::time::Duration;
    /// # use syrp_thread::Thread;
    /// let t = Thread::builder("wt").spawn(|ctx| { ctx.wait(None); }).unwrap();
    /// assert!(!t.wait_for_thread_to_exit(Some(Duration::from_millis(5))));
    /// t.notify();
    /// assert!(t.wait_for_thread_to_exit(None));
    /// ```
    #[doc(alias = "waitForThreadToExit")]
    pub fn wait_for_thread_to_exit(&self, timeout: Option<Duration>) -> bool {
        self.shared.wait_until(timeout, |flags| flags.finished)
    }

    /// The thread's id, equal to what [`current_thread_id`] returns inside it.
    ///
    /// ```
    /// # use syrp_thread::{Thread, current_thread_id};
    /// let t = Thread::builder("id").spawn(|_| {}).unwrap();
    /// assert_ne!(t.id(), current_thread_id());
    /// ```
    #[doc(alias = "getThreadId")]
    #[must_use]
    pub fn id(&self) -> ThreadId {
        self.id
    }

    /// The name given to [`Thread::builder`].
    ///
    /// ```
    /// # use syrp_thread::Thread;
    /// assert_eq!(Thread::builder("nm").spawn(|_| {}).unwrap().name(), Some("nm"));
    /// ```
    #[doc(alias = "getThreadName")]
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.join.as_ref().and_then(|j| j.thread().name())
    }

    /// Signals exit and waits up to `timeout` for the body to return, then joins it. On
    /// timeout returns `SYRP-THREAD-0001` and leaves the thread running, detached: this
    /// replaces JUCE's forced kill, which Rust cannot do safely.
    ///
    /// ```
    /// # use std::time::Duration;
    /// # use syrp_thread::{Stopped, Thread};
    /// let t = Thread::builder("st").spawn(|ctx| { ctx.wait(None); }).unwrap();
    /// assert_eq!(t.stop(Duration::from_secs(5)), Ok(Stopped::Finished));
    /// ```
    #[doc(alias = "stopThread")]
    pub fn stop(mut self, timeout: Duration) -> Result<Stopped, ThreadError> {
        self.signal_thread_should_exit();
        let join = self.join.take();
        if !self.wait_for_thread_to_exit(Some(timeout)) {
            return Err(ThreadError::StopTimedOut { timeout });
        }
        Ok(match join.map(JoinHandle::join) {
            Some(Err(_)) => Stopped::Panicked,
            _ => Stopped::Finished,
        })
    }
}

impl Drop for ThreadHandle {
    fn drop(&mut self) {
        if let Some(join) = self.join.take() {
            self.signal_thread_should_exit();
            if self.wait_for_thread_to_exit(Some(self.drop_timeout)) {
                let _ = join.join();
            }
        }
    }
}

/// The calling thread's id.
///
/// ```
/// assert_eq!(syrp_thread::current_thread_id(), std::thread::current().id());
/// ```
#[doc(alias = "getCurrentThreadId")]
#[must_use]
pub fn current_thread_id() -> ThreadId {
    std::thread::current().id()
}

/// The calling thread's name, if it has one (the test harness and `main` name theirs).
///
/// ```
/// let name = syrp_thread::current_thread_name();
/// # let _ = name;
/// ```
#[doc(alias = "getCurrentThreadName")]
#[must_use]
pub fn current_thread_name() -> Option<String> {
    std::thread::current().name().map(str::to_owned)
}

/// Blocks the calling thread for at least `duration`.
///
/// ```
/// syrp_thread::sleep(std::time::Duration::from_millis(1));
/// ```
pub fn sleep(duration: Duration) {
    std::thread::sleep(duration);
}

/// Offers the rest of the calling thread's time slice to other threads.
///
/// ```
/// syrp_thread::yield_now();
/// ```
#[doc(alias = "yield")]
pub fn yield_now() {
    std::thread::yield_now();
}

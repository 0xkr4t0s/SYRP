// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`ThreadPool`]: a fixed set of worker threads running queued jobs.

use std::collections::{HashMap, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use syrp_sys::SystemStats;

use crate::{Priority, Thread, ThreadError, ThreadHandle};

/// How a [`ThreadPool`] is built. Start from [`Default`] and change what you need.
///
/// ```
/// use syrp_thread::ThreadPoolOptions;
/// let opts = ThreadPoolOptions { num_threads: 2, ..ThreadPoolOptions::default() };
/// assert_eq!(opts.thread_name, "Pool");
/// ```
#[doc(alias = "juce::ThreadPoolOptions")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadPoolOptions {
    /// Name given to every worker thread (default `"Pool"`).
    pub thread_name: String,
    /// Number of workers; 0 is treated as 1. Defaults to the logical CPUs this process may use
    /// ([`SystemStats::num_cpus`]).
    pub num_threads: usize,
    /// Worker stack size in bytes, or `None` for the platform default.
    pub stack_size: Option<usize>,
    /// Worker priority, or `None` to leave it alone. A refusal is ignored, as in
    /// [`ThreadBuilder::priority`](crate::ThreadBuilder::priority).
    pub priority: Option<Priority>,
}

impl Default for ThreadPoolOptions {
    fn default() -> Self {
        Self {
            thread_name: "Pool".into(),
            num_threads: SystemStats::num_cpus(),
            stack_size: None,
            priority: None,
        }
    }
}

/// What a job wants after one call to [`ThreadPoolJob::run_job`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum JobStatus {
    /// The job is done and leaves the pool.
    Finished,
    /// Run the job again later: it goes to the back of the queue.
    RunAgain,
}

/// A unit of work for a [`ThreadPool`]. Closures can be added with [`ThreadPool::add_fn`].
///
/// ```
/// use syrp_thread::{JobContext, JobStatus, ThreadPoolJob};
/// struct Countdown(u32);
/// impl ThreadPoolJob for Countdown {
///     fn name(&self) -> &str { "countdown" }
///     fn run_job(&mut self, ctx: &JobContext) -> JobStatus {
///         self.0 -= 1;
///         if self.0 == 0 || ctx.should_exit() { JobStatus::Finished } else { JobStatus::RunAgain }
///     }
/// }
/// ```
#[doc(alias = "juce::ThreadPoolJob")]
pub trait ThreadPoolJob: Send + 'static {
    /// A name for diagnostics and for [`ThreadPool::remove_all_jobs`] filters.
    fn name(&self) -> &str;

    /// Does one slice of work. Long jobs should check [`JobContext::should_exit`] and return
    /// soon after it turns true.
    #[doc(alias = "runJob")]
    fn run_job(&mut self, ctx: &JobContext) -> JobStatus;
}

/// What a running job sees: whether it has been asked to stop.
#[derive(Debug)]
pub struct JobContext {
    interrupt: Arc<AtomicBool>,
}

impl JobContext {
    /// True once the job has been interrupted by a remove call or by dropping the pool.
    #[doc(alias = "shouldExit")]
    pub fn should_exit(&self) -> bool {
        self.interrupt.load(Ordering::Acquire)
    }
}

/// Names one job added to a [`ThreadPool`]. Handles are never reused, so a stale handle simply
/// refers to a job that is no longer in the pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct JobHandle(u64);

struct FnJob<F>(F);

impl<F: FnMut(&JobContext) -> JobStatus + Send + 'static> ThreadPoolJob for FnJob<F> {
    fn name(&self) -> &str {
        "closure"
    }

    fn run_job(&mut self, ctx: &JobContext) -> JobStatus {
        (self.0)(ctx)
    }
}

struct Entry {
    name: String,
    /// `None` while a worker has the job out to run it.
    job: Option<Box<dyn ThreadPoolJob>>,
    interrupt: Arc<AtomicBool>,
    /// Set by a remove call on a running job: it leaves when it returns, whatever its status.
    removed: bool,
}

#[derive(Default)]
struct State {
    queue: VecDeque<u64>,
    jobs: HashMap<u64, Entry>,
    next_id: u64,
    shutting_down: bool,
}

#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    /// Workers wait here for queued jobs.
    work: Condvar,
    /// Waiters for a job to leave the pool wait here.
    done: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Waits until none of `ids` is in the pool, or the timeout passes; returns which.
    fn wait_gone(&self, mut st: MutexGuard<'_, State>, ids: &[u64], t: Option<Duration>) -> bool {
        let deadline = t.and_then(|t| Instant::now().checked_add(t));
        loop {
            if !ids.iter().any(|id| st.jobs.contains_key(id)) {
                return true;
            }
            st = match (t, deadline) {
                (Some(_), Some(d)) => {
                    let left = d.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return false;
                    }
                    let waited = self.done.wait_timeout(st, left);
                    waited.unwrap_or_else(PoisonError::into_inner).0
                }
                _ => self.done.wait(st).unwrap_or_else(PoisonError::into_inner),
            };
        }
    }

    fn worker(&self) {
        let mut st = self.lock();
        loop {
            if st.shutting_down {
                return;
            }
            let Some(id) = st.queue.pop_front() else {
                st = self.work.wait(st).unwrap_or_else(PoisonError::into_inner);
                continue;
            };
            let Some(entry) = st.jobs.get_mut(&id) else {
                continue;
            };
            let Some(mut job) = entry.job.take() else {
                continue;
            };
            let ctx = JobContext {
                interrupt: Arc::clone(&entry.interrupt),
            };
            drop(st);
            // A panicking job counts as finished; the worker carries on.
            let status =
                catch_unwind(AssertUnwindSafe(|| job.run_job(&ctx))).unwrap_or(JobStatus::Finished);
            st = self.lock();
            let again = status == JobStatus::RunAgain && !st.shutting_down;
            let mut finished = None;
            if let Some(entry) = st.jobs.get_mut(&id) {
                if again && !entry.removed {
                    entry.job = Some(job);
                    st.queue.push_back(id);
                } else {
                    finished = Some(job);
                    st.jobs.remove(&id);
                }
            }
            self.done.notify_all();
            if finished.is_some() {
                // The job's own Drop may call back into the pool, so run it unlocked; a
                // panicking Drop must not take the worker down either.
                drop(st);
                let _ = catch_unwind(AssertUnwindSafe(move || drop(finished)));
                st = self.lock();
            }
        }
    }
}

/// A fixed set of worker threads that run [`ThreadPoolJob`]s in FIFO order.
///
/// A job never runs on two threads at once. [`JobStatus::RunAgain`] puts a job at the back of
/// the queue. A job that panics counts as finished. Dropping the pool removes queued jobs,
/// interrupts running ones and waits for them to return.
///
/// ```
/// use std::sync::Arc;
/// use std::sync::atomic::{AtomicUsize, Ordering};
/// use syrp_thread::{JobStatus, ThreadPool, ThreadPoolOptions};
/// let pool = ThreadPool::new(ThreadPoolOptions { num_threads: 2, ..Default::default() }).unwrap();
/// let hits = Arc::new(AtomicUsize::new(0));
/// let jobs: Vec<_> = (0..10).map(|_| {
///     let hits = Arc::clone(&hits);
///     pool.add_fn(move |_| { hits.fetch_add(1, Ordering::SeqCst); JobStatus::Finished })
/// }).collect();
/// for j in jobs { assert!(pool.wait_for_job_to_finish(j, None)); }
/// assert_eq!(hits.load(Ordering::SeqCst), 10);
/// ```
#[doc(alias = "juce::ThreadPool")]
pub struct ThreadPool {
    shared: Arc<Shared>,
    workers: Vec<ThreadHandle>,
}

impl std::fmt::Debug for ThreadPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThreadPool")
            .field("num_threads", &self.workers.len())
            .field("num_jobs", &self.num_jobs())
            .finish()
    }
}

impl ThreadPool {
    /// Starts the workers. Fails with `SYRP-THREAD-0002` if the OS cannot create one; the
    /// workers already started are stopped again.
    pub fn new(options: ThreadPoolOptions) -> Result<Self, ThreadError> {
        let shared = Arc::new(Shared::default());
        let mut pool = Self {
            shared: Arc::clone(&shared),
            workers: Vec::new(),
        };
        for _ in 0..options.num_threads.max(1) {
            let mut b = Thread::builder(options.thread_name.clone())
                // Drop waits for running jobs to return, however long they take.
                .drop_timeout(Duration::MAX);
            if let Some(p) = options.priority {
                b = b.priority(p);
            }
            if let Some(bytes) = options.stack_size {
                b = b.stack_size(bytes);
            }
            let s = Arc::clone(&shared);
            pool.workers.push(b.spawn(move |_| s.worker())?);
        }
        Ok(pool)
    }

    /// Queues `job` and returns its handle.
    #[doc(alias = "addJob")]
    pub fn add_job(&self, job: impl ThreadPoolJob) -> JobHandle {
        let name = job.name().to_owned();
        let mut st = self.shared.lock();
        let id = st.next_id;
        st.next_id += 1;
        st.jobs.insert(
            id,
            Entry {
                name,
                job: Some(Box::new(job)),
                interrupt: Arc::new(AtomicBool::new(false)),
                removed: false,
            },
        );
        st.queue.push_back(id);
        self.shared.work.notify_one();
        JobHandle(id)
    }

    /// Queues a closure as a job named `"closure"`.
    ///
    /// ```
    /// # use syrp_thread::{JobStatus, ThreadPool, ThreadPoolOptions};
    /// # let pool = ThreadPool::new(ThreadPoolOptions { num_threads: 1, ..Default::default() }).unwrap();
    /// let mut left = 3;
    /// let job = pool.add_fn(move |_| {
    ///     left -= 1;
    ///     if left == 0 { JobStatus::Finished } else { JobStatus::RunAgain }
    /// });
    /// assert!(pool.wait_for_job_to_finish(job, None));
    /// ```
    pub fn add_fn<F>(&self, f: F) -> JobHandle
    where
        F: FnMut(&JobContext) -> JobStatus + Send + 'static,
    {
        self.add_job(FnJob(f))
    }

    /// Removes a job. A queued job is dropped at once. A running job is interrupted first if
    /// `interrupt` is true, will not run again, and is waited for up to `timeout` (`None` waits
    /// for ever). A job no longer in the pool counts as removed. Fails with
    /// `SYRP-THREAD-0007` if the running job did not return in time; it still leaves the pool
    /// when it does.
    #[doc(alias = "removeJob")]
    pub fn remove_job(
        &self,
        job: JobHandle,
        interrupt: bool,
        timeout: Option<Duration>,
    ) -> Result<(), ThreadError> {
        self.remove(&[job.0], interrupt, timeout)
    }

    /// Removes every job for which `filter(handle, name)` is true, as
    /// [`remove_job`](Self::remove_job) does. Pass `|_, _| true` for all jobs. `filter` runs
    /// without the pool's lock held.
    #[doc(alias = "removeAllJobs")]
    pub fn remove_all_jobs(
        &self,
        interrupt: bool,
        timeout: Option<Duration>,
        mut filter: impl FnMut(JobHandle, &str) -> bool,
    ) -> Result<(), ThreadError> {
        let snapshot: Vec<(u64, String)> = {
            let st = self.shared.lock();
            st.jobs
                .iter()
                .map(|(id, e)| (*id, e.name.clone()))
                .collect()
        };
        let ids: Vec<u64> = snapshot
            .into_iter()
            .filter(|(id, name)| filter(JobHandle(*id), name))
            .map(|(id, _)| id)
            .collect();
        self.remove(&ids, interrupt, timeout)
    }

    fn remove(&self, ids: &[u64], interrupt: bool, t: Option<Duration>) -> Result<(), ThreadError> {
        let mut st = self.shared.lock();
        let mut dropped = Vec::new();
        for id in ids {
            let Some(entry) = st.jobs.get_mut(id) else {
                continue;
            };
            if let Some(job) = entry.job.take() {
                dropped.push(job);
                st.jobs.remove(id);
                st.queue.retain(|q| q != id);
            } else {
                entry.removed = true;
                if interrupt {
                    entry.interrupt.store(true, Ordering::Release);
                }
            }
        }
        drop(st);
        // Waiters on a queued job that was just removed must re-check now, not when a
        // worker next finishes a job.
        self.shared.done.notify_all();
        drop(dropped);
        let st = self.shared.lock();
        if self.shared.wait_gone(st, ids, t) {
            Ok(())
        } else {
            Err(ThreadError::JobTimedOut {
                timeout: t.unwrap_or(Duration::MAX),
            })
        }
    }

    /// Waits until `job` has left the pool or `timeout` passes (`None` waits for ever).
    /// Returns `true` if it left.
    #[doc(alias = "waitForJobToFinish")]
    #[must_use = "false means the wait timed out"]
    pub fn wait_for_job_to_finish(&self, job: JobHandle, timeout: Option<Duration>) -> bool {
        let st = self.shared.lock();
        self.shared.wait_gone(st, &[job.0], timeout)
    }

    /// Moves a queued job to the front of the queue. Returns `false` if it is running or
    /// no longer in the pool.
    #[doc(alias = "moveJobToFront")]
    pub fn move_job_to_front(&self, job: JobHandle) -> bool {
        let mut st = self.shared.lock();
        let Some(i) = st.queue.iter().position(|q| *q == job.0) else {
            return false;
        };
        st.queue.remove(i);
        st.queue.push_front(job.0);
        true
    }

    /// Jobs in the pool, queued or running.
    #[doc(alias = "getNumJobs")]
    pub fn num_jobs(&self) -> usize {
        self.shared.lock().jobs.len()
    }

    /// Worker threads in the pool.
    #[doc(alias = "getNumThreads")]
    pub fn num_threads(&self) -> usize {
        self.workers.len()
    }

    /// Whether `job` is still in the pool, queued or running.
    pub fn contains(&self, job: JobHandle) -> bool {
        self.shared.lock().jobs.contains_key(&job.0)
    }

    /// Whether a worker is running `job` right now.
    pub fn is_job_running(&self, job: JobHandle) -> bool {
        self.shared
            .lock()
            .jobs
            .get(&job.0)
            .is_some_and(|e| e.job.is_none())
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        let mut st = self.shared.lock();
        st.shutting_down = true;
        let queued: Vec<_> = st
            .jobs
            .values_mut()
            .filter_map(|e| {
                e.interrupt.store(true, Ordering::Release);
                e.job.take()
            })
            .collect();
        st.queue.clear();
        self.shared.work.notify_all();
        drop(st);
        drop(queued);
        // Each worker returns after its current job; the handles' drop joins them.
        self.workers.clear();
    }
}

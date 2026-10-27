// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Thread priorities, realtime scheduling options and CPU affinity.
//!
//! # Platform mapping
//!
//! | Request | Linux | macOS | Windows |
//! |---|---|---|---|
//! | [`Priority`] | nice value of the thread (`setpriority` on its TID): 15, 5, 0, -5, -10 | QoS class (`pthread_set_qos_class_self_np`): background, utility, default, user-initiated, user-interactive | `SetThreadPriority`: lowest, below normal, normal, above normal, highest |
//! | [`RealtimeOptions`] | `SCHED_FIFO` (`pthread_setschedparam`); priority 0–10 maps linearly onto the lower 80 % of the policy's range | Mach time-constraint policy (`thread_policy_set`) built from the period and work time; the 0–10 priority is not used | MMCSS task "Pro Audio" (`AvSetMmThreadCharacteristicsW`), then `AvSetMmThreadPriority`: 0–2 low, 3–6 normal, 7–8 high, 9–10 critical |
//! | affinity mask | `sched_setaffinity` | unsupported: [`ThreadError::AffinityUnsupported`] | `SetThreadAffinityMask` |
//!
//! Raising a priority usually needs a privilege: `CAP_SYS_NICE`, `RLIMIT_NICE` or
//! `RLIMIT_RTPRIO` on Linux. Refusals never stop a thread from running; see
//! [`ThreadHandle::realtime_status`](crate::ThreadHandle::realtime_status).

use std::time::Duration;

use crate::ThreadError;
use crate::os;

/// A thread's ordinary (non-realtime) scheduling priority. See the [module docs](self) for
/// what each level means on each OS.
///
/// ```
/// use syrp_thread::Priority;
/// assert_eq!(Priority::default(), Priority::Normal);
/// assert!(Priority::Highest > Priority::Low);
/// ```
#[doc(alias = "Thread::Priority")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Priority {
    /// Background work that may be starved.
    Lowest,
    /// Below the default.
    Low,
    /// The OS default for new threads.
    #[default]
    Normal,
    /// Above the default; user-facing work.
    High,
    /// The highest non-realtime priority.
    Highest,
}

/// The highest realtime priority [`RealtimeOptions::with_priority`] accepts.
pub const MAX_REALTIME_PRIORITY: u8 = 10;

/// What a realtime thread needs from the scheduler: a priority from 0 to 10 and, optionally,
/// its processing period and the work it expects to do in each period.
///
/// The default priority is 5. The period and work time feed macOS's time-constraint policy
/// and are checked by [`ThreadBuilder::spawn`](crate::ThreadBuilder::spawn): both must be
/// non-zero and the work time must not exceed the period.
///
/// ```
/// use std::time::Duration;
/// use syrp_thread::RealtimeOptions;
/// let opts = RealtimeOptions::new()
///     .with_priority(12)
///     .with_processing_period(Duration::from_micros(2_667))
///     .with_expected_work_time(Duration::from_millis(1));
/// assert_eq!(opts.priority(), 10); // clamped
/// assert_eq!(opts.processing_period(), Some(Duration::from_micros(2_667)));
/// assert_eq!(opts.expected_work_time(), Some(Duration::from_millis(1)));
/// ```
#[doc(alias = "Thread::RealtimeOptions")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RealtimeOptions {
    priority: u8,
    period: Option<Duration>,
    work: Option<Duration>,
}

impl Default for RealtimeOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl RealtimeOptions {
    /// Priority 5, no period and no expected work time.
    ///
    /// ```
    /// # use syrp_thread::RealtimeOptions;
    /// assert_eq!(RealtimeOptions::new().priority(), 5);
    /// ```
    #[must_use]
    pub const fn new() -> Self {
        Self {
            priority: 5,
            period: None,
            work: None,
        }
    }

    /// Sets the priority, clamped to 0..=[`MAX_REALTIME_PRIORITY`].
    ///
    /// ```
    /// # use syrp_thread::RealtimeOptions;
    /// assert_eq!(RealtimeOptions::new().with_priority(3).priority(), 3);
    /// ```
    #[doc(alias = "withPriority")]
    #[must_use]
    pub const fn with_priority(mut self, priority: u8) -> Self {
        self.priority = if priority > MAX_REALTIME_PRIORITY {
            MAX_REALTIME_PRIORITY
        } else {
            priority
        };
        self
    }

    /// Sets how often the thread wakes to process, such as one audio block.
    ///
    /// ```
    /// # use std::time::Duration;
    /// # use syrp_thread::RealtimeOptions;
    /// let o = RealtimeOptions::new().with_processing_period(Duration::from_millis(5));
    /// assert_eq!(o.processing_period(), Some(Duration::from_millis(5)));
    /// ```
    #[doc(alias = "withPeriodMs")]
    #[must_use]
    pub const fn with_processing_period(mut self, period: Duration) -> Self {
        self.period = Some(period);
        self
    }

    /// Sets how much CPU time the thread expects to need in each period.
    ///
    /// ```
    /// # use std::time::Duration;
    /// # use syrp_thread::RealtimeOptions;
    /// let o = RealtimeOptions::new().with_expected_work_time(Duration::from_millis(1));
    /// assert_eq!(o.expected_work_time(), Some(Duration::from_millis(1)));
    /// ```
    #[doc(alias = "withProcessingTimeMs")]
    #[must_use]
    pub const fn with_expected_work_time(mut self, work: Duration) -> Self {
        self.work = Some(work);
        self
    }

    /// The realtime priority, 0 to 10.
    ///
    /// ```
    /// # use syrp_thread::RealtimeOptions;
    /// assert_eq!(RealtimeOptions::default().priority(), 5);
    /// ```
    #[must_use]
    pub const fn priority(&self) -> u8 {
        self.priority
    }

    /// The processing period, if one was set.
    ///
    /// ```
    /// # use syrp_thread::RealtimeOptions;
    /// assert_eq!(RealtimeOptions::new().processing_period(), None);
    /// ```
    #[must_use]
    pub const fn processing_period(&self) -> Option<Duration> {
        self.period
    }

    /// The expected work time per period, if one was set.
    ///
    /// ```
    /// # use syrp_thread::RealtimeOptions;
    /// assert_eq!(RealtimeOptions::new().expected_work_time(), None);
    /// ```
    #[must_use]
    pub const fn expected_work_time(&self) -> Option<Duration> {
        self.work
    }

    pub(crate) fn check(&self) -> Result<(), ThreadError> {
        let bad = |reason| Err(ThreadError::InvalidRealtimeOptions { reason });
        match (self.period, self.work) {
            (Some(p), _) if p.is_zero() => bad("the processing period is zero"),
            (_, Some(w)) if w.is_zero() => bad("the expected work time is zero"),
            (Some(p), Some(w)) if w > p => bad("the expected work time exceeds the period"),
            _ => Ok(()),
        }
    }
}

/// Whether a scheduling request made at spawn time took effect.
///
/// ```
/// use syrp_thread::{SchedulingStatus, Thread};
/// let t = Thread::builder("plain").spawn(|_| {}).unwrap();
/// assert_eq!(t.realtime_status(), SchedulingStatus::NotRequested);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SchedulingStatus {
    /// Nothing was requested.
    NotRequested,
    /// The OS applied the request.
    Granted,
    /// The OS refused; the thread runs at its ordinary priority.
    #[non_exhaustive]
    Refused {
        /// The OS error code, when the OS reported one.
        os_error: Option<i32>,
    },
}

impl SchedulingStatus {
    pub(crate) fn from_result<T>(r: Result<T, Option<i32>>) -> Self {
        match r {
            Ok(_) => Self::Granted,
            Err(os_error) => Self::Refused { os_error },
        }
    }
}

/// Sets the calling thread's ordinary priority. Returns `SYRP-THREAD-0003` when the OS
/// refuses, which is common when raising it without privileges.
///
/// ```
/// use syrp_thread::{Priority, set_current_thread_priority};
/// std::thread::spawn(|| {
///     // Lowering is allowed everywhere; raising may be refused.
///     let _ = set_current_thread_priority(Priority::Low);
/// }).join().unwrap();
/// ```
#[doc(alias = "setPriority")]
pub fn set_current_thread_priority(priority: Priority) -> Result<(), ThreadError> {
    os::set_priority(priority).map_err(ThreadError::priority)
}

/// Restricts the calling thread to the CPUs whose bits are set in `mask` (bit 0 is CPU 0).
/// Returns `SYRP-THREAD-0004` on macOS, which has no such API, and `SYRP-THREAD-0005` when
/// the OS rejects the mask (for example when it is zero or names no available CPU).
///
/// ```
/// use syrp_thread::{Diagnostic, set_current_thread_affinity_mask};
/// std::thread::spawn(|| {
///     match set_current_thread_affinity_mask(0b1) {
///         Ok(()) => {}
///         Err(e) => assert!(["SYRP-THREAD-0004", "SYRP-THREAD-0005"].contains(&e.code().as_str())),
///     }
/// }).join().unwrap();
/// ```
#[doc(alias = "setCurrentThreadAffinityMask")]
pub fn set_current_thread_affinity_mask(mask: u64) -> Result<(), ThreadError> {
    os::set_affinity(mask)
}

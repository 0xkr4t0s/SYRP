// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The crate's error type and its `SYRP-THREAD-*` codes (catalogued in `errors/thread.toml`).

use std::borrow::Cow;
use std::fmt;
use std::time::Duration;

use syrp_error::{Diagnostic, ErrorCode};

/// Everything that can fail in this crate. Each variant has a stable code.
///
/// ```
/// use std::time::Duration;
/// use syrp_thread::{Diagnostic, Thread};
/// let (release, gate) = std::sync::mpsc::channel::<()>();
/// let t = Thread::builder("stubborn").spawn(move |_| { let _ = gate.recv(); }).unwrap();
/// let err = t.stop(Duration::from_millis(10)).unwrap_err();
/// assert_eq!(err.code().as_str(), "SYRP-THREAD-0001");
/// release.send(()).unwrap();
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ThreadError {
    /// SYRP-THREAD-0001: [`ThreadHandle::stop`](crate::ThreadHandle::stop) signalled the
    /// thread and it did not return within the timeout. The thread was left running, detached.
    #[non_exhaustive]
    StopTimedOut {
        /// How long `stop` waited.
        timeout: Duration,
    },
    /// SYRP-THREAD-0002: the operating system refused to create a thread.
    #[non_exhaustive]
    SpawnFailed {
        /// The OS error code, when the OS reported one.
        os_error: Option<i32>,
    },
    /// SYRP-THREAD-0003: the operating system refused to change the thread's priority.
    #[non_exhaustive]
    PriorityRefused {
        /// The OS error code, when the OS reported one.
        os_error: Option<i32>,
    },
    /// SYRP-THREAD-0004: this platform has no API to pin a thread to CPUs (macOS).
    AffinityUnsupported,
    /// SYRP-THREAD-0005: the operating system rejected the affinity mask.
    #[non_exhaustive]
    AffinityRefused {
        /// The OS error code, when the OS reported one.
        os_error: Option<i32>,
    },
    /// SYRP-THREAD-0006: a [`RealtimeOptions`](crate::RealtimeOptions) value is inconsistent.
    #[non_exhaustive]
    InvalidRealtimeOptions {
        /// Which rule the options broke.
        reason: &'static str,
    },
    /// SYRP-THREAD-0007: a [`ThreadPool`](crate::ThreadPool) remove call waited for a
    /// running job and it did not return within the timeout. The job still leaves the pool
    /// when it returns, and is not run again.
    #[non_exhaustive]
    JobTimedOut {
        /// How long the call waited.
        timeout: Duration,
    },
}

impl ThreadError {
    pub(crate) fn priority(os_error: Option<i32>) -> Self {
        Self::PriorityRefused { os_error }
    }
}

impl Diagnostic for ThreadError {
    fn code(&self) -> ErrorCode {
        ErrorCode::new(match self {
            Self::StopTimedOut { .. } => "SYRP-THREAD-0001",
            Self::SpawnFailed { .. } => "SYRP-THREAD-0002",
            Self::PriorityRefused { .. } => "SYRP-THREAD-0003",
            Self::AffinityUnsupported => "SYRP-THREAD-0004",
            Self::AffinityRefused { .. } => "SYRP-THREAD-0005",
            Self::InvalidRealtimeOptions { .. } => "SYRP-THREAD-0006",
            Self::JobTimedOut { .. } => "SYRP-THREAD-0007",
        })
    }

    fn hint(&self) -> Option<Cow<'static, str>> {
        Some(match self {
            Self::StopTimedOut { .. } => {
                "make the thread body poll `thread_should_exit()` or use `ctx.wait`".into()
            }
            Self::SpawnFailed { .. } => "lower the stack size or the number of threads".into(),
            Self::PriorityRefused { .. } | Self::AffinityRefused { .. } => {
                "the process may lack the privilege; on Linux check RLIMIT_NICE and CPU sets".into()
            }
            Self::AffinityUnsupported => "macOS has no thread affinity; skip the call".into(),
            Self::InvalidRealtimeOptions { .. } => {
                "keep the expected work time non-zero and no longer than the period".into()
            }
            Self::JobTimedOut { .. } => {
                "pass `interrupt = true` and have the job check `ctx.should_exit()`".into()
            }
        })
    }
}

fn os_suffix(f: &mut fmt::Formatter<'_>, os_error: Option<i32>) -> fmt::Result {
    match os_error {
        Some(e) => write!(f, " (OS error {e})"),
        None => Ok(()),
    }
}

impl fmt::Display for ThreadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let code = self.code();
        match self {
            Self::StopTimedOut { timeout } => write!(
                f,
                "{code}: the thread did not exit within {timeout:?} and was detached"
            ),
            Self::SpawnFailed { os_error } => {
                write!(f, "{code}: the OS could not create the thread")?;
                os_suffix(f, *os_error)
            }
            Self::PriorityRefused { os_error } => {
                write!(f, "{code}: the OS refused the thread priority")?;
                os_suffix(f, *os_error)
            }
            Self::AffinityUnsupported => {
                write!(
                    f,
                    "{code}: thread affinity is not supported on this platform"
                )
            }
            Self::AffinityRefused { os_error } => {
                write!(f, "{code}: the OS refused the affinity mask")?;
                os_suffix(f, *os_error)
            }
            Self::InvalidRealtimeOptions { reason } => {
                write!(f, "{code}: invalid realtime options: {reason}")
            }
            Self::JobTimedOut { timeout } => write!(
                f,
                "{code}: the pool job did not return within {timeout:?}; it leaves the pool when it does"
            ),
        }
    }
}

impl std::error::Error for ThreadError {}

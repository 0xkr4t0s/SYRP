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
}

impl Diagnostic for ThreadError {
    fn code(&self) -> ErrorCode {
        ErrorCode::new(match self {
            Self::StopTimedOut { .. } => "SYRP-THREAD-0001",
            Self::SpawnFailed { .. } => "SYRP-THREAD-0002",
        })
    }

    fn hint(&self) -> Option<Cow<'static, str>> {
        Some(match self {
            Self::StopTimedOut { .. } => {
                "make the thread body poll `thread_should_exit()` or use `ctx.wait`".into()
            }
            Self::SpawnFailed { .. } => "lower the stack size or the number of threads".into(),
        })
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
            Self::SpawnFailed { os_error: Some(e) } => {
                write!(
                    f,
                    "{code}: the OS could not create the thread (OS error {e})"
                )
            }
            Self::SpawnFailed { os_error: None } => {
                write!(f, "{code}: the OS could not create the thread")
            }
        }
    }
}

impl std::error::Error for ThreadError {}

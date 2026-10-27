// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The crate's error type and its codes (catalogued in `errors/loop.toml` from `SYRP-LOOP-0004`).

use std::borrow::Cow;
use std::fmt;
use std::io;

use syrp_error::{Diagnostic, ErrorCode};

/// Everything that can fail when creating an [`EventLoop`](crate::EventLoop). Each variant has
/// a stable code; loop operations themselves fail with the contract's
/// [`LoopError`](syrp_loop_api::LoopError).
///
/// ```
/// use syrp_error::Diagnostic;
/// use syrp_events::EventsError;
/// assert_eq!(EventsError::UnsupportedTarget.code().as_str(), "SYRP-LOOP-0006");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EventsError {
    /// SYRP-LOOP-0004: the calling thread already has an `EventLoop`.
    LoopAlreadyOnThread,
    /// SYRP-LOOP-0005: the operating system refused a call the loop needs.
    #[non_exhaustive]
    Os {
        /// The OS call that failed, such as `"kqueue"`.
        call: &'static str,
        /// The OS error number (`errno` or `GetLastError`), when the OS reported one.
        os_error: Option<i32>,
    },
    /// SYRP-LOOP-0006: this target has no owned message loop (iOS, Android, the web).
    UnsupportedTarget,
}

impl EventsError {
    /// The error for a failed OS call, taking the error number from `err`.
    #[cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))]
    pub(crate) fn os(call: &'static str, err: &io::Error) -> Self {
        Self::Os {
            call,
            os_error: err.raw_os_error(),
        }
    }
}

impl Diagnostic for EventsError {
    fn code(&self) -> ErrorCode {
        ErrorCode::new(match self {
            Self::LoopAlreadyOnThread => "SYRP-LOOP-0004",
            Self::Os { .. } => "SYRP-LOOP-0005",
            Self::UnsupportedTarget => "SYRP-LOOP-0006",
        })
    }

    fn hint(&self) -> Option<Cow<'static, str>> {
        Some(match self {
            Self::LoopAlreadyOnThread => {
                "share the existing loop (or its task_sender) instead of creating a second one"
                    .into()
            }
            Self::Os { .. } => "check the process's file-descriptor and handle limits".into(),
            Self::UnsupportedTarget => {
                "on this target, use a hosted loop or ManualLoop from syrp-loop-api".into()
            }
        })
    }
}

impl fmt::Display for EventsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let code = self.code();
        match self {
            Self::LoopAlreadyOnThread => write!(f, "{code}: this thread already has an EventLoop"),
            Self::Os {
                call,
                os_error: Some(e),
            } => {
                let os = io::Error::from_raw_os_error(*e);
                write!(f, "{code}: {call} failed: {os}")
            }
            Self::Os {
                call,
                os_error: None,
            } => {
                write!(f, "{code}: {call} failed")
            }
            Self::UnsupportedTarget => write!(f, "{code}: no owned message loop on this target"),
        }
    }
}

impl std::error::Error for EventsError {}

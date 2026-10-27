// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The crate's error type and its `SYRP-ANALYTICS-*` codes (catalogued in
//! `errors/analytics.toml`).

use std::borrow::Cow;
use std::fmt;

use syrp_error::{Diagnostic, ErrorCode};

/// Everything that can fail in this crate. Each variant has a stable code.
///
/// ```
/// use syrp_analytics::{AnalyticsError, Diagnostic};
/// let err = AnalyticsError::AlreadyRunning;
/// assert_eq!(err.code().as_str(), "SYRP-ANALYTICS-0002");
/// assert!(err.to_string().contains("already running"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AnalyticsError {
    /// SYRP-ANALYTICS-0001: the operating system refused to start the analytics thread.
    #[non_exhaustive]
    ThreadSpawn {
        /// The operating system's description of the failure.
        reason: String,
    },
    /// SYRP-ANALYTICS-0002: the analytics thread is already running.
    AlreadyRunning,
    /// SYRP-ANALYTICS-0003: a batch period under one millisecond.
    #[non_exhaustive]
    BatchPeriodTooShort {
        /// The period that was given.
        period: std::time::Duration,
    },
    /// SYRP-ANALYTICS-0004: the thread name contains a NUL character.
    InvalidThreadName,
}

impl Diagnostic for AnalyticsError {
    fn code(&self) -> ErrorCode {
        ErrorCode::new(match self {
            Self::ThreadSpawn { .. } => "SYRP-ANALYTICS-0001",
            Self::AlreadyRunning => "SYRP-ANALYTICS-0002",
            Self::BatchPeriodTooShort { .. } => "SYRP-ANALYTICS-0003",
            Self::InvalidThreadName => "SYRP-ANALYTICS-0004",
        })
    }

    fn hint(&self) -> Option<Cow<'static, str>> {
        Some(match self {
            Self::ThreadSpawn { .. } => "free resources and start the thread again".into(),
            Self::AlreadyRunning => "stop the running thread first".into(),
            Self::BatchPeriodTooShort { .. } => "use a period of at least 1 ms".into(),
            Self::InvalidThreadName => "remove the NUL character from the name".into(),
        })
    }
}

impl fmt::Display for AnalyticsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let code = self.code();
        match self {
            Self::ThreadSpawn { reason } => {
                write!(f, "{code}: could not start the analytics thread: {reason}")
            }
            Self::AlreadyRunning => write!(f, "{code}: the analytics thread is already running"),
            Self::BatchPeriodTooShort { period } => {
                write!(
                    f,
                    "{code}: batch period {period:?} is under one millisecond"
                )
            }
            Self::InvalidThreadName => write!(f, "{code}: thread name contains a NUL character"),
        }
    }
}

impl std::error::Error for AnalyticsError {}

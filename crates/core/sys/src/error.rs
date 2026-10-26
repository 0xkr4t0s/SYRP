// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The crate's error type and its `SYRP-SYS-*` codes (catalogued in `errors/sys.toml`).

use std::borrow::Cow;
use std::fmt;

use syrp_error::{Diagnostic, ErrorCode};

/// Everything that can fail in this crate. Each variant has a stable code.
///
/// ```
/// use syrp_sys::{Diagnostic, SysError, Time};
/// let err: SysError = Time::from_iso8601("2024-13-01").unwrap_err();
/// assert_eq!(err.code().as_str(), "SYRP-SYS-0001");
/// assert!(err.to_string().contains("month"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SysError {
    /// SYRP-SYS-0001: text given to [`Time::from_iso8601`](crate::Time::from_iso8601) is not
    /// an ISO 8601 date and time this crate accepts.
    #[non_exhaustive]
    InvalidIso8601 {
        /// Byte offset in the input where parsing stopped.
        position: usize,
        /// What was expected there.
        reason: &'static str,
    },
    /// SYRP-SYS-0002: text given to [`Uuid::parse`](crate::Uuid::parse) is not a UUID in the
    /// 36-character hyphenated or 32-character undelimited form.
    #[non_exhaustive]
    InvalidUuid {
        /// Byte offset in the input of the first problem.
        position: usize,
        /// What was expected there.
        reason: &'static str,
    },
    /// SYRP-SYS-0003: the operating system's random number source failed.
    #[non_exhaustive]
    EntropyUnavailable {
        /// The OS error code, when the OS reported one.
        os_error: Option<i32>,
    },
}

impl Diagnostic for SysError {
    fn code(&self) -> ErrorCode {
        ErrorCode::new(match self {
            Self::InvalidIso8601 { .. } => "SYRP-SYS-0001",
            Self::InvalidUuid { .. } => "SYRP-SYS-0002",
            Self::EntropyUnavailable { .. } => "SYRP-SYS-0003",
        })
    }

    fn hint(&self) -> Option<Cow<'static, str>> {
        Some(match self {
            Self::InvalidIso8601 { .. } => {
                "use a form such as 2024-01-31T12:34:56.789+01:00 or 20240131T123456Z".into()
            }
            Self::InvalidUuid { .. } => {
                "use 8-4-4-4-12 hex digits with hyphens, or 32 hex digits without".into()
            }
            Self::EntropyUnavailable { .. } => {
                "retry; on Linux, check that /dev/urandom or getrandom(2) is available".into()
            }
        })
    }
}

impl fmt::Display for SysError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let code = self.code();
        match self {
            Self::InvalidIso8601 { position, reason } => {
                write!(
                    f,
                    "{code}: invalid ISO 8601 date at byte {position}: {reason}"
                )
            }
            Self::InvalidUuid { position, reason } => {
                write!(f, "{code}: invalid UUID at byte {position}: {reason}")
            }
            Self::EntropyUnavailable { os_error: Some(e) } => {
                write!(f, "{code}: the OS random source failed (OS error {e})")
            }
            Self::EntropyUnavailable { os_error: None } => {
                write!(f, "{code}: the OS random source failed")
            }
        }
    }
}

impl std::error::Error for SysError {}

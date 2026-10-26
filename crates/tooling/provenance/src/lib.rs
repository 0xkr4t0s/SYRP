// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Clean-room provenance checks and the contamination scanner (master plan §10.9).
//!
//! - [`provenance`] checks commit trailers and SPDX headers over a git range (B1).
//!
//! The winnowing contamination scanner (B2-B5) follows in the next PR for this WP. This
//! crate never reads JUCE source.
//!
//! ```
//! use syrp_provenance::provenance::check_commit;
//! assert!(check_commit("abc", "Fix\n\nWP: WP-1\nAgent-Model: m").is_empty());
//! ```

use std::fmt;

pub mod provenance;

/// Errors this crate returns. Each carries a `SYRP-CR-*` code from `errors/cleanroom.toml`.
///
/// ```
/// let e = syrp_provenance::Error::Licence("bad".into());
/// assert_eq!(e.code(), "SYRP-CR-0006");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A `git` invocation failed.
    Git(String),
    /// A crate's licence could not be resolved from its `Cargo.toml`.
    Licence(String),
    /// A file could not be read.
    Io(String),
}

impl Error {
    /// The stable `SYRP-CR-*` code for this error.
    ///
    /// ```
    /// assert_eq!(syrp_provenance::Error::Git("x".into()).code(), "SYRP-CR-0005");
    /// ```
    pub fn code(&self) -> &'static str {
        match self {
            Error::Git(_) => "SYRP-CR-0005",
            Error::Licence(_) => "SYRP-CR-0006",
            Error::Io(_) => "SYRP-CR-0007",
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (Error::Git(m) | Error::Licence(m) | Error::Io(m)) = self;
        write!(f, "{}: {m}", self.code())
    }
}

impl std::error::Error for Error {}

/// One failed check, ready to become a diagnostic in a JSON envelope.
///
/// ```
/// let f = syrp_provenance::Finding::new("SYRP-CR-0004", "src/a.rs", "no header");
/// assert_eq!(f.code, "SYRP-CR-0004");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Finding {
    /// The `SYRP-CR-*` code.
    pub code: String,
    /// What the finding is about: a commit SHA or a file path.
    pub subject: String,
    /// Human-readable explanation.
    pub message: String,
}

impl Finding {
    /// Builds a finding.
    ///
    /// ```
    /// let f = syrp_provenance::Finding::new("SYRP-CR-0003", "abc123", "no WP trailer");
    /// assert_eq!(f.subject, "abc123");
    /// ```
    pub fn new(code: &str, subject: impl Into<String>, message: impl Into<String>) -> Self {
        Finding {
            code: code.to_owned(),
            subject: subject.into(),
            message: message.into(),
        }
    }
}

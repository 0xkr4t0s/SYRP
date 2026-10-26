// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Clean-room provenance checks and the contamination scanner (master plan §10.9).
//!
//! - [`provenance`] checks commit trailers and SPDX headers over a git range.
//! - [`scanner`] turns source text into winnowed fingerprints.
//! - [`database`] defines the fingerprint database format the quarantined job produces.
//! - [`report`] scores files against a database and renders the JSON report.
//!
//! This crate never reads JUCE source: it only consumes a [`database::Database`].
//!
//! ```
//! use syrp_provenance::{database::Database, report::scan};
//! let db = Database::from_sources(&["fn main() { let alpha = 1; let beta = 2; }"], 4, 2);
//! let report = scan(&db, &[("a.rs".into(), "fn other() {}".into())], 0.5);
//! assert!(report.passed());
//! ```

use std::fmt;

pub mod database;
pub mod provenance;
pub mod report;
pub mod scanner;

/// Errors this crate returns. Each carries a `SYRP-CR-*` code from `errors/cleanroom.toml`.
///
/// ```
/// let e = syrp_provenance::Error::Database("bad".into());
/// assert_eq!(e.code(), "SYRP-CR-0002");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The fingerprint database is unreadable, malformed or uses another normaliser.
    Database(String),
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
            Error::Database(_) => "SYRP-CR-0002",
            Error::Git(_) => "SYRP-CR-0005",
            Error::Licence(_) => "SYRP-CR-0006",
            Error::Io(_) => "SYRP-CR-0007",
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (Error::Database(m) | Error::Git(m) | Error::Licence(m) | Error::Io(m)) = self;
        write!(f, "{}: {m}", self.code())
    }
}

impl std::error::Error for Error {}

/// One failed check, ready to become a diagnostic in a JSON envelope.
///
/// ```
/// let f = syrp_provenance::Finding::new("SYRP-CR-0001", "src/a.rs", "too similar");
/// assert_eq!(f.code, "SYRP-CR-0001");
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

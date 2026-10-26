// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The crate's error type and its `SYRP-PROG-*` codes (catalogued in `errors/progress.toml`).

use std::borrow::Cow;
use std::fmt;
use std::path::PathBuf;

use syrp_error::{Diagnostic, ErrorCode};

/// Everything that can stop the progress page from being built. Each variant has a stable code.
///
/// ```
/// use syrp_progress::{Diagnostic, ProgressError};
/// let e = ProgressError::Git { detail: "not a git repository".into() };
/// assert_eq!(e.code().as_str(), "SYRP-PROG-0003");
/// assert!(e.to_string().contains("not a git repository"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgressError {
    /// SYRP-PROG-0001: a `work/` board file is missing or invalid.
    Board {
        /// The file or directory.
        path: PathBuf,
        /// What went wrong.
        detail: String,
    },
    /// SYRP-PROG-0002: the parity inventory or ledger is missing or invalid.
    Parity {
        /// The file.
        path: PathBuf,
        /// What went wrong.
        detail: String,
    },
    /// SYRP-PROG-0003: the git history could not be read.
    Git {
        /// What git reported.
        detail: String,
    },
    /// SYRP-PROG-0004: the open-PR file is unreadable or not a JSON list of `{number, title}`.
    OpenPrs {
        /// The file.
        path: PathBuf,
        /// What went wrong.
        detail: String,
    },
    /// SYRP-PROG-0005: an output file could not be written.
    Write {
        /// The file.
        path: PathBuf,
        /// The I/O error.
        detail: String,
    },
}

impl Diagnostic for ProgressError {
    fn code(&self) -> ErrorCode {
        match self {
            Self::Board { .. } => ErrorCode::new("SYRP-PROG-0001"),
            Self::Parity { .. } => ErrorCode::new("SYRP-PROG-0002"),
            Self::Git { .. } => ErrorCode::new("SYRP-PROG-0003"),
            Self::OpenPrs { .. } => ErrorCode::new("SYRP-PROG-0004"),
            Self::Write { .. } => ErrorCode::new("SYRP-PROG-0005"),
        }
    }

    fn hint(&self) -> Option<Cow<'static, str>> {
        Some(match self {
            Self::Board { .. } => "run `cargo syrp wp validate` and fix the file it names".into(),
            Self::Parity { .. } => "run `python3 parity/inventory/validate.py`".into(),
            Self::Git { .. } => {
                "run inside a git checkout with full history (`fetch-depth: 0` in CI)".into()
            }
            Self::OpenPrs { .. } => {
                "produce it with `gh pr list --state open --json number,title`".into()
            }
            Self::Write { .. } => "check that the output directory is writable".into(),
        })
    }
}

impl fmt::Display for ProgressError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            Self::Board { path, detail } => format!("board {}: {detail}", path.display()),
            Self::Parity { path, detail } => format!("parity {}: {detail}", path.display()),
            Self::Git { detail } => format!("git history: {detail}"),
            Self::OpenPrs { path, detail } => format!("open PRs {}: {detail}", path.display()),
            Self::Write { path, detail } => format!("writing {}: {detail}", path.display()),
        };
        write!(f, "{}: {msg}", self.code())
    }
}

impl std::error::Error for ProgressError {}

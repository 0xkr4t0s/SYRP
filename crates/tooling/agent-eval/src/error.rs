//! The harness's error type and its `SYRP-EVAL-*` codes (catalogued in `errors/eval.toml`).

use std::borrow::Cow;
use std::fmt;
use std::path::PathBuf;

use syrp_error::{Diagnostic, ErrorCode};

/// Everything that stops the harness itself (a failing task is a result, not an error).
///
/// ```
/// use syrp_agent_eval::EvalError;
/// use syrp_error::Diagnostic;
/// let e = EvalError::Spawn { program: "sh".into(), detail: "not found".into() };
/// assert_eq!(e.code().as_str(), "SYRP-EVAL-0003");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EvalError {
    /// SYRP-EVAL-0001: a task file is unreadable, not valid TOML or misses a required field.
    TaskFile {
        /// The task file.
        path: PathBuf,
        /// What is wrong with it.
        detail: String,
    },
    /// SYRP-EVAL-0002: the tasks directory or the work directory could not be read or written.
    Io {
        /// The path involved.
        path: PathBuf,
        /// The underlying I/O error.
        detail: String,
    },
    /// SYRP-EVAL-0003: the shell that runs setup, agent and verifier scripts could not start.
    Spawn {
        /// The program that failed to start.
        program: String,
        /// The underlying error.
        detail: String,
    },
    /// SYRP-EVAL-0004: the task selection or agent configuration is unusable.
    Config {
        /// What is wrong.
        detail: String,
    },
}

impl Diagnostic for EvalError {
    fn code(&self) -> ErrorCode {
        match self {
            Self::TaskFile { .. } => ErrorCode::new("SYRP-EVAL-0001"),
            Self::Io { .. } => ErrorCode::new("SYRP-EVAL-0002"),
            Self::Spawn { .. } => ErrorCode::new("SYRP-EVAL-0003"),
            Self::Config { .. } => ErrorCode::new("SYRP-EVAL-0004"),
        }
    }

    fn hint(&self) -> Option<Cow<'static, str>> {
        Some(match self {
            Self::TaskFile { .. } => {
                "a task needs id, title, prompt, setup, verify and reference strings".into()
            }
            Self::Io { .. } => "check the path exists and is writable".into(),
            Self::Spawn { .. } => "put a POSIX `sh` on PATH (Git Bash on Windows)".into(),
            Self::Config { .. } => "run `syrp-agent-eval run --help`".into(),
        })
    }
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TaskFile { path, detail } => write!(f, "task {}: {detail}", path.display()),
            Self::Io { path, detail } => write!(f, "{}: {detail}", path.display()),
            Self::Spawn { program, detail } => write!(f, "cannot start `{program}`: {detail}"),
            Self::Config { detail } => write!(f, "configuration: {detail}"),
        }
    }
}

impl std::error::Error for EvalError {}

pub(crate) fn io(path: &std::path::Path, e: &std::io::Error) -> EvalError {
    EvalError::Io {
        path: path.to_path_buf(),
        detail: e.to_string(),
    }
}

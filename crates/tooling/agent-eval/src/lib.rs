//! Agent-eval harness v0 (master plan §2, A20): a fixed task set scored automatically.
//!
//! Each task is a TOML file in `evals/tasks/` with a prompt, a setup script, a deterministic
//! verifier script and a reference solution (see [`Task`]). [`run`] gives every task a fresh
//! workspace directory, runs setup, then the configured [`Agent`], then the verifier, and
//! returns a [`Report`]: pass/fail and timing per task plus a success-rate KPI for the exit
//! evaluator. Agents are external commands, so the harness itself needs no API keys.
//! The binary is `syrp-agent-eval run`; see the crate's AGENTS.md.

mod error;
mod runner;

use std::path::Path;
use std::time::Duration;

pub use error::EvalError;
pub use runner::{Agent, Report, RunConfig, run};

/// The `schema` value of every report.
pub const REPORT_SCHEMA: &str = "syrp.agent-eval/v1";

/// Default limit for each phase (setup, agent, verify) of a task.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(600);

/// One eval task, parsed from `evals/tasks/<id>.toml`.
///
/// Scripts are POSIX `sh` (with `set -e`) run in the task's workspace. They see `EVAL_TASK`,
/// `EVAL_WORKSPACE`, `EVAL_PROMPT_FILE`, `SYRP_ROOT` (the repository under test) and a shell
/// function `syrp` that runs the Syrp CLI.
///
/// ```
/// use syrp_agent_eval::Task;
/// let src = "id = 'demo'\ntitle = 'Demo'\nprompt = 'Write ok to answer.txt.'\n\
///            setup = 'rm -f answer.txt'\nverify = 'test \"$(cat answer.txt)\" = ok'\n\
///            reference = 'echo ok > answer.txt'\ntimeout_secs = 5";
/// let t = Task::from_toml(src, "evals/tasks/demo.toml".as_ref())?;
/// assert_eq!((t.id(), t.timeout().as_secs()), ("demo", 5));
/// # Ok::<(), syrp_agent_eval::EvalError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    file: TaskFile,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskFile {
    id: String,
    title: String,
    prompt: String,
    setup: String,
    verify: String,
    reference: String,
    timeout_secs: Option<u64>,
}

impl Task {
    /// Parses a task file; its `id` must equal the stem of `path`, and no field may be empty.
    ///
    /// ```
    /// use syrp_agent_eval::Task;
    /// let err = Task::from_toml("id = 'x'", "x.toml".as_ref()).unwrap_err();
    /// assert!(err.to_string().contains("title"));
    /// ```
    pub fn from_toml(src: &str, path: &Path) -> Result<Task, EvalError> {
        let bad = |detail: String| EvalError::TaskFile {
            path: path.to_path_buf(),
            detail,
        };
        let file: TaskFile = toml::from_str(src).map_err(|e| bad(e.to_string()))?;
        let TaskFile {
            id,
            title,
            prompt,
            setup,
            verify,
            reference,
            timeout_secs: _,
        } = &file;
        if path.file_stem().and_then(|s| s.to_str()) != Some(id.as_str()) {
            return Err(bad(format!("id `{id}` does not match the file name")));
        }
        let parts = [title, prompt, setup, verify, reference];
        let keys = ["title", "prompt", "setup", "verify", "reference"];
        if let Some(i) = parts.iter().position(|v| v.trim().is_empty()) {
            return Err(bad(format!("`{}` is empty", keys[i])));
        }
        Ok(Task { file })
    }

    /// The task id (the file stem).
    ///
    /// ```
    /// fn f(t: &syrp_agent_eval::Task) -> &str { t.id() }
    /// ```
    #[must_use]
    pub fn id(&self) -> &str {
        &self.file.id
    }

    /// The prompt handed to the agent.
    ///
    /// ```
    /// fn f(t: &syrp_agent_eval::Task) -> &str { t.prompt() }
    /// ```
    #[must_use]
    pub fn prompt(&self) -> &str {
        &self.file.prompt
    }

    /// The limit for each phase (`timeout_secs`, default [`DEFAULT_TIMEOUT`]).
    ///
    /// ```
    /// fn f(t: &syrp_agent_eval::Task) -> u64 { t.timeout().as_secs() }
    /// ```
    #[must_use]
    pub fn timeout(&self) -> Duration {
        self.file
            .timeout_secs
            .map_or(DEFAULT_TIMEOUT, Duration::from_secs)
    }
}

/// Loads every `*.toml` in `dir`, sorted by file name.
///
/// ```no_run
/// let tasks = syrp_agent_eval::load_tasks(std::path::Path::new("evals/tasks"))?;
/// assert_eq!(tasks.len(), 10);
/// # Ok::<(), syrp_agent_eval::EvalError>(())
/// ```
pub fn load_tasks(dir: &Path) -> Result<Vec<Task>, EvalError> {
    let entries = std::fs::read_dir(dir).map_err(|e| error::io(dir, &e))?;
    let mut paths: Vec<_> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .collect();
    paths.sort();
    paths
        .iter()
        .map(|p| {
            let src = std::fs::read_to_string(p).map_err(|e| error::io(p, &e))?;
            Task::from_toml(&src, p)
        })
        .collect()
}

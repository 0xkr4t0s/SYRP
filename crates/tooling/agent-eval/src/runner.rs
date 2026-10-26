//! Running tasks: workspaces, the three phases, timing and the report.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;

use crate::error::io;
use crate::{EvalError, REPORT_SCHEMA, Task};

/// Who solves the task between setup and verification.
///
/// ```
/// use syrp_agent_eval::Agent;
/// let a = Agent::Command("my-agent --prompt {prompt_file} --cwd {workspace}".into());
/// assert_ne!(a, Agent::Reference);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Agent {
    /// Nobody: the verifier runs on the untouched setup (a baseline that must fail).
    None,
    /// Each task's own reference solution (self-test of the task set; offline, no keys).
    Reference,
    /// An external agent, run with `sh -c` after substituting `{prompt_file}`, `{workspace}`,
    /// `{root}` and `{task}` with shell-quoted values. This is the opt-in model-in-the-loop mode.
    Command(String),
}

impl Agent {
    fn label(&self) -> &'static str {
        match self {
            Agent::None => "none",
            Agent::Reference => "reference",
            Agent::Command(_) => "command",
        }
    }
}

/// Where and how to run tasks.
///
/// ```
/// use syrp_agent_eval::{Agent, RunConfig};
/// let _cfg = RunConfig::new(".", "target/agent-eval").agent(Agent::Reference);
/// ```
#[derive(Debug, Clone)]
pub struct RunConfig {
    root: PathBuf,
    work_dir: PathBuf,
    agent: Agent,
    syrp: String,
}

impl RunConfig {
    /// `root` is the Syrp checkout under test (`SYRP_ROOT`); each task gets
    /// `work_dir/<id>/` as its workspace, plus `<id>.prompt.md` and `<id>.log` beside it.
    /// Defaults: [`Agent::None`], `syrp` = `cargo run` of the CLI in `root`.
    ///
    /// ```
    /// let _cfg = syrp_agent_eval::RunConfig::new("/repo", "/tmp/eval");
    /// ```
    pub fn new(root: impl Into<PathBuf>, work_dir: impl Into<PathBuf>) -> Self {
        RunConfig {
            root: root.into(),
            work_dir: work_dir.into(),
            agent: Agent::None,
            syrp: "cargo run --quiet --manifest-path \"$SYRP_ROOT/Cargo.toml\" \
                   --package syrp-cli --bin cargo-syrp --"
                .into(),
        }
    }

    /// Sets the agent.
    ///
    /// ```
    /// use syrp_agent_eval::{Agent, RunConfig};
    /// let _cfg = RunConfig::new(".", "w").agent(Agent::Reference);
    /// ```
    #[must_use]
    pub fn agent(mut self, agent: Agent) -> Self {
        self.agent = agent;
        self
    }

    /// Sets the shell words the `syrp` function expands to, for example `"/bin/eval" syrp`.
    ///
    /// ```
    /// let _cfg = syrp_agent_eval::RunConfig::new(".", "w").syrp("cargo syrp");
    /// ```
    #[must_use]
    pub fn syrp(mut self, words: impl Into<String>) -> Self {
        self.syrp = words.into();
        self
    }
}

#[derive(Debug, Clone, Serialize)]
struct Phase {
    name: &'static str,
    exit_code: Option<i32>,
    duration_ms: u64,
    timed_out: bool,
}

#[derive(Debug, Clone, Serialize)]
struct TaskResult {
    id: String,
    title: String,
    passed: bool,
    failed_phase: Option<&'static str>,
    duration_ms: u64,
    phases: Vec<Phase>,
    log_tail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
struct Summary {
    total: usize,
    passed: usize,
    failed: usize,
    success_rate: f64,
}

/// A whole run. [`Report::to_json`] has, per task, `id`, `title`, `passed`, `failed_phase`,
/// `duration_ms`, `phases[{name, exit_code, duration_ms, timed_out}]` and `log_tail`, plus
/// `summary{total, passed, failed, success_rate}`: the KPI the exit evaluator reads.
///
/// ```
/// fn kpi(r: &syrp_agent_eval::Report) -> f64 { r.success_rate() }
/// ```
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    schema: &'static str,
    agent: &'static str,
    tasks: Vec<TaskResult>,
    summary: Summary,
}

impl Report {
    /// passed / total in `0.0..=1.0` (0 for an empty run).
    ///
    /// ```
    /// fn kpi(r: &syrp_agent_eval::Report) -> f64 { r.success_rate() }
    /// ```
    #[must_use]
    pub fn success_rate(&self) -> f64 {
        self.summary.success_rate
    }

    /// The report as JSON (`schema` = [`REPORT_SCHEMA`]).
    ///
    /// ```
    /// fn s(r: &syrp_agent_eval::Report) -> String { r.to_json().to_string() }
    /// ```
    #[must_use]
    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

/// Runs `tasks` in order and scores them. Task failures are results; only problems with the
/// harness itself (unwritable work directory, missing shell) are errors.
///
/// ```no_run
/// use syrp_agent_eval::{RunConfig, run};
/// let report = run(&[], &RunConfig::new(".", "target/agent-eval"))?;
/// assert_eq!(report.success_rate(), 0.0);
/// # Ok::<(), syrp_agent_eval::EvalError>(())
/// ```
pub fn run(tasks: &[Task], cfg: &RunConfig) -> Result<Report, EvalError> {
    fs::create_dir_all(&cfg.work_dir).map_err(|e| io(&cfg.work_dir, &e))?;
    let work = std::path::absolute(&cfg.work_dir).map_err(|e| io(&cfg.work_dir, &e))?;
    let root = std::path::absolute(&cfg.root).map_err(|e| io(&cfg.root, &e))?;
    // An empty git config isolates tasks from the user's (signing, hooks, templates).
    let gitconfig = work.join("gitconfig");
    File::create(&gitconfig).map_err(|e| io(&gitconfig, &e))?;
    let mut results = Vec::with_capacity(tasks.len());
    for task in tasks {
        results.push(run_task(task, cfg, &work, &root)?);
    }
    let passed = results.iter().filter(|r| r.passed).count();
    let total = results.len();
    let success_rate = if total == 0 {
        0.0
    } else {
        passed as f64 / total as f64
    };
    Ok(Report {
        schema: REPORT_SCHEMA,
        agent: cfg.agent.label(),
        tasks: results,
        summary: Summary {
            total,
            passed,
            failed: total - passed,
            success_rate,
        },
    })
}

fn shell_path(p: &Path) -> String {
    let s = p.display().to_string();
    if cfg!(windows) {
        s.replace('\\', "/")
    } else {
        s
    }
}

fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn run_task(
    task: &Task,
    cfg: &RunConfig,
    work: &Path,
    root: &Path,
) -> Result<TaskResult, EvalError> {
    let ws = work.join(task.id());
    if ws.exists() {
        fs::remove_dir_all(&ws).map_err(|e| io(&ws, &e))?;
    }
    fs::create_dir_all(&ws).map_err(|e| io(&ws, &e))?;
    let prompt_file = work.join(format!("{}.prompt.md", task.id()));
    fs::write(&prompt_file, task.prompt()).map_err(|e| io(&prompt_file, &e))?;
    let log = work.join(format!("{}.log", task.id()));
    File::create(&log).map_err(|e| io(&log, &e))?;

    let (ws_s, root_s, prompt_s) = (shell_path(&ws), shell_path(root), shell_path(&prompt_file));
    let agent_script = match &cfg.agent {
        Agent::None => None,
        Agent::Reference => Some(task.file.reference.to_string()),
        Agent::Command(template) => Some(
            template
                .replace("{prompt_file}", &quote(&prompt_s))
                .replace("{workspace}", &quote(&ws_s))
                .replace("{root}", &quote(&root_s))
                .replace("{task}", &quote(task.id())),
        ),
    };
    let env = [
        ("EVAL_TASK", task.id().to_string()),
        ("EVAL_WORKSPACE", ws_s),
        ("EVAL_PROMPT_FILE", prompt_s),
        ("SYRP_ROOT", root_s),
        ("GIT_CONFIG_GLOBAL", shell_path(&work.join("gitconfig"))),
        ("GIT_CONFIG_NOSYSTEM", "1".into()),
        ("GIT_AUTHOR_NAME", "syrp-eval".into()),
        ("GIT_AUTHOR_EMAIL", "eval@example.invalid".into()),
        ("GIT_COMMITTER_NAME", "syrp-eval".into()),
        ("GIT_COMMITTER_EMAIL", "eval@example.invalid".into()),
    ];

    let start = Instant::now();
    let mut phases = Vec::with_capacity(3);
    let mut failed_phase = None;
    let plan = [
        ("setup", Some(task.file.setup.to_string())),
        ("agent", agent_script),
        ("verify", Some(task.file.verify.to_string())),
    ];
    for (name, script) in plan {
        let Some(script) = script else { continue };
        let phase = exec(cfg, name, &script, &ws, &env, task.timeout(), &log)?;
        let ok = !phase.timed_out && phase.exit_code == Some(0);
        phases.push(phase);
        if !ok {
            failed_phase = Some(name);
            break;
        }
    }
    let text = fs::read_to_string(&log).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    let keep = if failed_phase.is_some() { 20 } else { 0 };
    let log_tail = lines[lines.len().saturating_sub(keep)..].join("\n");
    Ok(TaskResult {
        id: task.id().to_string(),
        title: task.file.title.to_string(),
        passed: failed_phase.is_none(),
        failed_phase,
        duration_ms: start.elapsed().as_millis() as u64,
        phases,
        log_tail,
    })
}

fn exec(
    cfg: &RunConfig,
    name: &'static str,
    script: &str,
    ws: &Path,
    env: &[(&str, String)],
    timeout: Duration,
    log: &Path,
) -> Result<Phase, EvalError> {
    let mut out = OpenOptions::new()
        .append(true)
        .open(log)
        .map_err(|e| io(log, &e))?;
    writeln!(out, "== {name} ==").map_err(|e| io(log, &e))?;
    let err = out.try_clone().map_err(|e| io(log, &e))?;
    let body = format!("set -e\nsyrp() {{ {} \"$@\"; }}\n{script}", cfg.syrp);
    let start = Instant::now();
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(body)
        .current_dir(ws)
        .envs(env.iter().map(|(k, v)| (*k, v.as_str())))
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        .spawn()
        .map_err(|e| EvalError::Spawn {
            program: "sh".into(),
            detail: e.to_string(),
        })?;
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if start.elapsed() >= timeout => {
                timed_out = true;
                let _ = child.kill();
                break child.wait().ok();
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(_) => break None,
        }
    };
    Ok(Phase {
        name,
        exit_code: status.filter(|_| !timed_out).and_then(|s| s.code()),
        duration_ms: start.elapsed().as_millis() as u64,
        timed_out,
    })
}

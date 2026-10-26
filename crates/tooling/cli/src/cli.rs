//! Command-line definitions and dispatch.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::{Args, Parser, Subcommand};
use serde_json::{Value, json};

use crate::envelope::{Diagnostic, ExitCode, Outcome};
use crate::wp::{Board, Ownership, SpecStatus};
use crate::{catalogue, checks, conductor, docs, gates, repo, review};

/// The Syrp closed-loop CLI. Output is JSON when stdout is not a terminal, or with --json.
#[derive(Debug, Parser)]
#[command(name = "cargo-syrp", bin_name = "cargo syrp", version)]
pub struct Cli {
    /// Force JSON output.
    #[arg(long, global = true)]
    pub json: bool,
    /// Repository root (default: search upwards from the current directory).
    #[arg(long, global = true)]
    pub root: Option<PathBuf>,
    /// The command.
    #[command(subcommand)]
    pub command: Cmd,
}

/// Top-level commands.
#[derive(Debug, Subcommand)]
pub enum Cmd {
    /// Diagnose the toolchain and repository. Run once per session.
    Doctor,
    /// Explain an SYRP-* error code: cause, fix, example.
    Explain {
        /// The code, for example SYRP-WP-0005.
        code: Option<String>,
        /// List every code.
        #[arg(long)]
        list: bool,
    },
    /// The fast gate: crate lints, layering, size, banned crates, WP specs, fmt and clippy.
    Check {
        /// Skip fmt and clippy (no compilation).
        #[arg(long)]
        lints_only: bool,
    },
    /// Run tests with a structured summary.
    Test {
        /// Limit to one package.
        #[arg(long = "crate")]
        krate: Option<String>,
    },
    /// Work-package helpers.
    #[command(subcommand)]
    Wp(WpCmd),
    /// The deterministic scheduler used by the conductor workflow.
    #[command(subcommand)]
    Conductor(ConductorCmd),
    /// PR-manager helpers.
    #[command(subcommand)]
    Pr(PrCmd),
    /// Reviewer helpers.
    #[command(subcommand)]
    Review(ReviewCmd),
    /// Documentation: build both sides, or search offline.
    #[command(subcommand)]
    Docs(DocsCmd),
    /// Print the JSON Schema of the result envelope.
    Schema,
}

/// `wp` subcommands.
#[derive(Debug, Subcommand)]
pub enum WpCmd {
    /// List every work package.
    List,
    /// Everything an agent needs to start a WP: definition, spec, owned paths, dependencies.
    Show {
        /// WP id.
        id: String,
    },
    /// WPs whose dependencies are done, from the spec files alone (the conductor also uses GitHub state).
    Ready {
        /// Extra WP ids to treat as done.
        #[arg(long, value_delimiter = ',')]
        done: Vec<String>,
    },
    /// Validate every spec and the dependency graph.
    Validate,
    /// The acceptance command: lints, the WP crate's clippy and tests, and path ownership.
    Check {
        /// WP id.
        id: String,
        /// Git ref to diff against for the ownership check.
        #[arg(long, default_value = "origin/main")]
        base: String,
    },
    /// Check that changed files stay inside the WP's ownership.
    CheckPaths {
        /// WP id.
        id: String,
        /// Files to check; default is `git diff --name-only <base>...HEAD`.
        #[arg(long, value_delimiter = ',')]
        files: Vec<String>,
        /// Git ref to diff against.
        #[arg(long, default_value = "origin/main")]
        base: String,
    },
}

/// `conductor` subcommands.
#[derive(Debug, Subcommand)]
pub enum ConductorCmd {
    /// Compute actions from GitHub state.
    Tick(TickArgs),
    /// After an agent run: the issue's new body and status label.
    Finish {
        /// File with `gh issue view <n> --json number,title,state,labels,body`.
        #[arg(long)]
        issue: PathBuf,
        /// The WP has an open PR now.
        #[arg(long)]
        has_pr: bool,
    },
}

/// Inputs for one tick.
#[derive(Debug, Args)]
pub struct TickArgs {
    /// File with `gh issue list --state all --label <wp labels> --json number,title,state,labels,body`.
    #[arg(long)]
    pub issues: PathBuf,
    /// File with `gh pr list --state open --json number,isDraft,labels,headRefName`.
    #[arg(long)]
    pub prs: PathBuf,
    /// Implementer slots.
    #[arg(long, default_value_t = 12)]
    pub slots: usize,
    /// Lease time-to-live in seconds.
    #[arg(long, default_value_t = 4 * 3600)]
    pub lease_ttl: u64,
    /// Current unix time (default: now).
    #[arg(long)]
    pub now: Option<u64>,
}

/// `pr` subcommands.
#[derive(Debug, Subcommand)]
pub enum PrCmd {
    /// Evaluate merge gates for a PR.
    Gates {
        /// File with `gh pr view <n> --json number,body,labels,files,commits,statusCheckRollup,mergeable,isDraft`.
        #[arg(long)]
        pr: PathBuf,
    },
}

/// `review` subcommands.
#[derive(Debug, Subcommand)]
pub enum ReviewCmd {
    /// Print the checklist a review must answer.
    Checklist,
    /// Validate a review file and derive its verdict and comment body.
    Validate {
        /// Review JSON file.
        file: PathBuf,
    },
}

/// `docs` subcommands.
#[derive(Debug, Subcommand)]
pub enum DocsCmd {
    /// Build the human site (mdBook) and the agent side into one directory.
    Build {
        /// Output directory.
        #[arg(long, default_value = "target/site")]
        out: PathBuf,
        /// Only build the agent side.
        #[arg(long)]
        skip_book: bool,
    },
    /// Search docs, error codes and work packages offline.
    Search {
        /// Words to look for.
        query: Vec<String>,
        /// Maximum hits.
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
}

impl Cmd {
    /// The command's name as reported in the envelope.
    pub fn name(&self) -> String {
        match self {
            Cmd::Doctor => "doctor".into(),
            Cmd::Explain { .. } => "explain".into(),
            Cmd::Check { .. } => "check".into(),
            Cmd::Test { .. } => "test".into(),
            Cmd::Wp(c) => format!(
                "wp {}",
                match c {
                    WpCmd::List => "list",
                    WpCmd::Show { .. } => "show",
                    WpCmd::Ready { .. } => "ready",
                    WpCmd::Validate => "validate",
                    WpCmd::Check { .. } => "check",
                    WpCmd::CheckPaths { .. } => "check-paths",
                }
            ),
            Cmd::Conductor(c) => format!(
                "conductor {}",
                match c {
                    ConductorCmd::Tick(_) => "tick",
                    ConductorCmd::Finish { .. } => "finish",
                }
            ),
            Cmd::Pr(_) => "pr gates".into(),
            Cmd::Review(c) => format!(
                "review {}",
                match c {
                    ReviewCmd::Checklist => "checklist",
                    ReviewCmd::Validate { .. } => "validate",
                }
            ),
            Cmd::Docs(c) => format!(
                "docs {}",
                match c {
                    DocsCmd::Build { .. } => "build",
                    DocsCmd::Search { .. } => "search",
                }
            ),
            Cmd::Schema => "schema".into(),
        }
    }
}

fn fail(d: Diagnostic, exit: ExitCode) -> Outcome {
    Outcome {
        diagnostics: vec![d],
        exit: Some(exit),
        ..Default::default()
    }
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, Outcome> {
    let src = fs::read_to_string(path).map_err(|e| {
        fail(
            Diagnostic::error("SYRP-CLI-0003", format!("{}: {e}", path.display())),
            ExitCode::Usage,
        )
    })?;
    serde_json::from_str(&src).map_err(|e| {
        fail(
            Diagnostic::error("SYRP-CLI-0003", format!("{}: {e}", path.display())),
            ExitCode::Usage,
        )
    })
}

fn changed_files(root: &Path, base: &str) -> Result<Vec<String>, Diagnostic> {
    let out = Command::new("git")
        .args(["diff", "--name-only", &format!("{base}...HEAD")])
        .current_dir(root)
        .output()
        .map_err(|e| Diagnostic::error("SYRP-CLI-0101", format!("git: {e}")))?;
    if !out.status.success() {
        return Err(Diagnostic::error(
            "SYRP-CLI-0004",
            format!(
                "git diff against {base} failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        )
        .hint("fetch the base branch, or pass --base"));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect())
}

/// Runs a command and returns its outcome.
pub fn dispatch(cli: &Cli) -> Outcome {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let root = match &cli.root {
        Some(r) => Ok(r.clone()),
        None => repo::find_root(&cwd),
    };
    // Commands that work without a repository.
    match &cli.command {
        Cmd::Doctor => return checks::doctor(root.as_deref().ok()),
        Cmd::Explain { list: true, .. } => {
            return Outcome {
                results: catalogue::all()
                    .into_iter()
                    .map(|e| json!({"code": e.code, "title": e.title}))
                    .collect(),
                ..Default::default()
            };
        }
        Cmd::Explain { code: Some(c), .. } => return catalogue::explain(c),
        Cmd::Explain { .. } => {
            return fail(
                Diagnostic::error("SYRP-CLI-0002", "give a code or --list"),
                ExitCode::Usage,
            );
        }
        Cmd::Schema => {
            return Outcome::result(
                serde_json::from_str(crate::ENVELOPE_JSON_SCHEMA).expect("schema"),
            );
        }
        Cmd::Review(ReviewCmd::Checklist) => {
            return Outcome {
                results: review::CHECKLIST
                    .iter()
                    .map(|(id, d)| json!({"id": id, "description": d}))
                    .collect(),
                ..Default::default()
            };
        }
        Cmd::Review(ReviewCmd::Validate { file }) => {
            return match fs::read_to_string(file) {
                Ok(s) => review::validate(&s),
                Err(e) => fail(
                    Diagnostic::error("SYRP-CLI-0003", format!("{}: {e}", file.display())),
                    ExitCode::Usage,
                ),
            };
        }
        _ => {}
    }
    let root = match root {
        Ok(r) => r,
        Err(d) => return fail(d, ExitCode::Usage),
    };

    match &cli.command {
        Cmd::Check { lints_only } => checks::check(&root, *lints_only),
        Cmd::Test { krate } => checks::test(&root, krate.as_deref()),
        Cmd::Wp(c) => wp_cmd(&root, c),
        Cmd::Conductor(c) => conductor_cmd(&root, c),
        Cmd::Pr(PrCmd::Gates { pr }) => {
            let view: gates::PrView = match read_json(pr) {
                Ok(v) => v,
                Err(o) => return o,
            };
            let (board, _) = Board::load(&root);
            match Ownership::load(&root) {
                Ok(own) => gates::evaluate(&view, &board, &own),
                Err(d) => fail(d, ExitCode::Failed),
            }
        }
        Cmd::Docs(DocsCmd::Build { out, skip_book }) => {
            let out = if out.is_absolute() {
                out.clone()
            } else {
                root.join(out)
            };
            docs::build(&root, &out, *skip_book)
        }
        Cmd::Docs(DocsCmd::Search { query, limit }) => {
            docs::search(&root, &query.join(" "), *limit)
        }
        Cmd::Doctor | Cmd::Explain { .. } | Cmd::Schema | Cmd::Review(_) => {
            unreachable!("handled above")
        }
    }
}

fn wp_cmd(root: &Path, c: &WpCmd) -> Outcome {
    let (board, mut diags) = Board::load(root);
    match c {
        WpCmd::List => Outcome {
            results: board
                .packages
                .values()
                .map(|wp| json!({
                    "id": wp.id, "title": wp.title, "role": wp.role, "status": wp.status,
                    "wave": board.wave_of(&wp.id).map(|w| w.id.clone()), "depends_on": wp.depends_on,
                }))
                .collect(),
            diagnostics: diags,
            ..Default::default()
        },
        WpCmd::Validate => {
            diags.extend(board.validate(root));
            Outcome {
                results: vec![json!({"packages": board.packages.len(), "waves": board.waves.len()})],
                diagnostics: diags,
                ..Default::default()
            }
        }
        WpCmd::Ready { done } => {
            let done_set: std::collections::BTreeSet<&str> = board
                .packages
                .values()
                .filter(|w| w.status == SpecStatus::Done)
                .map(|w| w.id.as_str())
                .chain(done.iter().map(String::as_str))
                .collect();
            let scheduled = board.scheduled();
            let mut ready: Vec<_> = board
                .packages
                .values()
                .filter(|w| scheduled.contains(&w.id) && !done_set.contains(w.id.as_str()))
                .filter(|w| w.depends_on.iter().all(|d| done_set.contains(d.as_str())))
                .map(|w| (board.dependents(&w.id), w))
                .collect();
            ready.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.id.cmp(&b.1.id)));
            Outcome {
                results: ready
                    .into_iter()
                    .map(|(n, w)| json!({"id": w.id, "title": w.title, "role": w.role, "unblocks": n}))
                    .collect(),
                diagnostics: diags,
                ..Default::default()
            }
        }
        WpCmd::Show { id } => {
            let Some(wp) = board.packages.get(id) else {
                return fail(Diagnostic::error("SYRP-WP-0002", format!("unknown work package {id}")), ExitCode::Usage);
            };
            let spec = fs::read_to_string(root.join(&wp.spec)).ok();
            let agents_md = wp
                .crate_path
                .as_ref()
                .and_then(|c| fs::read_to_string(root.join(c).join("AGENTS.md")).ok());
            let deps: Vec<Value> = wp
                .depends_on
                .iter()
                .map(|d| json!({"id": d, "spec_status": board.packages.get(d).map(|w| w.status)}))
                .collect();
            Outcome::result(json!({
                "package": wp,
                "wave": board.wave_of(id),
                "owned_paths": wp.owned_paths(),
                "dependencies": deps,
                "spec": spec,
                "crate_agents_md": agents_md,
                "branch": format!("wp/{id}-<slug>"),
                "next": [
                    "write failing tests from the spec first",
                    "loop: cargo syrp check --json; cargo syrp test --crate <pkg> --json",
                    format!("finish: {}", wp.acceptance),
                ],
            }))
        }
        WpCmd::CheckPaths { id, files, base } => {
            let Some(wp) = board.packages.get(id) else {
                return fail(Diagnostic::error("SYRP-WP-0002", format!("unknown work package {id}")), ExitCode::Usage);
            };
            let files = if files.is_empty() {
                match changed_files(root, base) {
                    Ok(f) => f,
                    Err(d) => return fail(d, ExitCode::Environment),
                }
            } else {
                files.clone()
            };
            match Ownership::load(root) {
                Ok(own) => Outcome {
                    results: vec![json!({"wp": id, "files": files})],
                    diagnostics: own.check_paths(wp, &files),
                    ..Default::default()
                },
                Err(d) => fail(d, ExitCode::Failed),
            }
        }
        WpCmd::Check { id, base } => {
            let Some(wp) = board.packages.get(id) else {
                return fail(Diagnostic::error("SYRP-WP-0002", format!("unknown work package {id}")), ExitCode::Usage);
            };
            let mut out = checks::check(root, false);
            let mut steps = vec![json!({"step": "check", "ok": out.exit_code() == ExitCode::Ok})];
            // Tests for the WP's own crate (or the whole workspace if it owns none).
            let pkg = wp.crate_path.as_ref().and_then(|c| {
                checks::crates(root).into_iter().find(|k| k.dir == c.trim_end_matches('/')).map(|k| k.name)
            });
            if wp.crate_path.is_some() && pkg.is_none() {
                out.diagnostics.push(Diagnostic::error(
                    "SYRP-WP-0008",
                    format!("{id} owns {} but no crate exists there yet", wp.crate_path.as_deref().unwrap_or("")),
                ));
            }
            let t = checks::test(root, pkg.as_deref());
            steps.push(json!({"step": "test", "ok": t.exit_code() == ExitCode::Ok, "summary": t.results.first()}));
            let t_exit = t.exit_code();
            out.diagnostics.extend(t.diagnostics);
            match changed_files(root, base) {
                Ok(files) => {
                    if let Ok(own) = Ownership::load(root) {
                        let d = own.check_paths(wp, &files);
                        steps.push(json!({"step": "ownership", "ok": d.is_empty(), "files": files.len()}));
                        out.diagnostics.extend(d);
                    }
                }
                Err(d) => out.diagnostics.push(Diagnostic { severity: crate::envelope::Severity::Warning, ..d }),
            }
            let mut exit = out.exit.unwrap_or(ExitCode::Ok).worst(t_exit);
            if out.diagnostics.iter().any(|d| d.severity == crate::envelope::Severity::Error) {
                exit = exit.worst(ExitCode::Failed);
            }
            out.exit = Some(exit);
            out.results = vec![json!({"wp": id, "steps": steps})];
            out
        }
    }
}

fn conductor_cmd(root: &Path, c: &ConductorCmd) -> Outcome {
    match c {
        ConductorCmd::Tick(a) => {
            let issues: Vec<conductor::Issue> = match read_json(&a.issues) {
                Ok(v) => v,
                Err(o) => return o,
            };
            let prs: Vec<conductor::PullRequest> = match read_json(&a.prs) {
                Ok(v) => v,
                Err(o) => return o,
            };
            let (board, mut diags) = Board::load(root);
            diags.extend(board.validate(root));
            if !diags.is_empty() {
                // Never schedule from a broken board.
                return Outcome {
                    diagnostics: diags,
                    exit: Some(ExitCode::Failed),
                    ..Default::default()
                };
            }
            let now = a.now.unwrap_or_else(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0)
            });
            let cfg = conductor::Config {
                slots: a.slots,
                lease_ttl: a.lease_ttl,
                now,
            };
            Outcome {
                results: conductor::tick(&board, &issues, &prs, cfg)
                    .into_iter()
                    .map(|a| serde_json::to_value(a).expect("action"))
                    .collect(),
                ..Default::default()
            }
        }
        ConductorCmd::Finish { issue, has_pr } => {
            let i: conductor::Issue = match read_json(issue) {
                Ok(v) => v,
                Err(o) => return o,
            };
            let (body, status) = conductor::finish(&i, *has_pr);
            Outcome::result(json!({"issue": i.number, "body": body, "status": status}))
        }
    }
}

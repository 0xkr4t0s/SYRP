//! `syrp-agent-eval`: run the eval task set and print the JSON report.
//!
//! Exit codes follow the CLI: 0 ok, 1 the success rate is below `--min-success-rate`,
//! 2 usage, 3 environment (the harness itself failed).

use std::io::Write;
use std::path::PathBuf;

use clap::Parser;
use syrp_agent_eval::{Agent, EvalError, RunConfig, load_tasks, run};
use syrp_error::Diagnostic;

/// Agent-eval harness: score agents on the fixed Syrp task set.
#[derive(Debug, Parser)]
#[command(name = "syrp-agent-eval", version)]
enum Cli {
    /// Run tasks and print the report (schema syrp.agent-eval/v1).
    Run(RunArgs),
    /// Run the Syrp CLI in-process, like `cargo syrp <args>`, with JSON output.
    Syrp {
        /// Arguments for the CLI.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

#[derive(Debug, clap::Args)]
struct RunArgs {
    /// Repository under test (`SYRP_ROOT`).
    #[arg(long, default_value = ".")]
    root: PathBuf,
    /// Task directory (default: <root>/evals/tasks).
    #[arg(long)]
    tasks: Option<PathBuf>,
    /// Workspaces, prompts and logs go here (default: <root>/target/agent-eval).
    #[arg(long)]
    work_dir: Option<PathBuf>,
    /// `none` (baseline), `reference` (self-test), or a command template using
    /// {prompt_file}, {workspace}, {root} and {task}.
    #[arg(long, default_value = "none")]
    agent: String,
    /// Only these task ids.
    #[arg(long, value_delimiter = ',')]
    only: Vec<String>,
    /// Also write the report to this file.
    #[arg(long)]
    out: Option<PathBuf>,
    /// Exit 1 when the success rate is below this.
    #[arg(long, default_value_t = 0.0)]
    min_success_rate: f64,
    /// Shell words for the `syrp` function (default: this binary's `syrp` passthrough).
    #[arg(long)]
    syrp: Option<String>,
}

fn main() {
    let code = match Cli::parse() {
        Cli::Syrp { args } => syrp(args),
        Cli::Run(args) => match run_all(&args) {
            Ok(rate) if rate >= args.min_success_rate => 0,
            Ok(_) => 1,
            Err(e) => {
                eprintln!("{}: {e} (hint: {})", e.code(), e.hint().unwrap_or_default());
                if matches!(e, EvalError::Config { .. }) {
                    2
                } else {
                    3
                }
            }
        },
    };
    std::process::exit(code);
}

fn run_all(args: &RunArgs) -> Result<f64, EvalError> {
    let agent = match args.agent.as_str() {
        "none" => Agent::None,
        "reference" => Agent::Reference,
        template => Agent::Command(template.to_string()),
    };
    let syrp = args.syrp.clone().unwrap_or_else(|| {
        let exe = std::env::current_exe().unwrap_or_default();
        format!("\"{}\" syrp", exe.display().to_string().replace('\\', "/"))
    });
    let root = &args.root;
    let work_dir = args
        .work_dir
        .clone()
        .unwrap_or(root.join("target/agent-eval"));
    let cfg = RunConfig::new(root, work_dir).agent(agent).syrp(syrp);
    let dir = args.tasks.clone().unwrap_or(root.join("evals/tasks"));
    let mut tasks = load_tasks(&dir)?;
    if let Some(id) = args
        .only
        .iter()
        .find(|id| !tasks.iter().any(|t| t.id() == *id))
    {
        return Err(EvalError::Config {
            detail: format!("no task `{id}` in {}", dir.display()),
        });
    }
    tasks.retain(|t| args.only.is_empty() || args.only.iter().any(|id| id == t.id()));
    let report = run(&tasks, &cfg)?;
    let text = serde_json::to_string_pretty(&report.to_json()).unwrap_or_default();
    if let Some(path) = &args.out {
        std::fs::write(path, &text).map_err(|e| EvalError::Io {
            path: path.clone(),
            detail: e.to_string(),
        })?;
    }
    let _ = writeln!(std::io::stdout().lock(), "{text}");
    Ok(report.success_rate())
}

fn syrp(args: Vec<String>) -> i32 {
    let argv = std::iter::once("cargo-syrp".to_string()).chain(args);
    let cli = syrp_cli::cli::Cli::try_parse_from(argv).unwrap_or_else(|e| e.exit());
    let outcome = syrp_cli::cli::dispatch(&cli);
    let env = syrp_cli::envelope::Envelope::new(&cli.command.name(), outcome, 0);
    let text = serde_json::to_string_pretty(&env).unwrap_or_default();
    let _ = writeln!(std::io::stdout().lock(), "{text}");
    env.exit_code
}

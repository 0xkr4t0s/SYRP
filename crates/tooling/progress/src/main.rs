// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! `syrp-progress`: writes `progress.json` and `index.html` for the progress page (spec B5).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use syrp_progress::{Diagnostic, OpenPr, ProgressError};

/// Builds the Syrp progress page from the board, the parity inventory and git history.
#[derive(Parser)]
#[command(name = "syrp-progress", version)]
struct Args {
    /// Repository root.
    #[arg(long, default_value = ".")]
    root: PathBuf,
    /// Output directory; created if missing.
    #[arg(long)]
    out: PathBuf,
    /// Optional JSON list of open PRs, as `gh pr list --json number,title` prints it.
    #[arg(long)]
    open_prs: Option<PathBuf>,
}

fn read_open(path: &Path) -> Result<Vec<OpenPr>, ProgressError> {
    let err = |detail: String| ProgressError::OpenPrs {
        path: path.to_path_buf(),
        detail,
    };
    let text = std::fs::read_to_string(path).map_err(|e| err(e.to_string()))?;
    serde_json::from_str(&text).map_err(|e| err(e.to_string()))
}

fn write(path: PathBuf, text: &str) -> Result<(), ProgressError> {
    std::fs::write(&path, text).map_err(|e| ProgressError::Write {
        path,
        detail: e.to_string(),
    })
}

fn run(args: &Args) -> Result<(), ProgressError> {
    let open = match &args.open_prs {
        Some(p) => read_open(p)?,
        None => Vec::new(),
    };
    let progress = syrp_progress::load(&args.root, &open)?;
    std::fs::create_dir_all(&args.out).map_err(|e| ProgressError::Write {
        path: args.out.clone(),
        detail: e.to_string(),
    })?;
    let json = serde_json::to_string_pretty(&progress).expect("snapshot always serialises");
    write(args.out.join("progress.json"), &(json + "\n"))?;
    write(
        args.out.join("index.html"),
        &syrp_progress::render_html(&progress),
    )
}

fn main() -> ExitCode {
    let args = Args::parse();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            if let Some(h) = e.hint() {
                eprintln!("hint: {h}");
            }
            ExitCode::from(3)
        }
    }
}

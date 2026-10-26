//! `cargo auricle`: see the library docs.

use std::io::{IsTerminal, Write};
use std::time::Instant;

use auricle_cli::cli::{Cli, dispatch};
use auricle_cli::envelope::{Envelope, ExitCode};
use clap::Parser;

fn main() {
    // Cargo runs subcommands as `cargo-auricle auricle <args>`; drop the repeated name.
    let mut args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("auricle") {
        args.remove(1);
    }
    let json_requested = args.iter().any(|a| a == "--json") || !std::io::stdout().is_terminal();
    let cli = match Cli::try_parse_from(&args) {
        Ok(cli) => cli,
        Err(e)
            if !json_requested
                || matches!(
                    e.kind(),
                    clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
                ) =>
        {
            e.exit()
        }
        Err(e) => {
            let outcome = auricle_cli::envelope::Outcome {
                diagnostics: vec![auricle_cli::envelope::Diagnostic::error(
                    "AUR-CLI-0005",
                    e.to_string(),
                )],
                exit: Some(ExitCode::Usage),
                ..Default::default()
            };
            emit(&Envelope::new("usage", outcome, 0), true);
            std::process::exit(ExitCode::Usage.code());
        }
    };
    let start = Instant::now();
    let outcome = dispatch(&cli);
    let env = Envelope::new(&cli.command.name(), outcome, start.elapsed().as_millis());
    emit(&env, json_requested);
    std::process::exit(env.exit_code);
}

fn emit(env: &Envelope, json: bool) {
    let mut out = std::io::stdout().lock();
    let text = if json {
        serde_json::to_string_pretty(env).expect("envelope serialises")
    } else {
        env.to_text()
    };
    let _ = writeln!(out, "{}", text.trim_end());
}

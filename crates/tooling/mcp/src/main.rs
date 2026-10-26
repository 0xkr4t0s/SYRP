//! `syrp-mcp`: serves the Syrp MCP server over stdio. See the library docs.
//!
//! Usage: `syrp-mcp [--root <path>]`. Without `--root` the repository is found by searching
//! upwards from the working directory, as `cargo syrp` does.

use std::path::PathBuf;
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let mut root: Option<PathBuf> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root" => match args.next() {
                Some(p) => root = Some(p.into()),
                None => return usage("--root needs a path"),
            },
            "--stdio" => {}
            "-h" | "--help" => {
                println!(
                    "syrp-mcp [--root <path>] [--stdio]\nServes the Syrp MCP server over stdio."
                );
                return ExitCode::SUCCESS;
            }
            "-V" | "--version" => {
                println!("syrp-mcp {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            other => return usage(&format!("unknown argument {other}")),
        }
    }
    // stdout carries the protocol; everything human goes to stderr.
    match syrp_mcp::serve_stdio(syrp_mcp::SyrpServer::new(root)).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("syrp-mcp: {e}");
            ExitCode::from(10)
        }
    }
}

fn usage(msg: &str) -> ExitCode {
    eprintln!("syrp-mcp: {msg}\nusage: syrp-mcp [--root <path>] [--stdio]");
    ExitCode::from(2)
}

// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! `syrp-provenance`: a thin command line for CI until `cargo syrp` wires this crate in.
//!
//! ```text
//! syrp-provenance check-range <range> [--root <dir>]
//! ```
//!
//! Prints a JSON report on stdout. Exit codes follow `cargo syrp`: 0 ok, 1 failed (git or
//! file error), 2 usage, 9 clean-room check failed.

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: syrp-provenance check-range <range> [--root <dir>]";

fn run(args: &[String]) -> Result<(String, bool), (String, u8)> {
    let usage = |m: &str| (format!("{m}\n{USAGE}"), 2);
    let mut root = PathBuf::from(".");
    let mut positional = Vec::new();
    let mut it = args.iter().skip(1);
    while let Some(a) = it.next() {
        if a == "--root" {
            root = PathBuf::from(it.next().ok_or_else(|| usage("--root needs a value"))?);
        } else {
            positional.push(a.as_str());
        }
    }
    match positional.as_slice() {
        ["check-range", range] => {
            let r = syrp_provenance::provenance::check_range(&root, range)
                .map_err(|e| (e.to_string(), 1))?;
            let json = serde_json::to_string_pretty(&r).expect("report serialises");
            Ok((json, r.passed()))
        }
        _ => Err(usage("unknown command")),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match run(&args) {
        Ok((json, passed)) => {
            println!("{json}");
            ExitCode::from(if passed { 0 } else { 9 })
        }
        Err((msg, code)) => {
            eprintln!("{msg}");
            ExitCode::from(code)
        }
    }
}

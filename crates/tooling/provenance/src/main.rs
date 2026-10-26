// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! `syrp-provenance`: a thin command line for CI until `cargo syrp` wires this crate in.
//!
//! ```text
//! syrp-provenance check-range <range> [--root <dir>]
//! syrp-provenance scan --db <fingerprints.json> --threshold <0..1> <file>...
//! ```
//!
//! Prints JSON on stdout. Exit codes follow `cargo syrp`: 0 ok, 1 failed (bad input or git),
//! 2 usage, 9 clean-room check failed.

use std::path::PathBuf;
use std::process::ExitCode;

use syrp_provenance::{database::Database, provenance, report};

const USAGE: &str = "usage: syrp-provenance check-range <range> [--root <dir>]\n       \
                     syrp-provenance scan --db <file> --threshold <0..1> <file>...";

fn run(args: &[String]) -> Result<(String, bool), (String, u8)> {
    let usage = |m: &str| (format!("{m}\n{USAGE}"), 2);
    let mut root = PathBuf::from(".");
    let mut db = None;
    let mut threshold = None;
    let mut positional = Vec::new();
    let mut it = args.iter().skip(1);
    while let Some(a) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| usage(&format!("{a} needs a value")))
        };
        match a.as_str() {
            "--root" => root = PathBuf::from(value()?),
            "--db" => db = Some(PathBuf::from(value()?)),
            "--threshold" => {
                let v: f64 = value()?.parse().map_err(|_| usage("bad --threshold"))?;
                if !(0.0..=1.0).contains(&v) {
                    return Err(usage("--threshold must be between 0 and 1"));
                }
                threshold = Some(v);
            }
            _ => positional.push(a.clone()),
        }
    }
    let fail = |e: syrp_provenance::Error| (e.to_string(), 1);
    match positional.split_first() {
        Some((cmd, [range])) if cmd == "check-range" => {
            let r = provenance::check_range(&root, range).map_err(fail)?;
            Ok((
                serde_json::to_string_pretty(&r).expect("serialises"),
                r.passed(),
            ))
        }
        Some((cmd, files)) if cmd == "scan" && !files.is_empty() => {
            let db = Database::load(&db.ok_or_else(|| usage("scan needs --db"))?).map_err(fail)?;
            let threshold = threshold.ok_or_else(|| usage("scan needs --threshold"))?;
            let mut inputs = Vec::new();
            for f in files {
                let text = std::fs::read_to_string(f).map_err(|e| {
                    fail(syrp_provenance::Error::Io(format!("cannot read {f}: {e}")))
                })?;
                inputs.push((f.clone(), text));
            }
            let r = report::scan(&db, &inputs, threshold);
            Ok((r.to_json(), r.passed()))
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

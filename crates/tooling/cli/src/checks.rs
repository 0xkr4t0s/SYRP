//! `check`, `test` and `doctor`: the fast local gates.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};

use crate::envelope::{Diagnostic, ExitCode, Outcome, Severity};
use crate::repo;
use crate::wp::Board;

/// Source files longer than this fail the size lint (master plan §12.5, "files under the size limit").
pub const MAX_FILE_LINES: usize = 1200;
/// Crate-name prefixes banned for clean-room reasons.
const BANNED_PREFIXES: &[&str] = &["aloe-"];
const LAYERS: &[&str] = &["L0", "L1", "L2", "L3", "L4", "L5", "L6", "L7"];

/// A workspace crate as the lints see it.
#[derive(Debug, Clone)]
pub struct CrateInfo {
    /// Package name.
    pub name: String,
    /// Directory relative to the root.
    pub dir: String,
    /// `L0`..`L7`, from `[package.metadata.syrp] layer`.
    pub layer: Option<String>,
    /// Optional sibling group; crates in the same group may not depend on each other.
    pub group: Option<String>,
    /// Normal and build dependencies (dev-dependencies are exempt from layering).
    pub deps: Vec<String>,
}

/// Reads every `crates/*/*/Cargo.toml`.
pub fn crates(root: &Path) -> Vec<CrateInfo> {
    let mut out = Vec::new();
    let Ok(groups) = fs::read_dir(root.join("crates")) else {
        return out;
    };
    let mut dirs: Vec<_> = groups
        .flatten()
        .filter(|g| g.path().is_dir())
        .flat_map(|g| fs::read_dir(g.path()).into_iter().flatten().flatten())
        .map(|e| e.path())
        .filter(|p| p.join("Cargo.toml").is_file())
        .collect();
    dirs.sort();
    for dir in dirs {
        let Ok(src) = fs::read_to_string(dir.join("Cargo.toml")) else {
            continue;
        };
        let Ok(doc) = toml::from_str::<toml::Value>(&src) else {
            continue;
        };
        let meta = doc
            .get("package")
            .and_then(|p| p.get("metadata"))
            .and_then(|m| m.get("syrp"));
        let get = |k: &str| {
            meta.and_then(|m| m.get(k))
                .and_then(|v| v.as_str())
                .map(str::to_string)
        };
        let mut deps = Vec::new();
        for table in ["dependencies", "build-dependencies"] {
            if let Some(t) = doc.get(table).and_then(|t| t.as_table()) {
                for (k, v) in t {
                    // Respect `package = "..."` renames.
                    let real = v.get("package").and_then(|p| p.as_str()).unwrap_or(k);
                    deps.push(real.to_string());
                }
            }
        }
        out.push(CrateInfo {
            name: doc
                .get("package")
                .and_then(|p| p.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or_default()
                .to_string(),
            dir: repo::rel(root, &dir),
            layer: get("layer"),
            group: get("group"),
            deps,
        });
    }
    out
}

/// Layering, sibling, metadata and AGENTS.md lints over the workspace crates.
pub fn lint_crates(root: &Path, crates: &[CrateInfo]) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let by_name: BTreeMap<&str, &CrateInfo> = crates.iter().map(|c| (c.name.as_str(), c)).collect();
    let rank = |l: &Option<String>| {
        l.as_deref()
            .and_then(|l| LAYERS.iter().position(|x| *x == l))
    };
    for c in crates {
        let manifest = format!("{}/Cargo.toml", c.dir);
        if rank(&c.layer).is_none() {
            diags.push(
                Diagnostic::error(
                    "SYRP-CLI-0206",
                    format!("{} has no valid [package.metadata.syrp] layer", c.name),
                )
                .at(&manifest)
                .hint("add `layer = \"L0\"`..`\"L7\"` per the master plan §5.2"),
            );
        }
        if !root.join(&c.dir).join("AGENTS.md").is_file() {
            diags.push(
                Diagnostic::error("SYRP-CLI-0206", format!("{} has no AGENTS.md", c.name))
                    .at(format!("{}/AGENTS.md", c.dir))
                    .hint("copy the headings from docs/agents/crate-agents-template.md"),
            );
        }
        for dep in &c.deps {
            let Some(d) = by_name.get(dep.as_str()) else {
                continue;
            };
            if let (Some(mine), Some(theirs)) = (rank(&c.layer), rank(&d.layer))
                && theirs > mine
            {
                diags.push(
                    Diagnostic::error(
                        "SYRP-CLI-0203",
                        format!(
                            "{} ({}) depends on higher-layer {} ({})",
                            c.name,
                            c.layer.as_deref().unwrap_or("?"),
                            d.name,
                            d.layer.as_deref().unwrap_or("?")
                        ),
                    )
                    .at(&manifest),
                );
            }
            if c.group.is_some() && c.group == d.group {
                diags.push(
                    Diagnostic::error(
                        "SYRP-CLI-0203",
                        format!(
                            "{} and {} are siblings in group {:?}",
                            c.name,
                            d.name,
                            c.group.as_deref().unwrap_or("")
                        ),
                    )
                    .at(&manifest),
                );
            }
        }
    }
    diags
}

/// Fails any `.rs` file over [`MAX_FILE_LINES`].
pub fn lint_sizes(root: &Path) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for dir in ["crates", "apps"] {
        for f in repo::files_with_suffix(&root.join(dir), ".rs") {
            let n = fs::read_to_string(&f)
                .map(|s| s.lines().count())
                .unwrap_or(0);
            if n > MAX_FILE_LINES {
                diags.push(
                    Diagnostic::error(
                        "SYRP-CLI-0205",
                        format!("{n} lines (limit {MAX_FILE_LINES})"),
                    )
                    .at(repo::rel(root, &f))
                    .hint("split the module"),
                );
            }
        }
    }
    diags
}

/// Rejects banned crates anywhere in `Cargo.lock`.
pub fn lint_banned(root: &Path) -> Vec<Diagnostic> {
    let Ok(src) = fs::read_to_string(root.join("Cargo.lock")) else {
        return vec![];
    };
    src.lines()
        .filter_map(|l| l.strip_prefix("name = \""))
        .map(|n| n.trim_end_matches('"'))
        .filter(|n| BANNED_PREFIXES.iter().any(|p| n.starts_with(p)))
        .map(|n| {
            Diagnostic::error(
                "SYRP-CLI-0204",
                format!("banned dependency {n} (clean room)"),
            )
            .at("Cargo.lock")
        })
        .collect()
}

struct Run {
    ok: bool,
    output: String,
}

fn run(root: &Path, program: &str, args: &[&str]) -> Result<Run, Diagnostic> {
    let out = Command::new(program)
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|e| {
            Diagnostic::error("SYRP-CLI-0101", format!("could not run {program}: {e}"))
                .hint("run `cargo syrp doctor`")
        })?;
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    Ok(Run {
        ok: out.status.success(),
        output: text,
    })
}

fn tail(s: &str, n: usize) -> String {
    let lines: Vec<&str> = s.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

/// `cargo syrp check`: fmt, clippy, layering, size, metadata, banned crates and WP specs.
pub fn check(root: &Path, lints_only: bool) -> Outcome {
    let mut out = Outcome::default();
    let mut steps: Vec<Value> = Vec::new();
    let mut step = |name: &str, diags: Vec<Diagnostic>, out: &mut Outcome| {
        let failed = diags.iter().any(|d| d.severity == Severity::Error);
        steps.push(json!({"step": name, "ok": !failed, "findings": diags.len()}));
        out.diagnostics.extend(diags);
    };

    let crates = crates(root);
    step("lint-crates", lint_crates(root, &crates), &mut out);
    step("lint-size", lint_sizes(root), &mut out);
    let banned = lint_banned(root);
    if !banned.is_empty() {
        out.exit = Some(ExitCode::CleanRoom);
    }
    step("lint-banned", banned, &mut out);
    // A committed review.json could be published as the reviewer's verdict.
    let planted: Vec<Diagnostic> = [
        "review.json",
        "review-primary.json",
        "review-adversarial.json",
    ]
    .iter()
    .filter(|f| root.join(f).exists())
    .map(|f| {
        Diagnostic::error(
            "SYRP-CLI-0207",
            format!("{f} must not exist in the repository"),
        )
        .at(*f)
        .hint("delete it; reviewers write their review outside the checkout")
    })
    .collect();
    step("lint-review-files", planted, &mut out);
    let (board, mut wp_diags) = Board::load(root);
    wp_diags.extend(board.validate(root));
    step("wp-validate", wp_diags, &mut out);

    if !lints_only {
        for (name, program, args, code) in [
            (
                "fmt",
                "cargo",
                &["fmt", "--all", "--", "--check"][..],
                "SYRP-CLI-0201",
            ),
            (
                "clippy",
                "cargo",
                &[
                    "clippy",
                    "--workspace",
                    "--all-targets",
                    "--quiet",
                    "--",
                    "-D",
                    "warnings",
                ][..],
                "SYRP-CLI-0202",
            ),
        ] {
            match run(root, program, args) {
                Ok(r) if r.ok => step(name, vec![], &mut out),
                Ok(r) => step(
                    name,
                    vec![Diagnostic::error(
                        code,
                        format!("{name} failed:\n{}", tail(&r.output, 40)),
                    )],
                    &mut out,
                ),
                Err(d) => {
                    out.exit = Some(ExitCode::Environment);
                    step(name, vec![d], &mut out);
                }
            }
        }
    }
    let failed = out
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error);
    if failed && out.exit.is_none() {
        out.exit = Some(ExitCode::Failed);
    }
    out.results
        .push(json!({"crates": crates.len(), "steps": steps}));
    out
}

/// Parses `test result:` lines from libtest output into (passed, failed, ignored).
pub fn parse_libtest(output: &str) -> (u64, u64, u64) {
    let mut totals = (0, 0, 0);
    for line in output.lines().filter(|l| l.starts_with("test result:")) {
        let num = |key: &str| {
            line.split(';')
                .find(|p| p.trim_end().ends_with(key))
                .and_then(|p| p.split_whitespace().rev().nth(1))
                .and_then(|n| n.parse::<u64>().ok())
                .unwrap_or(0)
        };
        totals.0 += num("passed");
        totals.1 += num("failed");
        totals.2 += num("ignored");
    }
    totals
}

/// `cargo syrp test`: cargo test (nextest when installed) with a structured summary.
pub fn test(root: &Path, krate: Option<&str>) -> Outcome {
    let mut args: Vec<&str> = vec!["test", "--quiet"];
    match krate {
        Some(k) => args.extend(["--package", k]),
        None => args.push("--workspace"),
    }
    match run(root, "cargo", &args) {
        Err(d) => Outcome {
            diagnostics: vec![d],
            exit: Some(ExitCode::Environment),
            ..Default::default()
        },
        Ok(r) => {
            let (passed, failed, ignored) = parse_libtest(&r.output);
            let mut out = Outcome::result(json!({
                "runner": "cargo test",
                "passed": passed, "failed": failed, "ignored": ignored,
            }));
            if !r.ok {
                out.diagnostics.push(Diagnostic::error(
                    "SYRP-CLI-0301",
                    format!("tests failed:\n{}", tail(&r.output, 60)),
                ));
                out.exit = Some(ExitCode::Failed);
            }
            out
        }
    }
}

/// `cargo syrp doctor`: toolchain and tool availability.
pub fn doctor(root: Option<&Path>) -> Outcome {
    let mut out = Outcome::default();
    let mut tools = Vec::new();
    // (program, version args, required, purpose)
    let probes: &[(&str, &[&str], bool, &str)] = &[
        ("cargo", &["--version"], true, "build"),
        ("rustc", &["--version"], true, "build"),
        ("git", &["--version"], true, "version control"),
        ("cargo", &["fmt", "--version"], true, "check: fmt"),
        ("cargo", &["clippy", "--version"], true, "check: clippy"),
        ("gh", &["--version"], false, "pipeline: GitHub automation"),
        ("mdbook", &["--version"], false, "docs: human site"),
        (
            "cargo",
            &["nextest", "--version"],
            false,
            "test: faster runner",
        ),
        (
            "cargo",
            &["deny", "--version"],
            false,
            "gate G5: licences and advisories",
        ),
    ];
    for (program, args, required, purpose) in probes {
        let label = if *program == "cargo" && args[0] != "--version" {
            format!("cargo-{}", args[0])
        } else {
            program.to_string()
        };
        let found = Command::new(program)
            .args(*args)
            .output()
            .ok()
            .filter(|o| o.status.success());
        let version = found.as_ref().map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string()
        });
        if version.is_none() {
            let d = if *required {
                out.exit = Some(ExitCode::Environment);
                Diagnostic::error("SYRP-CLI-0101", format!("{label} is missing ({purpose})"))
            } else {
                Diagnostic::warning("SYRP-CLI-0101", format!("{label} is missing ({purpose})"))
            };
            out.diagnostics.push(d.hint(format!("install {label}")));
        }
        tools.push(
            json!({"tool": label, "required": required, "purpose": purpose, "version": version}),
        );
    }
    let mut repo_info = Value::Null;
    if let Some(root) = root {
        let pinned = fs::read_to_string(root.join("rust-toolchain.toml"))
            .ok()
            .and_then(|s| toml::from_str::<toml::Value>(&s).ok())
            .and_then(|v| {
                v.get("toolchain")?
                    .get("channel")?
                    .as_str()
                    .map(str::to_string)
            });
        let rustc = tools
            .iter()
            .find(|t| t["tool"] == "rustc")
            .and_then(|t| t["version"].as_str().map(str::to_string));
        if let (Some(p), Some(r)) = (&pinned, &rustc)
            && !r.contains(p.as_str())
        {
            out.diagnostics.push(
                    Diagnostic::warning("SYRP-CLI-0102", format!("rustc is `{r}` but rust-toolchain.toml pins {p}"))
                        .hint("rustup will switch automatically inside the repo; outside it, run `rustup toolchain install`"),
                );
        }
        let (board, diags) = Board::load(root);
        let invalid = diags.len() + board.validate(root).len();
        repo_info = json!({"root": root.display().to_string(), "pinned_toolchain": pinned,
            "work_packages": board.packages.len(), "waves": board.waves.len(), "invalid_specs": invalid});
    }
    out.results.push(json!({"tools": tools, "repo": repo_info}));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_libtest_summaries() {
        let out = "test result: ok. 12 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out\n\
                   test result: FAILED. 3 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out";
        assert_eq!(parse_libtest(out), (15, 2, 1));
    }

    #[test]
    fn layering_rejects_upward_and_sibling_deps() {
        let c = |name: &str, layer: &str, group: Option<&str>, deps: &[&str]| CrateInfo {
            name: name.into(),
            dir: format!("crates/x/{name}"),
            layer: Some(layer.into()),
            group: group.map(str::to_string),
            deps: deps.iter().map(|d| d.to_string()).collect(),
        };
        let crates = vec![
            c("low", "L0", None, &["high"]),
            c("high", "L3", None, &["low"]),
            c("wav", "L3", Some("format-leaf"), &["flac"]),
            c("flac", "L3", Some("format-leaf"), &[]),
        ];
        let diags: Vec<_> = lint_crates(Path::new("/nonexistent"), &crates)
            .into_iter()
            .filter(|d| d.code == "SYRP-CLI-0203")
            .collect();
        assert_eq!(diags.len(), 2, "{diags:#?}");
    }
}

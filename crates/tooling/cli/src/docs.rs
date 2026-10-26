//! `docs build` and `docs search`: the two documentation sides from one set of sources
//! (master plan §13.1).
//!
//! Human side: mdBook over `docs/`. Agent side: `llms.txt`, `llms-full.txt`, Markdown
//! mirrors, per-crate briefs and JSON indexes, all generated into `<out>/agents/`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

use crate::catalogue;
use crate::checks;
use crate::envelope::{Diagnostic, ExitCode, Outcome};
use crate::repo;
use crate::wp::Board;

/// Markdown sources that make up the docs, in reading order: root agent guide, docs tree,
/// per-crate AGENTS.md.
fn sources(root: &Path) -> Vec<PathBuf> {
    let mut v = vec![root.join("README.md"), root.join("AGENTS.md")];
    for section in ["guide", "agents", "cookbook", "migration", "design"] {
        v.extend(repo::files_with_suffix(
            &root.join("docs").join(section),
            ".md",
        ));
    }
    v.extend(repo::files_with_suffix(&root.join("crates"), "AGENTS.md"));
    v.retain(|p| p.is_file());
    v
}

fn title_of(src: &str, fallback: &str) -> String {
    src.lines()
        .find_map(|l| l.strip_prefix("# "))
        .map(str::trim)
        .unwrap_or(fallback)
        .to_string()
}

fn write(path: &Path, contents: &str, artifacts: &mut Vec<String>) -> Result<(), Diagnostic> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)
            .map_err(|e| Diagnostic::error("AUR-CLI-0401", format!("{}: {e}", dir.display())))?;
    }
    fs::write(path, contents)
        .map_err(|e| Diagnostic::error("AUR-CLI-0401", format!("{}: {e}", path.display())))?;
    artifacts.push(path.display().to_string());
    Ok(())
}

/// Builds both sides into `out`.
pub fn build(root: &Path, out: &Path, skip_book: bool) -> Outcome {
    let mut o = Outcome::default();
    // The book goes first: mdbook clears its output directory.
    if skip_book {
        o.results.push(json!({"human_side": "skipped"}));
    } else {
        match Command::new("mdbook")
            .arg("build")
            .arg("--dest-dir")
            .arg(out)
            .arg(root)
            .output()
        {
            Ok(r) if r.status.success() => {
                o.results
                    .push(json!({"human_side": out.display().to_string()}));
            }
            Ok(r) => {
                o.diagnostics.push(Diagnostic::error(
                    "AUR-CLI-0402",
                    format!("mdbook failed: {}", String::from_utf8_lossy(&r.stderr)),
                ));
                o.exit = Some(ExitCode::Failed);
            }
            Err(_) => o.diagnostics.push(
                Diagnostic::warning(
                    "AUR-CLI-0101",
                    "mdbook is missing; built the agent side only",
                )
                .hint("cargo install mdbook, or pass --skip-book"),
            ),
        }
    }
    let agents = out.join("agents");
    let mut artifacts = Vec::new();
    let result = (|| -> Result<Value, Diagnostic> {
        let srcs = sources(root);
        let mut index = String::from(
            "# Auricle\n\n> A Rust reimplementation of JUCE, built and used agent-first. Every \
             command emits one JSON envelope with stable exit codes; run `cargo auricle doctor` \
             first and `cargo auricle explain <AUR-code>` on any error.\n\n",
        );
        let mut full = String::new();
        let mut sections: std::collections::BTreeMap<&str, Vec<String>> = Default::default();
        for p in &srcs {
            let rel = repo::rel(root, p);
            let text = fs::read_to_string(p)
                .map_err(|e| Diagnostic::error("AUR-CLI-0401", format!("{rel}: {e}")))?;
            let title = title_of(&text, &rel);
            write(&agents.join("md").join(&rel), &text, &mut artifacts)?;
            full.push_str(&format!("\n\n<!-- source: {rel} -->\n\n{text}"));
            let section = if rel.starts_with("crates/") {
                "Crate briefs"
            } else if rel.starts_with("docs/agents/") || rel == "AGENTS.md" {
                "Agent guide"
            } else if rel.starts_with("docs/design/") {
                "Design"
            } else {
                "Docs"
            };
            sections
                .entry(section)
                .or_default()
                .push(format!("- [{title}](md/{rel})"));
        }
        for (name, entries) in &sections {
            index.push_str(&format!("## {name}\n\n{}\n\n", entries.join("\n")));
        }
        index.push_str(
            "## Machine-readable\n\n- [errors.json](errors.json): error catalogue with fixes\n\
             - [work-packages.json](work-packages.json): the work board\n\
             - [crates.json](crates.json): workspace crates and layers\n\
             - [envelope.schema.json](envelope.schema.json): the CLI result envelope\n",
        );
        write(&agents.join("llms.txt"), &index, &mut artifacts)?;
        write(&agents.join("llms-full.txt"), &full, &mut artifacts)?;

        let errors = serde_json::to_string_pretty(&catalogue::all()).expect("errors serialise");
        write(&agents.join("errors.json"), &errors, &mut artifacts)?;
        let (board, _) = Board::load(root);
        let packages: Vec<_> = board.packages.values().collect();
        write(
            &agents.join("work-packages.json"),
            &serde_json::to_string_pretty(&json!({"packages": packages, "waves": board.waves}))
                .expect("board"),
            &mut artifacts,
        )?;
        let crates: Vec<Value> = checks::crates(root)
            .into_iter()
            .map(|c| json!({"name": c.name, "dir": c.dir, "layer": c.layer}))
            .collect();
        write(
            &agents.join("crates.json"),
            &serde_json::to_string_pretty(&crates).expect("crates"),
            &mut artifacts,
        )?;
        write(
            &agents.join("envelope.schema.json"),
            crate::ENVELOPE_JSON_SCHEMA,
            &mut artifacts,
        )?;
        Ok(json!({"agent_side": agents.display().to_string(), "sources": srcs.len()}))
    })();
    match result {
        Ok(v) => o.results.push(v),
        Err(d) => {
            o.diagnostics.push(d);
            o.exit = Some(ExitCode::Internal);
            return o;
        }
    }

    o.artifacts = artifacts;
    o
}

/// Plain substring search over the docs, error catalogue and WP titles.
pub fn search(root: &Path, query: &str, limit: usize) -> Outcome {
    let q = query.to_lowercase();
    let terms: Vec<&str> = q.split_whitespace().collect();
    let mut hits: Vec<(usize, Value)> = Vec::new();
    for p in sources(root) {
        let Ok(text) = fs::read_to_string(&p) else {
            continue;
        };
        let lower = text.to_lowercase();
        let score: usize = terms.iter().map(|t| lower.matches(t).count()).sum();
        if score == 0 {
            continue;
        }
        let lines: Vec<Value> = text
            .lines()
            .enumerate()
            .filter(|(_, l)| terms.iter().any(|t| l.to_lowercase().contains(t)))
            .take(3)
            .map(|(n, l)| json!({"line": n + 1, "text": l.trim()}))
            .collect();
        hits.push((score, json!({"kind": "doc", "path": repo::rel(root, &p), "title": title_of(&text, ""), "matches": lines})));
    }
    for e in catalogue::all() {
        let hay = format!("{} {} {} {}", e.code, e.title, e.cause, e.fix).to_lowercase();
        let score: usize = terms.iter().map(|t| hay.matches(t).count()).sum();
        if score > 0 {
            hits.push((
                score,
                json!({"kind": "error", "code": e.code, "title": e.title}),
            ));
        }
    }
    let (board, _) = Board::load(root);
    for wp in board.packages.values() {
        let hay = format!("{} {}", wp.id, wp.title).to_lowercase();
        let score: usize = terms.iter().map(|t| hay.matches(t).count()).sum();
        if score > 0 {
            hits.push((
                score,
                json!({"kind": "work-package", "id": wp.id, "title": wp.title}),
            ));
        }
    }
    hits.sort_by_key(|h| std::cmp::Reverse(h.0));
    Outcome {
        results: hits
            .into_iter()
            .take(limit)
            .map(|(s, mut v)| {
                v["score"] = json!(s);
                v
            })
            .collect(),
        ..Default::default()
    }
}

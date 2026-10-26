//! Locating the repository root and walking its files.

use std::fs;
use std::path::{Path, PathBuf};

use crate::envelope::Diagnostic;

/// Finds the Syrp repository root: the nearest ancestor holding both `ownership.toml`
/// and a workspace `Cargo.toml`.
pub fn find_root(start: &Path) -> Result<PathBuf, Diagnostic> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        if d.join("ownership.toml").is_file() && d.join("Cargo.toml").is_file() {
            return Ok(d.to_path_buf());
        }
        dir = d.parent();
    }
    Err(Diagnostic::error(
        "SYRP-CLI-0001",
        format!("no Syrp repository found at or above {}", start.display()),
    )
    .hint("run inside the syrp checkout, or pass --root <path>"))
}

/// Lists files under `dir` (recursively) whose names end with `suffix`, sorted.
/// Skips `target/` and hidden directories.
pub fn files_with_suffix(dir: &Path, suffix: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk(dir, &mut |p| {
        if p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(suffix))
        {
            out.push(p.to_path_buf());
        }
    });
    out.sort();
    out
}

fn walk(dir: &Path, f: &mut dyn FnMut(&Path)) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if name == "target" || name.starts_with('.') {
                continue;
            }
            walk(&path, f);
        } else {
            f(&path);
        }
    }
}

/// `path` relative to `root`, with forward slashes, for stable output across platforms.
pub fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

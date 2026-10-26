// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Provenance checks over a git range (B1).
//!
//! - Every non-merge commit must carry a `WP:` or `Change-Request:` trailer and an
//!   `Agent-Model:` trailer, each with a non-empty value (`SYRP-CR-0003`).
//! - Every Rust source file (`*.rs`) *added* in the range that lives inside a crate must start
//!   with `// SPDX-License-Identifier: <licence>`, where `<licence>` is the crate's
//!   `Cargo.toml` `license` field, following `license.workspace = true` to the workspace root
//!   (`SYRP-CR-0004`). Files that already existed before the range are not checked, so older
//!   files without headers do not fail. Files outside any crate are skipped.
//!
//! ```
//! use syrp_provenance::provenance::check_commit;
//! let msg = "Add thing\n\nWP: WP-0.1-12\nAgent-Model: claude-opus-5-5\n";
//! assert!(check_commit("abc", msg).is_empty());
//! assert_eq!(check_commit("abc", "Add thing").len(), 2);
//! ```

use std::path::Path;
use std::process::Command;

use serde::Serialize;

use crate::{Error, Finding};

/// One commit: its SHA and full message.
///
/// ```
/// let c = syrp_provenance::provenance::Commit { sha: "a1".into(), message: "m".into() };
/// assert_eq!(c.sha, "a1");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    /// Full commit SHA.
    pub sha: String,
    /// Raw commit message, subject included.
    pub message: String,
}

/// Result of [`check_range`].
///
/// ```
/// let r = syrp_provenance::provenance::ProvenanceReport::default();
/// assert!(r.passed());
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ProvenanceReport {
    /// Non-merge commits examined.
    pub commits_checked: usize,
    /// Added source files examined for an SPDX header.
    pub files_checked: usize,
    /// Every failed check.
    pub findings: Vec<Finding>,
}

impl ProvenanceReport {
    /// Whether no check failed.
    ///
    /// ```
    /// assert!(syrp_provenance::provenance::ProvenanceReport::default().passed());
    /// ```
    pub fn passed(&self) -> bool {
        self.findings.is_empty()
    }
}

/// Parses the trailer block: the last paragraph of the message, if it is not the subject,
/// read as `Key: value` lines. Lines that are not trailers are ignored.
///
/// ```
/// use syrp_provenance::provenance::trailers;
/// let t = trailers("Subject\n\nBody text.\n\nWP: WP-1\nAgent-Model: m\n");
/// assert_eq!(t, [("WP".to_string(), "WP-1".to_string()),
///                ("Agent-Model".to_string(), "m".to_string())]);
/// ```
pub fn trailers(message: &str) -> Vec<(String, String)> {
    let normalised = message.replace("\r\n", "\n");
    let paragraphs: Vec<&str> = normalised
        .split("\n\n")
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    if paragraphs.len() < 2 {
        return Vec::new();
    }
    paragraphs[paragraphs.len() - 1]
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            let valid =
                !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
            valid.then(|| (key.to_owned(), value.trim().to_owned()))
        })
        .collect()
}

/// Checks one commit's trailers.
///
/// ```
/// use syrp_provenance::provenance::check_commit;
/// let msg = "Fix\n\nChange-Request: #12\nAgent-Model: m\n";
/// assert!(check_commit("abc", msg).is_empty());
/// ```
pub fn check_commit(sha: &str, message: &str) -> Vec<Finding> {
    let t = trailers(message);
    let has = |key: &str| {
        t.iter()
            .any(|(k, v)| k.eq_ignore_ascii_case(key) && !v.is_empty())
    };
    let mut out = Vec::new();
    if !has("WP") && !has("Change-Request") {
        out.push(Finding::new(
            "SYRP-CR-0003",
            sha,
            "commit has no `WP:` or `Change-Request:` trailer",
        ));
    }
    if !has("Agent-Model") {
        out.push(Finding::new(
            "SYRP-CR-0003",
            sha,
            "commit has no `Agent-Model:` trailer",
        ));
    }
    out
}

/// The header line every new source file of a crate with this licence must start with.
///
/// ```
/// use syrp_provenance::provenance::spdx_header;
/// assert_eq!(spdx_header("MIT"), "// SPDX-License-Identifier: MIT");
/// ```
pub fn spdx_header(licence: &str) -> String {
    format!("// SPDX-License-Identifier: {licence}")
}

/// Checks that `contents` starts with the SPDX header for `licence`.
///
/// ```
/// use syrp_provenance::provenance::check_spdx;
/// assert!(check_spdx("a.rs", "// SPDX-License-Identifier: MIT\nfn f() {}", "MIT").is_none());
/// assert!(check_spdx("a.rs", "fn f() {}", "MIT").is_some());
/// ```
pub fn check_spdx(path: &str, contents: &str, licence: &str) -> Option<Finding> {
    let expected = spdx_header(licence);
    let first = contents.trim_start_matches('\u{feff}').lines().next();
    match first.map(str::trim_end) {
        Some(line) if line == expected => None,
        Some(line) if line.starts_with("// SPDX-License-Identifier:") => Some(Finding::new(
            "SYRP-CR-0004",
            path,
            format!("SPDX header {line:?} does not match the crate licence; expected {expected:?}"),
        )),
        _ => Some(Finding::new(
            "SYRP-CR-0004",
            path,
            format!("new source file must start with {expected:?}"),
        )),
    }
}

fn read_toml(path: &Path) -> Result<toml::Table, Error> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| Error::Licence(format!("cannot read {}: {e}", path.display())))?;
    text.parse()
        .map_err(|e| Error::Licence(format!("{} is not valid TOML: {e}", path.display())))
}

/// Resolves the licence of the crate containing `rel_path` (relative to the workspace
/// `root`), or `None` when no `Cargo.toml` with a `[package]` lies between it and `root`.
///
/// ```
/// use syrp_provenance::provenance::crate_licence;
/// let none = crate_licence(std::path::Path::new("/nonexistent"), "a/b.rs").unwrap();
/// assert_eq!(none, None);
/// ```
pub fn crate_licence(root: &Path, rel_path: &str) -> Result<Option<String>, Error> {
    let mut dir = Path::new(rel_path).parent();
    while let Some(d) = dir {
        let manifest = root.join(d).join("Cargo.toml");
        if manifest.is_file() {
            let table = read_toml(&manifest)?;
            if let Some(pkg) = table.get("package") {
                return licence_of(root, &manifest, pkg).map(Some);
            }
        }
        dir = d.parent();
    }
    Ok(None)
}

fn licence_of(root: &Path, manifest: &Path, pkg: &toml::Value) -> Result<String, Error> {
    let missing = || Error::Licence(format!("{} has no package.license", manifest.display()));
    match pkg.get("license").ok_or_else(missing)? {
        toml::Value::String(s) => Ok(s.clone()),
        toml::Value::Table(t)
            if t.get("workspace").and_then(toml::Value::as_bool) == Some(true) =>
        {
            let ws = read_toml(&root.join("Cargo.toml"))?;
            ws.get("workspace")
                .and_then(|w| w.get("package"))
                .and_then(|p| p.get("license"))
                .and_then(toml::Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| {
                    Error::Licence("workspace Cargo.toml has no workspace.package.license".into())
                })
        }
        _ => Err(missing()),
    }
}

fn git(root: &Path, args: &[&str]) -> Result<String, Error> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| Error::Git(format!("cannot run git: {e}")))?;
    if !out.status.success() {
        return Err(Error::Git(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Lists the non-merge commits in `range` (any `git log` range, e.g. `origin/main..HEAD`).
///
/// ```
/// let err = syrp_provenance::provenance::git_commits("/nonexistent".as_ref(), "a..b");
/// assert_eq!(err.unwrap_err().code(), "SYRP-CR-0005");
/// ```
pub fn git_commits(root: &Path, range: &str) -> Result<Vec<Commit>, Error> {
    let raw = git(
        root,
        &["log", "--no-merges", "--format=%H%x00%B%x1e", range, "--"],
    )?;
    Ok(raw
        .split('\x1e')
        .filter_map(|rec| {
            let (sha, message) = rec.trim_start().split_once('\0')?;
            Some(Commit {
                sha: sha.to_owned(),
                message: message.to_owned(),
            })
        })
        .collect())
}

/// Lists the paths added (status `A`) between the endpoints of `range`.
///
/// ```
/// let err = syrp_provenance::provenance::git_added_files("/nonexistent".as_ref(), "a..b");
/// assert_eq!(err.unwrap_err().code(), "SYRP-CR-0005");
/// ```
pub fn git_added_files(root: &Path, range: &str) -> Result<Vec<String>, Error> {
    let raw = git(
        root,
        &["diff", "--name-only", "--diff-filter=A", "-z", range, "--"],
    )?;
    Ok(raw
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect())
}

/// Runs both checks over `range` in the repository at `root`. Added files are read from the
/// working tree, so check out the range's head commit first.
///
/// ```
/// let err = syrp_provenance::provenance::check_range("/nonexistent".as_ref(), "a..b");
/// assert!(err.is_err());
/// ```
pub fn check_range(root: &Path, range: &str) -> Result<ProvenanceReport, Error> {
    let mut report = ProvenanceReport::default();
    for c in git_commits(root, range)? {
        report.commits_checked += 1;
        report.findings.extend(check_commit(&c.sha, &c.message));
    }
    for path in git_added_files(root, range)? {
        if !path.ends_with(".rs") {
            continue;
        }
        let Some(licence) = crate_licence(root, &path)? else {
            continue;
        };
        let contents = std::fs::read_to_string(root.join(&path))
            .map_err(|e| Error::Io(format!("cannot read {path}: {e}")))?;
        report.files_checked += 1;
        report
            .findings
            .extend(check_spdx(&path, &contents, &licence));
    }
    Ok(report)
}

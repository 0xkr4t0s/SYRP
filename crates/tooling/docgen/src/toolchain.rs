//! Nightly detection (B3) and rustdoc JSON generation, which needs
//! `-Z unstable-options --output-format json`.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::DocError;

/// A nightly toolchain that can produce rustdoc JSON.
///
/// ```no_run
/// println!("{}", syrp_docgen::detect_nightly("nightly").unwrap().version);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NightlyToolchain {
    /// The rustup toolchain name, used as `cargo +<name>`.
    pub name: String,
    /// `rustdoc --version` output, for example `rustdoc 1.101.0-nightly (76456a688 2026-10-25)`.
    pub version: String,
}

/// Checks that rustup toolchain `toolchain` exists and its rustdoc is a nightly.
///
/// Returns [`DocError::NightlyMissing`] (SYRP-DOC-0001) when rustup is absent, the toolchain is
/// not installed, or its rustdoc is not a nightly build (it would reject the unstable flags).
///
/// ```
/// use syrp_docgen::Diagnostic;
/// let err = syrp_docgen::detect_nightly("syrp-no-such-toolchain").unwrap_err();
/// assert_eq!(err.code().as_str(), "SYRP-DOC-0001");
/// ```
pub fn detect_nightly(toolchain: &str) -> Result<NightlyToolchain, DocError> {
    let name = toolchain.to_owned();
    let missing = |detail: String| DocError::NightlyMissing {
        toolchain: name.clone(),
        detail,
    };
    let mut cmd = Command::new("rustup");
    cmd.args(["run", toolchain, "rustdoc", "--version"])
        .env_remove("RUSTUP_TOOLCHAIN");
    let out = cmd
        .output()
        .map_err(|e| missing(format!("cannot run rustup: {e}")))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let first = stderr.lines().next().unwrap_or("not installed");
        return Err(missing(first.trim().to_owned()));
    }
    let version = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    if !(version.contains("-nightly") || version.contains("-dev")) {
        return Err(missing(format!("`{version}` is not a nightly")));
    }
    Ok(NightlyToolchain { name, version })
}

/// Runs `cargo +<nightly> rustdoc -p <package> --lib -- -Z unstable-options --output-format json`
/// in `workspace` with `--target-dir target_dir`, and returns the path of the JSON it wrote.
///
/// Fails with SYRP-DOC-0003 (with the end of rustdoc's stderr) if cargo or rustdoc fails.
///
/// ```no_run
/// let nightly = syrp_docgen::detect_nightly("nightly").unwrap();
/// let target = std::path::Path::new("target/docgen");
/// let json = syrp_docgen::generate_rustdoc_json(&nightly, ".".as_ref(), "syrp-docgen", target);
/// assert!(json.unwrap().ends_with("doc/syrp_docgen.json"));
/// ```
pub fn generate_rustdoc_json(
    nightly: &NightlyToolchain,
    workspace: &Path,
    package: &str,
    target_dir: &Path,
) -> Result<PathBuf, DocError> {
    let json = target_dir.join(format!("doc/{}.json", package.replace('-', "_")));
    let fail = |detail: String| DocError::Read {
        path: json.clone(),
        detail,
    };
    let mut cmd = Command::new("cargo");
    cmd.arg(format!("+{}", nightly.name))
        .args(["rustdoc", "--quiet", "--lib", "-p", package]);
    cmd.arg("--target-dir")
        .arg(target_dir)
        .current_dir(workspace);
    cmd.args(["--", "-Z", "unstable-options", "--output-format", "json"]);
    let out = cmd.env_remove("RUSTUP_TOOLCHAIN").output();
    let out = out.map_err(|e| fail(format!("cannot run cargo: {e}")))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let lines: Vec<&str> = stderr.lines().collect();
        let tail = lines[lines.len().saturating_sub(5)..].join("\n");
        return Err(fail(format!("cargo rustdoc failed: {tail}")));
    }
    Ok(json)
}

//! The error-code catalogue (`errors/*.toml`) and `cargo syrp explain`.
//!
//! The catalogue files are embedded at build time so `explain` works anywhere, including
//! outside a checkout.

use serde::{Deserialize, Serialize};

use crate::envelope::{Diagnostic, ExitCode, Outcome};

/// The catalogue sources, embedded. Add a domain here when adding `errors/<domain>.toml`.
const SOURCES: &[(&str, &str)] = &[
    (
        "analytics",
        include_str!("../../../../errors/analytics.toml"),
    ),
    ("audio", include_str!("../../../../errors/audio.toml")),
    (
        "cleanroom",
        include_str!("../../../../errors/cleanroom.toml"),
    ),
    ("cli", include_str!("../../../../errors/cli.toml")),
    (
        "conductor",
        include_str!("../../../../errors/conductor.toml"),
    ),
    ("device", include_str!("../../../../errors/device.toml")),
    ("doc", include_str!("../../../../errors/doc.toml")),
    ("error", include_str!("../../../../errors/error.toml")),
    ("eval", include_str!("../../../../errors/eval.toml")),
    ("format", include_str!("../../../../errors/format.toml")),
    ("gfx", include_str!("../../../../errors/gfx.toml")),
    ("io", include_str!("../../../../errors/io.toml")),
    ("loop", include_str!("../../../../errors/loop.toml")),
    ("maths", include_str!("../../../../errors/maths.toml")),
    ("midi", include_str!("../../../../errors/midi.toml")),
    ("net", include_str!("../../../../errors/net.toml")),
    ("peer", include_str!("../../../../errors/peer.toml")),
    ("plugin", include_str!("../../../../errors/plugin.toml")),
    ("pr", include_str!("../../../../errors/pr.toml")),
    ("progress", include_str!("../../../../errors/progress.toml")),
    ("rt", include_str!("../../../../errors/rt.toml")),
    ("state", include_str!("../../../../errors/state.toml")),
    ("strings", include_str!("../../../../errors/strings.toml")),
    ("sys", include_str!("../../../../errors/sys.toml")),
    ("testkit", include_str!("../../../../errors/testkit.toml")),
    ("thread", include_str!("../../../../errors/thread.toml")),
    ("var", include_str!("../../../../errors/var.toml")),
    ("wp", include_str!("../../../../errors/wp.toml")),
];

/// One catalogue entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Entry {
    /// `SYRP-<DOMAIN>-<NNNN>`.
    pub code: String,
    /// Short title.
    pub title: String,
    /// `error`, `warning` or `info`.
    pub severity: String,
    /// Why it happens.
    pub cause: String,
    /// How to fix it.
    pub fix: String,
    /// Optional worked example.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub example: Option<String>,
}

#[derive(Deserialize)]
struct File {
    error: Vec<Entry>,
}

/// Loads every embedded entry. Panics only if an embedded file is malformed, which the
/// unit tests below catch at build time.
pub fn all() -> Vec<Entry> {
    let mut out = Vec::new();
    for (domain, src) in SOURCES {
        let file: File = toml::from_str(src)
            .unwrap_or_else(|e| panic!("errors/{domain}.toml is malformed: {e}"));
        out.extend(file.error);
    }
    out
}

/// Looks up one code, case-insensitively.
pub fn lookup(code: &str) -> Option<Entry> {
    all()
        .into_iter()
        .find(|e| e.code.eq_ignore_ascii_case(code))
}

/// `cargo syrp explain <code>`.
pub fn explain(code: &str) -> Outcome {
    match lookup(code) {
        Some(entry) => Outcome::result(serde_json::to_value(entry).expect("entry serialises")),
        None => Outcome {
            diagnostics: vec![
                Diagnostic::error("SYRP-CLI-0002", format!("unknown error code {code}"))
                    .hint("codes look like SYRP-CLI-0203; `cargo syrp explain --list` lists them"),
            ],
            exit: Some(ExitCode::Usage),
            ..Default::default()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn catalogue_parses_and_codes_are_unique_and_well_formed() {
        let entries = all();
        assert!(!entries.is_empty());
        let mut seen = HashSet::new();
        for e in &entries {
            assert!(seen.insert(e.code.clone()), "duplicate code {}", e.code);
            let parts: Vec<_> = e.code.split('-').collect();
            assert_eq!(parts.len(), 3, "bad code {}", e.code);
            assert_eq!(parts[0], "SYRP");
            assert!(
                parts[1].chars().all(|c| c.is_ascii_uppercase()),
                "bad domain {}",
                e.code
            );
            assert!(parts[2].len() == 4 && parts[2].chars().all(|c| c.is_ascii_digit()));
            assert!(["error", "warning", "info"].contains(&e.severity.as_str()));
        }
    }

    /// A new `errors/<domain>.toml` is invisible to `explain` until it is listed in `SOURCES`.
    #[test]
    fn every_errors_file_is_embedded() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../errors");
        let embedded: HashSet<&str> = SOURCES.iter().map(|(d, _)| *d).collect();
        for f in crate::repo::files_with_suffix(&dir, ".toml") {
            let domain = f.file_stem().unwrap().to_str().unwrap().to_owned();
            assert!(
                embedded.contains(domain.as_str()),
                "errors/{domain}.toml is not in catalogue::SOURCES"
            );
        }
    }

    #[test]
    fn explain_is_case_insensitive() {
        assert!(lookup("syrp-cli-0001").is_some());
        assert_eq!(
            explain(&format!("SYRP-{}-0000", "NOPE")).exit_code(),
            ExitCode::Usage
        );
    }
}

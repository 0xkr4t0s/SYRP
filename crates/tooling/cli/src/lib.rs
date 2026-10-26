//! Library behind `cargo syrp`.
//!
//! The CLI, the MCP server (WP-0.2-10) and the Agent Skill's scripts all call this library,
//! so their results and schemas are identical (master plan §13.3). Every command returns an
//! [`envelope::Outcome`]; the binary wraps it in an [`envelope::Envelope`] and exits with
//! its [`envelope::ExitCode`].

// Diagnostics are deliberately rich (hint, location, docs); the CLI is not a hot path.
#![allow(clippy::result_large_err)]

pub mod catalogue;
pub mod checks;
pub mod cli;
pub mod conductor;
pub mod docs;
pub mod envelope;
pub mod gates;
pub mod repo;
pub mod review;
pub mod wp;

/// JSON Schema of the result envelope, published on the agent docs side.
pub const ENVELOPE_JSON_SCHEMA: &str = include_str!("../../../../docs/agents/envelope.schema.json");

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    /// Every `SYRP-*` code the CLI emits must be documented in `errors/*.toml`.
    #[test]
    fn every_emitted_code_is_catalogued() {
        let known: BTreeSet<String> = crate::catalogue::all()
            .into_iter()
            .map(|e| e.code)
            .collect();
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        for f in crate::repo::files_with_suffix(&dir, ".rs") {
            let src = std::fs::read_to_string(&f).unwrap();
            for (i, _) in src.match_indices("\"SYRP-") {
                let code: String = src[i + 1..].chars().take_while(|c| *c != '"').collect();
                let parts: Vec<&str> = code.split('-').collect();
                let well_formed = parts.len() == 3
                    && parts[1].chars().all(|c| c.is_ascii_uppercase())
                    && parts[2].len() == 4
                    && parts[2].chars().all(|c| c.is_ascii_digit());
                if well_formed {
                    assert!(
                        known.contains(&code),
                        "{code} used in {} is not catalogued",
                        f.display()
                    );
                }
            }
        }
    }

    #[test]
    fn envelope_schema_is_json() {
        let v: serde_json::Value = serde_json::from_str(super::ENVELOPE_JSON_SCHEMA).unwrap();
        assert_eq!(
            v["properties"]["schema"]["const"],
            crate::envelope::ENVELOPE_SCHEMA
        );
    }
}

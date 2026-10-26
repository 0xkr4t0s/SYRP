//! Structured reviews (master plan §12.5).
//!
//! Reviewer agents never set statuses or approve directly. They write a review JSON file;
//! `cargo syrp review validate` checks it against the checklist and derives the verdict,
//! and the workflow (not the agent) posts the comment and the commit status.

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::envelope::{Diagnostic, ExitCode, Outcome};

/// The checklist every review must answer, in order.
pub const CHECKLIST: &[(&str, &str)] = &[
    (
        "spec-conformance",
        "Every behaviour statement in the WP spec maps to at least one test",
    ),
    (
        "juce-semantics",
        "WP scenarios pass at the declared tolerance, plus a 10% sample of others",
    ),
    (
        "tests",
        "Unit, property and compile-fail tests where applicable",
    ),
    (
        "rt-safety",
        "No allocation or locking reachable from RtCtx; RtSafe claims justified",
    ),
    (
        "api-conventions",
        "Naming, thread tokens, error codes, doc aliases, no items outside the contract",
    ),
    (
        "docs",
        "Public items documented with examples; AGENTS.md, errors and changes/ updated",
    ),
    (
        "clean-room",
        "No JUCE-derived text or constants; dependency licences in policy; trailers present",
    ),
    (
        "unsafe-ffi",
        "Every unsafe block has a SAFETY comment; Miri or sanitisers clean",
    ),
    (
        "performance",
        "Benchmarks within budget where the WP declares them",
    ),
];

/// One checklist answer.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Item {
    /// A [`CHECKLIST`] id.
    pub id: String,
    /// Passed (or not applicable).
    pub pass: bool,
    /// Not applicable to this PR.
    #[serde(default)]
    pub na: bool,
    /// What the reviewer ran or read. Required: the reviewer must show its own evidence.
    #[serde(default)]
    pub evidence: String,
}

/// A review file.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Review {
    /// PR number.
    pub pr: u64,
    /// WP id.
    #[serde(default)]
    pub wp: Option<String>,
    /// `primary` or `adversarial`.
    #[serde(default = "primary")]
    pub kind: String,
    /// Checklist answers.
    pub items: Vec<Item>,
    /// Overall summary.
    #[serde(default)]
    pub summary: String,
    /// Concrete changes the implementer must make.
    #[serde(default)]
    pub requested_changes: Vec<String>,
    /// Commands the reviewer ran, with their exit codes.
    #[serde(default)]
    pub commands_run: Vec<String>,
}

fn primary() -> String {
    "primary".into()
}

/// Validates a review and derives its verdict and comment body.
pub fn validate(src: &str) -> Outcome {
    let review: Review = match serde_json::from_str(src) {
        Ok(r) => r,
        Err(e) => {
            return Outcome {
                diagnostics: vec![Diagnostic::error(
                    "SYRP-PR-0007",
                    format!("review JSON is invalid: {e}"),
                )],
                exit: Some(ExitCode::Usage),
                ..Default::default()
            };
        }
    };
    let mut diags = Vec::new();
    for (id, _) in CHECKLIST {
        match review.items.iter().find(|i| i.id == *id) {
            None => diags.push(Diagnostic::error(
                "SYRP-PR-0007",
                format!("checklist item `{id}` is missing"),
            )),
            Some(i) if i.pass && !i.na && i.evidence.trim().is_empty() => {
                diags.push(Diagnostic::error(
                    "SYRP-PR-0007",
                    format!("`{id}` passes without evidence; attach what you ran"),
                ))
            }
            _ => {}
        }
    }
    if review.commands_run.is_empty() {
        diags.push(Diagnostic::error(
            "SYRP-PR-0007",
            "commands_run is empty: reviewers rerun everything themselves",
        ));
    }
    if !diags.is_empty() {
        return Outcome {
            diagnostics: diags,
            exit: Some(ExitCode::Usage),
            ..Default::default()
        };
    }

    let failed: Vec<&Item> = review.items.iter().filter(|i| !i.pass).collect();
    let approve = failed.is_empty() && review.requested_changes.is_empty();
    let mut body = format!(
        "### {} review: {}\n\n{}\n\n| Check | Result | Evidence |\n|---|---|---|\n",
        review.kind,
        if approve {
            "approved"
        } else {
            "changes requested"
        },
        review.summary
    );
    for (id, desc) in CHECKLIST {
        let i = review
            .items
            .iter()
            .find(|i| i.id == *id)
            .expect("validated above");
        let result = if i.na {
            "n/a"
        } else if i.pass {
            "pass"
        } else {
            "FAIL"
        };
        body.push_str(&format!(
            "| {id}: {desc} | {result} | {} |\n",
            i.evidence.replace('\n', " ").replace('|', "\\|")
        ));
    }
    if !review.requested_changes.is_empty() {
        body.push_str("\n**Requested changes**\n\n");
        for c in &review.requested_changes {
            body.push_str(&format!("- {c}\n"));
        }
    }
    body.push_str("\n<details><summary>Commands run</summary>\n\n```\n");
    body.push_str(&review.commands_run.join("\n"));
    body.push_str("\n```\n</details>\n");

    Outcome::result(json!({
        "pr": review.pr,
        "kind": review.kind,
        "approve": approve,
        "state": if approve { "success" } else { "failure" },
        "body": body,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn review(pass_all: bool, evidence: &str) -> String {
        let items: Vec<_> = CHECKLIST
            .iter()
            .enumerate()
            .map(|(n, (id, _))| json!({"id": id, "pass": pass_all || n > 0, "evidence": evidence}))
            .collect();
        json!({"pr": 1, "items": items, "summary": "s", "commands_run": ["cargo syrp wp check X"]})
            .to_string()
    }

    #[test]
    fn complete_passing_review_approves() {
        let o = validate(&review(true, "ran tests"));
        assert_eq!(o.exit_code(), ExitCode::Ok);
        assert_eq!(o.results[0]["approve"], true);
    }

    #[test]
    fn any_failure_requests_changes() {
        let o = validate(&review(false, "ran tests"));
        assert_eq!(o.results[0]["approve"], false);
        assert_eq!(o.results[0]["state"], "failure");
    }

    #[test]
    fn passes_without_evidence_are_rejected() {
        let o = validate(&review(true, ""));
        assert_eq!(o.exit_code(), ExitCode::Usage);
    }

    #[test]
    fn missing_items_are_rejected() {
        let o = validate(r#"{"pr":1,"items":[],"commands_run":["x"]}"#);
        assert_eq!(o.diagnostics.len(), CHECKLIST.len());
    }
}

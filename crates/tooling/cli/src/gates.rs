//! Merge gates evaluated by the PR manager (master plan §12.8).
//!
//! Gates that are CI jobs (tests, parity, contracts, deny, docs, lints) are folded into G1:
//! every required check must be green. The gates here are the ones that need PR metadata:
//! front matter, size, ownership, provenance, review statuses and risk routing.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::conductor::Label;
use crate::envelope::{Diagnostic, ExitCode, Outcome};
use crate::wp::{Board, Ownership, Role, WorkPackage};

/// Commit-status context the reviewer workflow sets.
pub const REVIEW_CONTEXT: &str = "syrp/review";
/// Commit-status context the adversarial verifier sets.
pub const ADVERSARIAL_CONTEXT: &str = "syrp/adversarial";
/// Checks produced by the pipeline itself (the review statuses and the jobs of `reviewer.yml`),
/// which G1 must not wait on. Matched exactly, or as a prefix when ending in `(`.
const PIPELINE_CHECKS: &[&str] = &[
    REVIEW_CONTEXT,
    ADVERSARIAL_CONTEXT,
    "route",
    "publish",
    "review (",
    "gates",
];

fn is_pipeline_check(name: &str) -> bool {
    PIPELINE_CHECKS.iter().any(|p| {
        if p.ends_with('(') {
            name.starts_with(p)
        } else {
            name == *p
        }
    })
}
/// The per-PR size cap in changed lines.
pub const MAX_CHANGED_LINES: u64 = 1200;
/// Files that do not count towards the size cap.
const SIZE_EXEMPT: &[&str] = &["Cargo.lock"];
const FRONT_MATTER_OPEN: &str = "<!-- syrp-pr: ";

/// A changed file.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileChange {
    /// Path.
    pub path: String,
    /// Added lines.
    #[serde(default)]
    pub additions: u64,
    /// Deleted lines.
    #[serde(default)]
    pub deletions: u64,
}

/// A commit.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    /// First line.
    #[serde(default)]
    pub message_headline: String,
    /// The rest, where trailers live.
    #[serde(default)]
    pub message_body: String,
}

/// A PR as printed by `gh pr view <n> --json number,body,labels,files,commits,statusCheckRollup,mergeable,isDraft`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrView {
    /// Number.
    pub number: u64,
    /// Body holding the front matter.
    #[serde(default)]
    pub body: String,
    /// Labels.
    #[serde(default)]
    pub labels: Vec<Label>,
    /// Changed files.
    #[serde(default)]
    pub files: Vec<FileChange>,
    /// Commits.
    #[serde(default)]
    pub commits: Vec<Commit>,
    /// Check runs and status contexts, as raw JSON.
    #[serde(default)]
    pub status_check_rollup: Vec<Value>,
    /// `MERGEABLE`, `CONFLICTING` or `UNKNOWN`.
    #[serde(default)]
    pub mergeable: String,
    /// Draft?
    #[serde(default)]
    pub is_draft: bool,
}

/// What a PR is for.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// An increment of a work package.
    #[default]
    Wp,
    /// A batched shared-file change by the PR manager.
    ChangeRequest,
}

/// Machine-readable PR front matter (master plan §12.6).
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq)]
#[serde(default)]
pub struct FrontMatter {
    /// WP id; required when `kind` is `wp`.
    pub wp: Option<String>,
    /// What the PR is for.
    pub kind: Kind,
    /// True on the PR that completes the WP; its body must say `Closes #<issue>`.
    #[serde(rename = "final")]
    pub is_final: bool,
    /// Risk categories the author declares.
    pub risk: Vec<String>,
    /// Contract crates touched.
    pub contracts_touched: Vec<String>,
    /// Ledger rows moved.
    pub ledger_rows: Vec<String>,
    /// New third-party dependencies.
    pub deps_added: Vec<String>,
    /// `unsafe` blocks added.
    pub unsafe_count: u32,
}

impl FrontMatter {
    /// Extracts the front matter from a PR body.
    pub fn parse(body: &str) -> Result<FrontMatter, String> {
        let start = body
            .find(FRONT_MATTER_OPEN)
            .ok_or("no `<!-- syrp-pr: {...} -->` marker")?;
        let rest = &body[start + FRONT_MATTER_OPEN.len()..];
        let end = rest
            .find(" -->")
            .ok_or("unterminated front matter marker")?;
        serde_json::from_str(&rest[..end])
            .map_err(|e| format!("front matter is not valid JSON: {e}"))
    }
}

/// Overall decision.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum Verdict {
    /// Every gate passed: enqueue for merge.
    Merge,
    /// Something is still running.
    Wait,
    /// The branch conflicts with main: the PR manager agent rebases it.
    Conflict,
    /// A gate failed.
    Block,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Pass,
    Pending,
    Fail,
}

fn check_state(v: &Value) -> (String, State) {
    // CheckRun: {name, status, conclusion}; StatusContext: {context, state}.
    if let Some(ctx) = v.get("context").and_then(Value::as_str) {
        let state = match v.get("state").and_then(Value::as_str).unwrap_or("") {
            "SUCCESS" => State::Pass,
            "PENDING" | "EXPECTED" | "" => State::Pending,
            _ => State::Fail,
        };
        return (ctx.to_string(), state);
    }
    let name = v
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("?")
        .to_string();
    if v.get("status").and_then(Value::as_str) != Some("COMPLETED") {
        return (name, State::Pending);
    }
    let state = match v.get("conclusion").and_then(Value::as_str).unwrap_or("") {
        "SUCCESS" | "SKIPPED" | "NEUTRAL" => State::Pass,
        _ => State::Fail,
    };
    (name, state)
}

/// Risk categories implied by which paths a PR touches.
pub fn path_risks(files: &[FileChange], own: &Ownership) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for f in files {
        if f.path.starts_with("contracts/") {
            out.insert("contract-change".to_string());
        }
        if let Some(pm) = own.roles.get("pr-manager")
            && pm.paths.iter().any(|p| crate::wp::path_matches(p, &f.path))
        {
            out.insert("ci-infra".to_string());
        }
        if f.path.starts_with("LICENSES/") || f.path == "LICENSE.md" || f.path == "deny.toml" {
            out.insert("licensing".to_string());
        }
    }
    out
}

/// Evaluates every gate for one PR.
pub fn evaluate(pr: &PrView, board: &Board, own: &Ownership) -> Outcome {
    let mut gates: Vec<Value> = Vec::new();
    let mut diags = Vec::new();
    let mut verdict = Verdict::Merge;
    let mut extra_diags = Vec::new();
    let mut record = |name: &str, state: State, detail: String, diags_out: Option<Diagnostic>| {
        gates.push(json!({"gate": name, "state": match state {
            State::Pass => "pass", State::Pending => "pending", State::Fail => "fail" }, "detail": detail}));
        let v = match state {
            State::Pass => Verdict::Merge,
            State::Pending => Verdict::Wait,
            State::Fail => Verdict::Block,
        };
        verdict = verdict.max(v);
        if let Some(d) = diags_out {
            diags.push(d);
        }
    };

    if pr.is_draft {
        record("draft", State::Pending, "PR is a draft".into(), None);
    }

    // Front matter and the WP it names.
    let fm = FrontMatter::parse(&pr.body);
    let wp: Option<&WorkPackage> = match &fm {
        Ok(fm) if fm.kind == Kind::Wp => {
            match fm.wp.as_deref().and_then(|id| board.packages.get(id)) {
                Some(wp) => {
                    record("front-matter", State::Pass, format!("WP {}", wp.id), None);
                    Some(wp)
                }
                None => {
                    let msg = format!("front matter names unknown WP {:?}", fm.wp);
                    record(
                        "front-matter",
                        State::Fail,
                        msg.clone(),
                        Some(Diagnostic::error("SYRP-PR-0005", msg)),
                    );
                    None
                }
            }
        }
        Ok(_) => {
            record("front-matter", State::Pass, "change request".into(), None);
            None
        }
        Err(e) => {
            record(
                "front-matter",
                State::Fail,
                e.clone(),
                Some(Diagnostic::error("SYRP-PR-0005", e.clone())),
            );
            None
        }
    };

    // Size.
    let changed: u64 = pr
        .files
        .iter()
        .filter(|f| !SIZE_EXEMPT.contains(&f.path.as_str()))
        .map(|f| f.additions + f.deletions)
        .sum();
    if changed > MAX_CHANGED_LINES {
        let msg = format!("{changed} changed lines exceeds the {MAX_CHANGED_LINES}-line cap");
        record(
            "size",
            State::Fail,
            msg.clone(),
            Some(Diagnostic::error("SYRP-PR-0001", msg).hint("split the PR")),
        );
    } else {
        record("size", State::Pass, format!("{changed} lines"), None);
    }

    // Ownership.
    let paths: Vec<String> = pr.files.iter().map(|f| f.path.clone()).collect();
    let owner_diags = match (wp, &fm) {
        (Some(wp), _) => own.check_paths(wp, &paths),
        (None, Ok(fm)) if fm.kind == Kind::ChangeRequest => {
            // Change requests are PR-manager work: shared files plus anything a WP may touch.
            let pm = WorkPackage {
                id: "change-request".into(),
                title: String::new(),
                crate_path: None,
                paths: vec![],
                depends_on: vec![],
                contracts: vec![],
                ledger_rows: vec![],
                spec: String::new(),
                scenarios: vec![],
                tolerance: None,
                risk: vec![],
                acceptance: String::new(),
                platforms: vec![],
                budget: Default::default(),
                role: Role::PrManager,
                status: Default::default(),
            };
            own.check_paths(&pm, &paths)
        }
        _ => vec![],
    };
    if owner_diags.is_empty() {
        record("ownership", State::Pass, "all paths owned".into(), None);
    } else {
        let n = owner_diags.len();
        record(
            "ownership",
            State::Fail,
            format!("{n} path(s) outside ownership"),
            None,
        );
        extra_diags.extend(owner_diags);
    }

    // Provenance trailers on every commit (master plan §10.5).
    let missing: Vec<&str> = pr
        .commits
        .iter()
        .filter(|c| {
            let has = |t: &str| c.message_body.lines().any(|l| l.starts_with(t));
            !(has("WP:") || has("Change-Request:")) || !has("Agent-Model:")
        })
        .map(|c| c.message_headline.as_str())
        .collect();
    if missing.is_empty() {
        record(
            "provenance",
            State::Pass,
            format!("{} commit(s)", pr.commits.len()),
            None,
        );
    } else {
        let msg = format!(
            "{} commit(s) lack WP:/Agent-Model: trailers: {}",
            missing.len(),
            missing.join("; ")
        );
        record(
            "provenance",
            State::Fail,
            msg.clone(),
            Some(Diagnostic::error("SYRP-PR-0006", msg)),
        );
    }

    // G1: CI, G2: review, G10: adversarial.
    let checks: Vec<(String, State)> = pr.status_check_rollup.iter().map(check_state).collect();
    let ci: Vec<&(String, State)> = checks
        .iter()
        .filter(|(n, _)| !is_pipeline_check(n))
        .collect();
    let failed: Vec<&str> = ci
        .iter()
        .filter(|(_, s)| *s == State::Fail)
        .map(|(n, _)| n.as_str())
        .collect();
    if ci.is_empty() {
        record(
            "G1-ci",
            State::Pending,
            "no checks reported yet".into(),
            None,
        );
    } else if !failed.is_empty() {
        let msg = format!("failing: {}", failed.join(", "));
        record(
            "G1-ci",
            State::Fail,
            msg.clone(),
            Some(Diagnostic::error("SYRP-PR-0002", msg)),
        );
    } else if ci.iter().any(|(_, s)| *s == State::Pending) {
        record("G1-ci", State::Pending, "checks running".into(), None);
    } else {
        record(
            "G1-ci",
            State::Pass,
            format!("{} checks green", ci.len()),
            None,
        );
    }

    let status_of = |ctx: &str| checks.iter().find(|(n, _)| n == ctx).map(|(_, s)| *s);
    match status_of(REVIEW_CONTEXT) {
        Some(State::Pass) => record("G2-review", State::Pass, "approved".into(), None),
        Some(State::Fail) => record(
            "G2-review",
            State::Fail,
            "changes requested".into(),
            Some(Diagnostic::error(
                "SYRP-PR-0003",
                "reviewer requested changes",
            )),
        ),
        _ => record(
            "G2-review",
            State::Pending,
            "awaiting reviewer".into(),
            None,
        ),
    }

    let mut risks: BTreeSet<String> = path_risks(&pr.files, own);
    risks.extend(
        pr.labels
            .iter()
            .filter_map(|l| l.name.strip_prefix("risk:").map(str::to_string)),
    );
    if let Ok(fm) = &fm {
        risks.extend(fm.risk.iter().cloned());
        if fm.unsafe_count > 0 {
            risks.insert("unsafe".into());
        }
        if !fm.deps_added.is_empty() {
            risks.insert("supply-chain".into());
        }
    }
    if let Some(wp) = wp {
        risks.extend(wp.risk.iter().cloned());
    }
    if risks.is_empty() {
        record("G10-adversarial", State::Pass, "not required".into(), None);
    } else {
        let list = risks.iter().cloned().collect::<Vec<_>>().join(", ");
        match status_of(ADVERSARIAL_CONTEXT) {
            Some(State::Pass) => record(
                "G10-adversarial",
                State::Pass,
                format!("approved for {list}"),
                None,
            ),
            Some(State::Fail) => record(
                "G10-adversarial",
                State::Fail,
                format!("rejected ({list})"),
                Some(Diagnostic::error(
                    "SYRP-PR-0004",
                    format!("adversarial verifier rejected ({list})"),
                )),
            ),
            _ => record(
                "G10-adversarial",
                State::Pending,
                format!("required for {list}"),
                None,
            ),
        }
    }

    diags.extend(extra_diags);
    if pr.mergeable == "CONFLICTING" {
        verdict = verdict.max(Verdict::Conflict);
    }

    let exit = if verdict == Verdict::Block {
        Some(ExitCode::Failed)
    } else {
        Some(ExitCode::Ok)
    };
    Outcome {
        results: vec![json!({
            "pr": pr.number,
            "verdict": verdict,
            "risks": risks,
            "final": fm.as_ref().map(|f| f.is_final).unwrap_or(false),
            "gates": gates,
        })],
        diagnostics: diags,
        artifacts: vec![],
        exit,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wp::tests::{board, wp};

    fn own() -> Ownership {
        toml::from_str(
            r#"
            [roles.pr-manager]
            paths = ["Cargo.toml", ".github/"]
            [any-wp]
            paths = ["changes/"]
            "#,
        )
        .unwrap()
    }

    fn green_pr() -> PrView {
        PrView {
            number: 7,
            body: "<!-- syrp-pr: {\"wp\":\"A\",\"final\":true} -->\nCloses #3".into(),
            labels: vec![],
            files: vec![FileChange {
                path: "crates/x/A/src/lib.rs".into(),
                additions: 100,
                deletions: 2,
            }],
            commits: vec![Commit {
                message_headline: "[A] thing".into(),
                message_body: "WP: A\nAgent-Model: m".into(),
            }],
            status_check_rollup: vec![
                json!({"name": "test", "status": "COMPLETED", "conclusion": "SUCCESS"}),
                json!({"context": REVIEW_CONTEXT, "state": "SUCCESS"}),
                json!({"name": "review (primary)", "status": "IN_PROGRESS"}),
            ],
            mergeable: "MERGEABLE".into(),
            is_draft: false,
        }
    }

    fn verdict(o: &Outcome) -> String {
        o.results[0]["verdict"].as_str().unwrap().to_string()
    }

    #[test]
    fn all_green_merges() {
        let o = evaluate(&green_pr(), &board(vec![wp("A", &[])]), &own());
        assert_eq!(verdict(&o), "merge", "{:#?}", o.results);
        assert_eq!(o.results[0]["final"], true);
    }

    #[test]
    fn pending_review_waits_and_failed_ci_blocks() {
        let b = board(vec![wp("A", &[])]);
        let mut pr = green_pr();
        pr.status_check_rollup
            .retain(|c| c.get("context").and_then(Value::as_str) != Some(REVIEW_CONTEXT));
        assert_eq!(verdict(&evaluate(&pr, &b, &own())), "wait");
        pr.status_check_rollup[0] =
            json!({"name": "test", "status": "COMPLETED", "conclusion": "FAILURE"});
        let o = evaluate(&pr, &b, &own());
        assert_eq!(verdict(&o), "block");
        assert!(o.diagnostics.iter().any(|d| d.code == "SYRP-PR-0002"));
    }

    #[test]
    fn oversize_foreign_paths_and_missing_trailers_block() {
        let b = board(vec![wp("A", &[])]);
        let mut pr = green_pr();
        pr.files.push(FileChange {
            path: "Cargo.toml".into(),
            additions: 1200,
            deletions: 0,
        });
        pr.commits[0].message_body = "no trailers".into();
        let o = evaluate(&pr, &b, &own());
        let codes: Vec<_> = o.diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(codes.contains(&"SYRP-PR-0001"));
        assert!(codes.contains(&"SYRP-WP-0005"));
        assert!(codes.contains(&"SYRP-PR-0006"));
        // Touching a shared file also makes the PR high-risk.
        assert!(
            o.results[0]["risks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r == "ci-infra")
        );
    }

    #[test]
    fn risky_prs_need_the_adversarial_verifier() {
        let mut w = wp("A", &[]);
        w.risk = vec!["public-api".into()];
        let b = board(vec![w]);
        let mut pr = green_pr();
        assert_eq!(verdict(&evaluate(&pr, &b, &own())), "wait");
        pr.status_check_rollup
            .push(json!({"context": ADVERSARIAL_CONTEXT, "state": "SUCCESS"}));
        assert_eq!(verdict(&evaluate(&pr, &b, &own())), "merge");
    }

    #[test]
    fn conflicts_and_missing_front_matter() {
        let b = board(vec![wp("A", &[])]);
        let mut pr = green_pr();
        pr.mergeable = "CONFLICTING".into();
        assert_eq!(verdict(&evaluate(&pr, &b, &own())), "conflict");
        pr.body = "nothing".into();
        let o = evaluate(&pr, &b, &own());
        assert_eq!(verdict(&o), "block");
        assert!(o.diagnostics.iter().any(|d| d.code == "SYRP-PR-0005"));
    }
}

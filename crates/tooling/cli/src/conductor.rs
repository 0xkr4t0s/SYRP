//! The conductor's scheduling core (master plan §12.3).
//!
//! This is deterministic code, not an LLM. It is a pure function from the board (the
//! `work/` files) plus a snapshot of GitHub state (issues and pull requests as printed by
//! `gh ... --json`) to a list of actions. The `conductor.yml` workflow gathers the snapshot,
//! runs `cargo syrp conductor tick`, and applies the actions with `gh`.
//!
//! Live WP state is kept on each WP's GitHub issue:
//! - labels: `wp:<id>`, `status:{ready,in-progress,in-review}`, `needs-human`;
//! - a lease marker in the issue body: `<!-- syrp-lease: {...} -->`.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::wp::{Board, Role, SpecStatus};

/// Label name prefix carrying a WP id.
pub const WP_LABEL: &str = "wp:";
const MARKER_OPEN: &str = "<!-- syrp-lease: ";
const MARKER_CLOSE: &str = " -->";

/// A GitHub label as `gh` prints it.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Label {
    /// Label name.
    pub name: String,
}

/// An issue as printed by `gh issue list --json number,title,state,labels,body`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Issue {
    /// Issue number.
    pub number: u64,
    /// Title.
    #[serde(default)]
    pub title: String,
    /// `OPEN` or `CLOSED`.
    pub state: String,
    /// Labels.
    #[serde(default)]
    pub labels: Vec<Label>,
    /// Body, holding the lease marker.
    #[serde(default)]
    pub body: String,
}

/// A pull request as printed by `gh pr list --json number,isDraft,labels,headRefName,body`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PullRequest {
    /// PR number.
    pub number: u64,
    /// Draft PRs are still being written by their implementer.
    #[serde(default)]
    pub is_draft: bool,
    /// Labels.
    #[serde(default)]
    pub labels: Vec<Label>,
    /// Branch name, `wp/<id>-<slug>` by convention.
    #[serde(default)]
    pub head_ref_name: String,
}

fn has_label(labels: &[Label], name: &str) -> bool {
    labels.iter().any(|l| l.name == name)
}

fn wp_of_labels(labels: &[Label]) -> Option<String> {
    labels
        .iter()
        .find_map(|l| l.name.strip_prefix(WP_LABEL).map(str::to_string))
}

impl PullRequest {
    /// The WP this PR belongs to: its `wp:` label, else its `wp/<id>` branch.
    pub fn wp(&self) -> Option<String> {
        wp_of_labels(&self.labels).or_else(|| {
            let rest = self.head_ref_name.strip_prefix("wp/")?;
            // Branch is wp/WP-0.1-03-some-slug: the id is the first three dash-separated parts.
            let parts: Vec<&str> = rest.splitn(4, '-').collect();
            (parts.len() >= 3).then(|| parts[..3].join("-"))
        })
    }
}

/// Per-issue lease and counters, stored as JSON in the issue body.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Lease {
    /// Who holds it (for example the workflow run id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// Unix seconds when the lease was taken or last heartbeat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<u64>,
    /// Fresh-agent restarts so far.
    #[serde(default)]
    pub restarts: u32,
    /// Review rounds so far.
    #[serde(default)]
    pub round: u32,
}

impl Lease {
    /// Reads the marker from an issue body. A missing or malformed marker is a fresh lease.
    pub fn parse(body: &str) -> Lease {
        let Some(start) = body.find(MARKER_OPEN) else {
            return Lease::default();
        };
        let rest = &body[start + MARKER_OPEN.len()..];
        let Some(end) = rest.find(MARKER_CLOSE) else {
            return Lease::default();
        };
        serde_json::from_str(&rest[..end]).unwrap_or_default()
    }

    /// Returns `body` with this lease's marker replacing the old one (or prepended).
    pub fn write_into(&self, body: &str) -> String {
        let marker = format!(
            "{MARKER_OPEN}{}{MARKER_CLOSE}",
            serde_json::to_string(self).expect("lease")
        );
        if let Some(start) = body.find(MARKER_OPEN)
            && let Some(len) = body[start + MARKER_OPEN.len()..].find(MARKER_CLOSE)
        {
            let end = start + MARKER_OPEN.len() + len + MARKER_CLOSE.len();
            return format!("{}{marker}{}", &body[..start], &body[end..]);
        }
        format!("{marker}\n{body}")
    }
}

/// Knobs for one tick.
#[derive(Debug, Clone, Copy)]
pub struct Config {
    /// Implementer slots (10-15 per the agreed workflow).
    pub slots: usize,
    /// Lease time-to-live in seconds (4 h per the plan).
    pub lease_ttl: u64,
    /// Current unix time.
    pub now: u64,
}

/// Why a spawn happens.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    /// First (or next) increment of a WP.
    Implement,
    /// Address review feedback on an open PR.
    Revise,
}

/// One thing the workflow must do. Serialised with an `action` tag.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "action", rename_all = "kebab-case")]
pub enum Action {
    /// Create the tracking issue for a scheduled WP.
    OpenIssue {
        /// WP id.
        wp: String,
        /// Issue title.
        title: String,
        /// Labels to create and apply.
        labels: Vec<String>,
        /// Issue body (includes a fresh lease marker).
        body: String,
    },
    /// Start an agent on a WP. The workflow labels the issue `status:in-progress` and
    /// writes `body` (with the new lease) before starting the agent.
    Spawn {
        /// WP id.
        wp: String,
        /// Tracking issue.
        issue: u64,
        /// Agent role prompt to use.
        role: Role,
        /// New work or a revision.
        mode: Mode,
        /// The PR being revised.
        #[serde(skip_serializing_if = "Option::is_none")]
        pr: Option<u64>,
        /// New issue body.
        body: String,
    },
    /// A lease expired without a heartbeat: put the WP back in the ready pool.
    Release {
        /// WP id.
        wp: String,
        /// Tracking issue.
        issue: u64,
        /// New issue body (restarts incremented).
        body: String,
        /// Why.
        reason: String,
    },
    /// Hand to a human: label `needs-human` and comment (master plan §12.9).
    Escalate {
        /// WP id.
        wp: String,
        /// Tracking issue.
        issue: u64,
        /// Catalogued reason code.
        code: String,
        /// Explanation posted as a comment.
        reason: String,
    },
}

/// Runs one scheduling tick.
pub fn tick(board: &Board, issues: &[Issue], prs: &[PullRequest], cfg: Config) -> Vec<Action> {
    let mut actions = Vec::new();
    let scheduled = board.scheduled();

    let mut issue_of: BTreeMap<String, &Issue> = BTreeMap::new();
    for issue in issues {
        if let Some(id) = wp_of_labels(&issue.labels) {
            // Prefer an open issue if a WP somehow has several.
            let keep = issue_of.get(&id).is_some_and(|i| i.state == "OPEN");
            if !keep {
                issue_of.insert(id, issue);
            }
        }
    }
    let mut open_prs: BTreeMap<String, Vec<&PullRequest>> = BTreeMap::new();
    for pr in prs {
        if let Some(id) = pr.wp() {
            open_prs.entry(id).or_default().push(pr);
        }
    }

    let done: BTreeSet<String> = board
        .packages
        .values()
        .filter(|wp| {
            wp.status == SpecStatus::Done
                || issue_of.get(&wp.id).is_some_and(|i| i.state == "CLOSED")
        })
        .map(|wp| wp.id.clone())
        .collect();

    // 1. Issues for scheduled WPs that do not have one.
    for id in &scheduled {
        let Some(wp) = board.packages.get(id) else {
            continue;
        };
        if done.contains(id) || issue_of.contains_key(id) {
            continue;
        }
        let wave = board.wave_of(id);
        let mut labels = vec![
            format!("{WP_LABEL}{id}"),
            "status:ready".to_string(),
            format!("role:{}", role_name(wp.role)),
        ];
        if let Some(w) = wave {
            labels.push(format!("phase:{}", w.phase));
            labels.push(format!("wave:{}", w.id));
        }
        labels.extend(wp.risk.iter().map(|r| format!("risk:{r}")));
        let body = Lease::default().write_into(&format!(
            "Work package **{id}**: {}\n\n- Spec: `{}`\n- Definition: `work/packages/{id}.toml`\n- Depends on: {}\n- Acceptance: `{}`\n\nThis issue is managed by the conductor. Do not edit the marker line above.",
            wp.title,
            wp.spec,
            if wp.depends_on.is_empty() { "nothing".to_string() } else { wp.depends_on.join(", ") },
            wp.acceptance,
        ));
        actions.push(Action::OpenIssue {
            wp: id.clone(),
            title: format!("[{id}] {}", wp.title),
            labels,
            body,
        });
    }

    // 2. Expired leases.
    let mut busy = 0usize;
    let mut released = BTreeSet::new();
    for (id, issue) in &issue_of {
        if issue.state != "OPEN" || !has_label(&issue.labels, "status:in-progress") {
            continue;
        }
        let lease = Lease::parse(&issue.body);
        let expired = lease.at.is_none_or(|at| at + cfg.lease_ttl < cfg.now);
        if !expired {
            busy += 1;
            continue;
        }
        let max = board
            .packages
            .get(id)
            .map_or(2, |wp| wp.budget.max_restarts);
        let next = Lease {
            agent: None,
            at: None,
            restarts: lease.restarts + 1,
            round: lease.round,
        };
        if next.restarts > max {
            actions.push(Action::Escalate {
                wp: id.clone(),
                issue: issue.number,
                code: "SYRP-COND-0001".into(),
                reason: format!(
                    "{id} lost its lease {} times (limit {max}). It needs re-planning or a person.",
                    next.restarts
                ),
            });
        } else {
            actions.push(Action::Release {
                wp: id.clone(),
                issue: issue.number,
                body: next.write_into(&issue.body),
                reason: "lease expired without a heartbeat".into(),
            });
        }
        released.insert(id.clone());
    }

    // 3. Candidates for new agents.
    struct Candidate<'a> {
        id: String,
        issue: &'a Issue,
        mode: Mode,
        pr: Option<u64>,
        lease: Lease,
    }
    let mut candidates: Vec<Candidate<'_>> = Vec::new();
    for (id, issue) in &issue_of {
        let Some(wp) = board.packages.get(id) else {
            continue;
        };
        if issue.state != "OPEN"
            || done.contains(id)
            || !scheduled.contains(id)
            || released.contains(id)
            || has_label(&issue.labels, "needs-human")
            || has_label(&issue.labels, "status:in-progress")
        {
            continue;
        }
        let lease = Lease::parse(&issue.body);
        let prs_here = open_prs.get(id).map(Vec::as_slice).unwrap_or_default();
        if let Some(pr) = prs_here
            .iter()
            .find(|p| has_label(&p.labels, "review:changes-requested"))
        {
            if lease.round >= wp.budget.max_review_rounds {
                actions.push(Action::Escalate {
                    wp: id.clone(),
                    issue: issue.number,
                    code: "SYRP-COND-0002".into(),
                    reason: format!(
                        "{id}: PR #{} still has changes requested after {} review rounds.",
                        pr.number, lease.round
                    ),
                });
            } else {
                candidates.push(Candidate {
                    id: id.clone(),
                    issue,
                    mode: Mode::Revise,
                    pr: Some(pr.number),
                    lease,
                });
            }
            continue;
        }
        if !prs_here.is_empty() {
            continue; // waiting on review or merge
        }
        if !wp.depends_on.iter().all(|d| done.contains(d)) {
            continue;
        }
        if matches!(wp.role, Role::Human | Role::Oracle) {
            actions.push(Action::Escalate {
                wp: id.clone(),
                issue: issue.number,
                code: "SYRP-COND-0003".into(),
                reason: format!(
                    "{id} is ready but its role is `{}`, which the pipeline cannot run.",
                    role_name(wp.role)
                ),
            });
            continue;
        }
        candidates.push(Candidate {
            id: id.clone(),
            issue,
            mode: Mode::Implement,
            pr: None,
            lease,
        });
    }

    // Revisions first (they unblock merges), then critical path, then risk, then id.
    candidates.sort_by(|a, b| {
        let key = |c: &Candidate<'_>| {
            let wp = &board.packages[&c.id];
            (
                c.mode != Mode::Revise,
                std::cmp::Reverse(board.dependents(&c.id)),
                std::cmp::Reverse(wp.risk.len()),
                c.id.clone(),
            )
        };
        key(a).cmp(&key(b))
    });

    let free = cfg.slots.saturating_sub(busy);
    for c in candidates.into_iter().take(free) {
        let wp = &board.packages[&c.id];
        let lease = Lease {
            agent: None,
            at: Some(cfg.now),
            restarts: c.lease.restarts,
            round: c.lease.round + u32::from(c.mode == Mode::Revise),
        };
        actions.push(Action::Spawn {
            wp: c.id.clone(),
            issue: c.issue.number,
            role: wp.role,
            mode: c.mode,
            pr: c.pr,
            body: lease.write_into(&c.issue.body),
        });
    }
    actions
}

/// The label spelling of a role.
pub fn role_name(role: Role) -> &'static str {
    match role {
        Role::Implementer => "implementer",
        Role::SpecRoom => "spec-room",
        Role::PrManager => "pr-manager",
        Role::Oracle => "oracle",
        Role::Human => "human",
    }
}

/// After an agent run ends: the new issue body and status label. A run that produced no
/// open PR counts as a restart.
pub fn finish(issue: &Issue, has_open_pr: bool) -> (String, &'static str) {
    let lease = Lease::parse(&issue.body);
    let next = Lease {
        agent: None,
        at: None,
        restarts: lease.restarts + u32::from(!has_open_pr),
        round: lease.round,
    };
    (
        next.write_into(&issue.body),
        if has_open_pr {
            "status:in-review"
        } else {
            "status:ready"
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wp::tests::{board, wp};

    fn label(n: &str) -> Label {
        Label { name: n.into() }
    }

    fn issue(n: u64, id: &str, state: &str, extra: &[&str], lease: Lease) -> Issue {
        let mut labels = vec![label(&format!("wp:{id}"))];
        labels.extend(extra.iter().map(|l| label(l)));
        Issue {
            number: n,
            title: id.into(),
            state: state.into(),
            labels,
            body: lease.write_into("body"),
        }
    }

    fn cfg() -> Config {
        Config {
            slots: 12,
            lease_ttl: 100,
            now: 1_000,
        }
    }

    fn spawned(actions: &[Action]) -> Vec<String> {
        actions
            .iter()
            .filter_map(|a| match a {
                Action::Spawn { wp, .. } => Some(wp.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn lease_marker_round_trips_and_replaces() {
        let l = Lease {
            agent: Some("run-1".into()),
            at: Some(5),
            restarts: 1,
            round: 2,
        };
        let body = l.write_into("hello");
        assert_eq!(Lease::parse(&body), l);
        let l2 = Lease {
            restarts: 3,
            ..l.clone()
        };
        let body2 = l2.write_into(&body);
        assert_eq!(Lease::parse(&body2), l2);
        assert_eq!(body2.matches("syrp-lease").count(), 1);
        assert!(body2.ends_with("hello"));
        assert_eq!(Lease::parse("no marker"), Lease::default());
    }

    #[test]
    fn opens_issues_for_scheduled_wps() {
        let b = board(vec![wp("WP-0.1-01", &[])]);
        let a = tick(&b, &[], &[], cfg());
        assert!(matches!(&a[0], Action::OpenIssue { wp, .. } if wp == "WP-0.1-01"));
    }

    #[test]
    fn spawns_only_when_dependencies_are_done() {
        let b = board(vec![wp("A", &[]), wp("B", &["A"])]);
        let issues = vec![
            issue(1, "A", "OPEN", &["status:ready"], Lease::default()),
            issue(2, "B", "OPEN", &["status:ready"], Lease::default()),
        ];
        assert_eq!(spawned(&tick(&b, &issues, &[], cfg())), vec!["A"]);
        let issues = vec![
            issue(1, "A", "CLOSED", &[], Lease::default()),
            issue(2, "B", "OPEN", &["status:ready"], Lease::default()),
        ];
        assert_eq!(spawned(&tick(&b, &issues, &[], cfg())), vec!["B"]);
    }

    #[test]
    fn respects_slots_and_prefers_critical_path() {
        let b = board(vec![
            wp("A", &[]),
            wp("B", &[]),
            wp("C", &["B"]),
            wp("D", &["C"]),
        ]);
        let issues: Vec<_> = ["A", "B"]
            .iter()
            .enumerate()
            .map(|(i, id)| issue(i as u64, id, "OPEN", &["status:ready"], Lease::default()))
            .collect();
        let a = tick(&b, &issues, &[], Config { slots: 1, ..cfg() });
        assert_eq!(spawned(&a), vec!["B"]);
    }

    #[test]
    fn busy_leases_use_slots_and_expired_ones_release() {
        let b = board(vec![wp("A", &[]), wp("B", &[]), wp("C", &[])]);
        let live = Lease {
            at: Some(950),
            ..Default::default()
        };
        let stale = Lease {
            at: Some(10),
            ..Default::default()
        };
        let issues = vec![
            issue(1, "A", "OPEN", &["status:in-progress"], live),
            issue(2, "B", "OPEN", &["status:in-progress"], stale),
            issue(3, "C", "OPEN", &["status:ready"], Lease::default()),
        ];
        let a = tick(&b, &issues, &[], Config { slots: 2, ..cfg() });
        assert!(
            a.iter()
                .any(|x| matches!(x, Action::Release { wp, .. } if wp == "B"))
        );
        assert_eq!(spawned(&a), vec!["C"]);
    }

    #[test]
    fn too_many_restarts_escalates() {
        let b = board(vec![wp("A", &[])]);
        let stale = Lease {
            at: Some(10),
            restarts: 2,
            ..Default::default()
        };
        let issues = vec![issue(1, "A", "OPEN", &["status:in-progress"], stale)];
        let a = tick(&b, &issues, &[], cfg());
        assert!(matches!(&a[0], Action::Escalate { code, .. } if code == "SYRP-COND-0001"));
    }

    #[test]
    fn changes_requested_spawns_revision_then_escalates() {
        let b = board(vec![wp("A", &[])]);
        let pr = PullRequest {
            number: 9,
            is_draft: false,
            labels: vec![label("wp:A"), label("review:changes-requested")],
            head_ref_name: "wp/A".into(),
        };
        let issues = vec![issue(
            1,
            "A",
            "OPEN",
            &["status:in-review"],
            Lease {
                round: 1,
                ..Default::default()
            },
        )];
        let a = tick(&b, &issues, std::slice::from_ref(&pr), cfg());
        match &a[0] {
            Action::Spawn { mode, pr, body, .. } => {
                assert_eq!(*mode, Mode::Revise);
                assert_eq!(*pr, Some(9));
                assert_eq!(Lease::parse(body).round, 2);
            }
            other => panic!("{other:?}"),
        }
        let issues = vec![issue(
            1,
            "A",
            "OPEN",
            &["status:in-review"],
            Lease {
                round: 3,
                ..Default::default()
            },
        )];
        let a = tick(&b, &issues, &[pr], cfg());
        assert!(matches!(&a[0], Action::Escalate { code, .. } if code == "SYRP-COND-0002"));
    }

    #[test]
    fn open_pr_without_feedback_waits() {
        let b = board(vec![wp("A", &[])]);
        let pr = PullRequest {
            number: 9,
            is_draft: false,
            labels: vec![],
            head_ref_name: "wp/A-x-y-slug".into(),
        };
        // Branch-derived id only works for real ids; use a label here.
        let pr = PullRequest {
            labels: vec![label("wp:A")],
            ..pr
        };
        let issues = vec![issue(
            1,
            "A",
            "OPEN",
            &["status:in-review"],
            Lease::default(),
        )];
        assert!(spawned(&tick(&b, &issues, &[pr], cfg())).is_empty());
    }

    #[test]
    fn human_roles_escalate_instead_of_spawning() {
        let mut w = wp("A", &[]);
        w.role = Role::Human;
        let b = board(vec![w]);
        let issues = vec![issue(1, "A", "OPEN", &["status:ready"], Lease::default())];
        let a = tick(&b, &issues, &[], cfg());
        assert!(matches!(&a[0], Action::Escalate { code, .. } if code == "SYRP-COND-0003"));
        let issues = vec![issue(
            1,
            "A",
            "OPEN",
            &["status:ready", "needs-human"],
            Lease::default(),
        )];
        assert!(tick(&b, &issues, &[], cfg()).is_empty());
    }

    #[test]
    fn branch_names_carry_wp_ids() {
        let pr = PullRequest {
            number: 1,
            is_draft: false,
            labels: vec![],
            head_ref_name: "wp/WP-0.1-03-error-codes".into(),
        };
        assert_eq!(pr.wp().as_deref(), Some("WP-0.1-03"));
    }

    #[test]
    fn finish_counts_failed_runs_as_restarts() {
        let i = issue(
            1,
            "A",
            "OPEN",
            &["status:in-progress"],
            Lease {
                at: Some(5),
                ..Default::default()
            },
        );
        let (body, status) = finish(&i, false);
        assert_eq!(status, "status:ready");
        assert_eq!(Lease::parse(&body).restarts, 1);
        let (body, status) = finish(&i, true);
        assert_eq!(status, "status:in-review");
        assert_eq!(Lease::parse(&body).restarts, 0);
    }
}

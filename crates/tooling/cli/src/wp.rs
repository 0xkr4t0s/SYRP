//! Work packages: loading `work/packages/*.toml` and `work/waves/*.toml`, validating the
//! dependency graph, and path ownership (master plan §12.2 and §12.4).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::envelope::Diagnostic;
use crate::repo;

/// Which kind of agent (or person) does a WP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    /// An implementer agent (the default).
    #[default]
    Implementer,
    /// A spec-room agent: specs, scenarios, ledger inventory. Never reads JUCE source.
    SpecRoom,
    /// The PR manager: shared files and infrastructure.
    PrManager,
    /// An oracle agent working in the separate `juce-oracle` repository.
    Oracle,
    /// Needs a person (accounts, legal, hardware). The conductor escalates instead of spawning.
    Human,
}

/// Lifecycle as recorded in the spec file. Live status (in progress, in review) lives on the
/// WP's GitHub issue; this field only records work finished outside the pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SpecStatus {
    /// Not started, or tracked by its issue.
    #[default]
    Planned,
    /// Done (for example by the bootstrap). Treated as merged.
    Done,
}

/// Per-WP limits enforced by the conductor.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Budget {
    /// Agent-hours before the conductor throttles.
    pub agent_hours: u32,
    /// Review rounds before escalation.
    pub max_review_rounds: u32,
    /// Fresh-agent restarts before re-planning and escalation.
    pub max_restarts: u32,
}

impl Default for Budget {
    fn default() -> Self {
        Budget {
            agent_hours: 24,
            max_review_rounds: 3,
            max_restarts: 2,
        }
    }
}

/// One work package, as written in `work/packages/<id>.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WorkPackage {
    /// `WP-<phase>.<wave>-<nn>`.
    pub id: String,
    /// One line.
    pub title: String,
    /// The crate directory this WP owns, if it owns one.
    #[serde(default, rename = "crate")]
    pub crate_path: Option<String>,
    /// Extra owned paths (a trailing `/` means a directory prefix).
    #[serde(default)]
    pub paths: Vec<String>,
    /// WPs that must be merged first.
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Frozen contract snapshots this WP builds against.
    #[serde(default)]
    pub contracts: Vec<String>,
    /// Parity-ledger rows this WP moves.
    #[serde(default)]
    pub ledger_rows: Vec<String>,
    /// Behaviour spec, relative to the repo root.
    pub spec: String,
    /// Parity scenario globs.
    #[serde(default)]
    pub scenarios: Vec<String>,
    /// Tolerance profile name.
    #[serde(default)]
    pub tolerance: Option<String>,
    /// Risk categories. Any entry triggers the adversarial verifier (gate G10).
    #[serde(default)]
    pub risk: Vec<String>,
    /// The acceptance command, identical for implementer and reviewer.
    pub acceptance: String,
    /// Platforms the acceptance must pass on.
    #[serde(default)]
    pub platforms: Vec<String>,
    /// Limits.
    #[serde(default)]
    pub budget: Budget,
    /// Who does it.
    #[serde(default)]
    pub role: Role,
    /// Done outside the pipeline?
    #[serde(default)]
    pub status: SpecStatus,
}

impl WorkPackage {
    /// Every path prefix this WP may edit.
    pub fn owned_paths(&self) -> Vec<String> {
        let mut v: Vec<String> = self.paths.clone();
        if let Some(c) = &self.crate_path {
            v.push(format!("{}/", c.trim_end_matches('/')));
        }
        v
    }
}

/// A wave: a named group of WPs that are scheduled together.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Wave {
    /// For example `0.1`.
    pub id: String,
    /// Phase number.
    pub phase: u32,
    /// Human title.
    pub title: String,
    /// Disabled waves are never scheduled.
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Member WP ids.
    pub packages: Vec<String>,
}

fn yes() -> bool {
    true
}

/// Everything under `work/`.
#[derive(Debug, Clone, Default)]
pub struct Board {
    /// By id, sorted.
    pub packages: BTreeMap<String, WorkPackage>,
    /// Sorted by id.
    pub waves: Vec<Wave>,
}

impl Board {
    /// Loads and parses every spec. Parse failures become diagnostics; the rest still load.
    pub fn load(root: &Path) -> (Board, Vec<Diagnostic>) {
        let mut board = Board::default();
        let mut diags = Vec::new();
        for path in repo::files_with_suffix(&root.join("work/packages"), ".toml") {
            let rel = repo::rel(root, &path);
            match fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|s| toml::from_str::<WorkPackage>(&s).map_err(|e| e.to_string()))
            {
                Ok(wp) => {
                    let stem = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or_default();
                    if stem != wp.id {
                        diags.push(
                            Diagnostic::error(
                                "AUR-WP-0001",
                                format!("file name {stem} does not match id {}", wp.id),
                            )
                            .at(&rel),
                        );
                    }
                    board.packages.insert(wp.id.clone(), wp);
                }
                Err(e) => diags.push(Diagnostic::error("AUR-WP-0001", e).at(&rel)),
            }
        }
        for path in repo::files_with_suffix(&root.join("work/waves"), ".toml") {
            let rel = repo::rel(root, &path);
            match fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|s| toml::from_str::<Wave>(&s).map_err(|e| e.to_string()))
            {
                Ok(w) => board.waves.push(w),
                Err(e) => diags.push(Diagnostic::error("AUR-WP-0001", e).at(&rel)),
            }
        }
        board.waves.sort_by(|a, b| a.id.cmp(&b.id));
        (board, diags)
    }

    /// Graph and consistency checks: unknown ids, cycles, overlapping ownership, missing specs.
    pub fn validate(&self, root: &Path) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        for wp in self.packages.values() {
            for dep in &wp.depends_on {
                if !self.packages.contains_key(dep) {
                    diags.push(
                        Diagnostic::error(
                            "AUR-WP-0003",
                            format!("{} depends on unknown {dep}", wp.id),
                        )
                        .at(format!("work/packages/{}.toml", wp.id)),
                    );
                }
            }
            if !root.join(&wp.spec).is_file() {
                diags.push(
                    Diagnostic::error(
                        "AUR-WP-0007",
                        format!("{}: spec {} is missing", wp.id, wp.spec),
                    )
                    .at(format!("work/packages/{}.toml", wp.id)),
                );
            }
        }
        if let Some(cycle) = self.find_cycle() {
            diags.push(Diagnostic::error(
                "AUR-WP-0004",
                format!("dependency cycle: {}", cycle.join(" -> ")),
            ));
        }
        // Ownership must not overlap, or two agents could edit the same files at once. A WP that
        // (transitively) depends on another may take over its paths: they never run together.
        let owned: Vec<(String, String)> = self
            .packages
            .values()
            .flat_map(|wp| {
                wp.owned_paths()
                    .into_iter()
                    .map(move |p| (wp.id.clone(), p))
            })
            .collect();
        for (i, (a_id, a)) in owned.iter().enumerate() {
            for (b_id, b) in &owned[i + 1..] {
                if a_id != b_id
                    && (path_matches(a, b) || path_matches(b, a))
                    && !self.depends_on_transitively(a_id, b_id)
                    && !self.depends_on_transitively(b_id, a_id)
                {
                    diags.push(Diagnostic::error(
                        "AUR-WP-0006",
                        format!("{a_id} and {b_id} both own {}", a.min(b)),
                    ));
                }
            }
        }
        for wave in &self.waves {
            for id in &wave.packages {
                if !self.packages.contains_key(id) {
                    diags.push(
                        Diagnostic::error(
                            "AUR-WP-0002",
                            format!("wave {} lists unknown {id}", wave.id),
                        )
                        .at(format!("work/waves/{}.toml", wave.id)),
                    );
                }
            }
        }
        diags
    }

    fn find_cycle(&self) -> Option<Vec<String>> {
        #[derive(Clone, Copy, PartialEq)]
        enum Mark {
            Visiting,
            Done,
        }
        fn visit(
            board: &Board,
            id: &str,
            marks: &mut BTreeMap<String, Mark>,
            stack: &mut Vec<String>,
        ) -> Option<Vec<String>> {
            match marks.get(id) {
                Some(Mark::Done) => return None,
                Some(Mark::Visiting) => {
                    let start = stack.iter().position(|s| s == id).unwrap_or(0);
                    let mut cycle = stack[start..].to_vec();
                    cycle.push(id.to_string());
                    return Some(cycle);
                }
                None => {}
            }
            marks.insert(id.to_string(), Mark::Visiting);
            stack.push(id.to_string());
            if let Some(wp) = board.packages.get(id) {
                for dep in &wp.depends_on {
                    if let Some(c) = visit(board, dep, marks, stack) {
                        return Some(c);
                    }
                }
            }
            stack.pop();
            marks.insert(id.to_string(), Mark::Done);
            None
        }
        let mut marks = BTreeMap::new();
        for id in self.packages.keys() {
            if let Some(c) = visit(self, id, &mut marks, &mut Vec::new()) {
                return Some(c);
            }
        }
        None
    }

    /// True if `a` waits (directly or indirectly) on `b`.
    pub fn depends_on_transitively(&self, a: &str, b: &str) -> bool {
        let mut seen = BTreeSet::new();
        let mut frontier = vec![a.to_string()];
        while let Some(cur) = frontier.pop() {
            let Some(wp) = self.packages.get(&cur) else {
                continue;
            };
            for d in &wp.depends_on {
                if d == b {
                    return true;
                }
                if seen.insert(d.clone()) {
                    frontier.push(d.clone());
                }
            }
        }
        false
    }

    /// Ids of WPs in enabled waves.
    pub fn scheduled(&self) -> BTreeSet<String> {
        self.waves
            .iter()
            .filter(|w| w.enabled)
            .flat_map(|w| w.packages.iter().cloned())
            .collect()
    }

    /// The wave a WP belongs to.
    pub fn wave_of(&self, id: &str) -> Option<&Wave> {
        self.waves
            .iter()
            .find(|w| w.packages.iter().any(|p| p == id))
    }

    /// How many WPs (transitively) wait on `id`. Used to put critical-path work first.
    pub fn dependents(&self, id: &str) -> usize {
        let mut seen = BTreeSet::new();
        let mut frontier = vec![id.to_string()];
        while let Some(cur) = frontier.pop() {
            for wp in self.packages.values() {
                if wp.depends_on.contains(&cur) && seen.insert(wp.id.clone()) {
                    frontier.push(wp.id.clone());
                }
            }
        }
        seen.len()
    }
}

/// True if `path` is `pattern`, or lies under it when `pattern` ends in `/`.
pub fn path_matches(pattern: &str, path: &str) -> bool {
    if pattern.ends_with('/') {
        path.starts_with(pattern)
    } else {
        path == pattern
    }
}

/// Role-owned and any-WP paths from `ownership.toml`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Ownership {
    /// Role name to owned paths.
    #[serde(default)]
    pub roles: BTreeMap<String, RolePaths>,
    /// Paths every WP may touch.
    #[serde(default, rename = "any-wp")]
    pub any_wp: RolePaths,
}

/// A role's paths.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RolePaths {
    /// Why the role exists.
    #[serde(default)]
    pub description: String,
    /// Owned path patterns.
    #[serde(default)]
    pub paths: Vec<String>,
}

impl Ownership {
    /// Reads `ownership.toml`.
    pub fn load(root: &Path) -> Result<Ownership, Diagnostic> {
        let src = fs::read_to_string(root.join("ownership.toml"))
            .map_err(|e| Diagnostic::error("AUR-WP-0001", format!("ownership.toml: {e}")))?;
        toml::from_str(&src)
            .map_err(|e| Diagnostic::error("AUR-WP-0001", format!("ownership.toml: {e}")))
    }

    /// Checks that a change set stays inside what `wp` may edit. Implementer WPs may touch
    /// their owned paths plus the any-WP paths; WPs done by a role also get that role's paths.
    pub fn check_paths(&self, wp: &WorkPackage, files: &[String]) -> Vec<Diagnostic> {
        let mut allowed = wp.owned_paths();
        allowed.extend(self.any_wp.paths.iter().cloned());
        let role_key = match wp.role {
            Role::PrManager => Some("pr-manager"),
            Role::SpecRoom => Some("spec-room"),
            _ => None,
        };
        if let Some(k) = role_key
            && let Some(r) = self.roles.get(k)
        {
            allowed.extend(r.paths.iter().cloned());
        }
        files
            .iter()
            .filter(|f| !allowed.iter().any(|p| path_matches(p, f)))
            .map(|f| {
                let owner = self
                    .roles
                    .iter()
                    .find(|(_, r)| r.paths.iter().any(|p| path_matches(p, f)))
                    .map(|(name, _)| format!(" (owned by role {name})"))
                    .unwrap_or_default();
                Diagnostic::error("AUR-WP-0005", format!("{} may not edit {f}{owner}", wp.id))
                    .at(f.clone())
                    .hint("file a change request issue for shared files instead of editing them")
            })
            .collect()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn wp(id: &str, deps: &[&str]) -> WorkPackage {
        WorkPackage {
            id: id.into(),
            title: id.into(),
            crate_path: Some(format!("crates/x/{id}")),
            paths: vec![],
            depends_on: deps.iter().map(|s| s.to_string()).collect(),
            contracts: vec![],
            ledger_rows: vec![],
            spec: "work/specs/x.md".into(),
            scenarios: vec![],
            tolerance: None,
            risk: vec![],
            acceptance: "true".into(),
            platforms: vec![],
            budget: Budget::default(),
            role: Role::Implementer,
            status: SpecStatus::Planned,
        }
    }

    pub(crate) fn board(wps: Vec<WorkPackage>) -> Board {
        let ids = wps.iter().map(|w| w.id.clone()).collect();
        Board {
            packages: wps.into_iter().map(|w| (w.id.clone(), w)).collect(),
            waves: vec![Wave {
                id: "0.1".into(),
                phase: 0,
                title: "t".into(),
                enabled: true,
                packages: ids,
            }],
        }
    }

    #[test]
    fn detects_cycles() {
        let b = board(vec![wp("A", &["B"]), wp("B", &["C"]), wp("C", &["A"])]);
        let c = b.find_cycle().expect("cycle");
        assert_eq!(c.first(), c.last());
        assert!(
            board(vec![wp("A", &["B"]), wp("B", &[])])
                .find_cycle()
                .is_none()
        );
    }

    #[test]
    fn counts_transitive_dependents() {
        let b = board(vec![
            wp("A", &[]),
            wp("B", &["A"]),
            wp("C", &["B"]),
            wp("D", &[]),
        ]);
        assert_eq!(b.dependents("A"), 2);
        assert_eq!(b.dependents("D"), 0);
    }

    #[test]
    fn ownership_allows_own_crate_and_rejects_shared_files() {
        let own: Ownership = toml::from_str(
            r#"
            [roles.pr-manager]
            paths = ["Cargo.toml", ".github/"]
            [any-wp]
            paths = ["changes/"]
            "#,
        )
        .unwrap();
        let w = wp("A", &[]);
        let ok = own.check_paths(
            &w,
            &["crates/x/A/src/lib.rs".into(), "changes/a.json".into()],
        );
        assert!(ok.is_empty());
        let bad = own.check_paths(&w, &["Cargo.toml".into(), "crates/x/AB/lib.rs".into()]);
        assert_eq!(bad.len(), 2);
        assert!(bad[0].message.contains("pr-manager"));
    }

    #[test]
    fn sequential_wps_may_share_a_crate() {
        let mut a = wp("A", &[]);
        let mut b = wp("B", &["A"]);
        let mut c = wp("C", &[]);
        for w in [&mut a, &mut b, &mut c] {
            w.crate_path = Some("crates/x/shared".into());
        }
        let diags = board(vec![a, b, c]).validate(Path::new("/nonexistent"));
        let overlaps: Vec<_> = diags.iter().filter(|d| d.code == "AUR-WP-0006").collect();
        // A-B is fine (B depends on A); A-C and B-C collide.
        assert_eq!(overlaps.len(), 2, "{overlaps:#?}");
    }

    #[test]
    fn path_prefixes_need_a_slash() {
        assert!(path_matches("crates/a/", "crates/a/x.rs"));
        assert!(!path_matches("crates/a", "crates/ab"));
        assert!(path_matches("Cargo.toml", "Cargo.toml"));
    }
}

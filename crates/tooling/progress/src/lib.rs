// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Builds the Syrp progress page (WP-0.2-16).
//!
//! [`load`] reads the work-package board (`work/`), the JUCE class inventory and parity ledger
//! (`parity/`) and the first-parent git history, and returns a [`Progress`] snapshot.
//! [`render_html`] turns that snapshot into one self-contained HTML page, and the
//! `syrp-progress` binary writes both `progress.json` and `index.html`.
//!
//! ```no_run
//! let progress = syrp_progress::load(std::path::Path::new("."), &[]).unwrap();
//! let html = syrp_progress::render_html(&progress);
//! assert!(html.starts_with("<!doctype html>"));
//! ```

mod error;
mod history;
mod parity;

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub use error::ProgressError;
pub use history::{MergedPr, parse_history};
pub use parity::{ClassStatus, Module};
pub use syrp_error::Diagnostic;

/// The page template. `/*DATA*/` is replaced by the snapshot as JSON.
const TEMPLATE: &str = include_str!("page.html");

/// Everything the page shows, as of one commit.
///
/// ```
/// let p = syrp_progress::Progress::default();
/// assert!(p.waves.is_empty() && p.modules.is_empty() && p.prs.is_empty());
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Progress {
    /// Short hash of the commit the snapshot was taken at.
    pub commit: String,
    /// Waves in id order, each with its work packages in board order.
    pub waves: Vec<Wave>,
    /// JUCE modules in inventory order.
    pub modules: Vec<Module>,
    /// Merged PRs, oldest first.
    pub prs: Vec<MergedPr>,
}

/// One wave of the board.
///
/// ```
/// let w = syrp_progress::Wave { id: "0.2".into(), phase: 0, title: "Contracts".into(), enabled: true, wps: vec![] };
/// assert!(w.wps.is_empty());
/// ```
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Wave {
    /// Wave id, for example `0.2`.
    pub id: String,
    /// Roadmap phase the wave belongs to.
    pub phase: u32,
    /// Wave title.
    pub title: String,
    /// Whether the conductor schedules this wave.
    pub enabled: bool,
    /// Its work packages.
    pub wps: Vec<Wp>,
}

/// One work package as the page shows it.
///
/// ```
/// use syrp_progress::{Wp, WpStatus};
/// let wp = Wp { id: "WP-0.2-16".into(), title: "progress".into(), krate: None, status: WpStatus::Planned, prs: vec![], open: vec![], hours: 12 };
/// assert_eq!(wp.status, WpStatus::Planned);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Wp {
    /// WP id.
    pub id: String,
    /// WP title.
    pub title: String,
    /// The crate directory it owns, if any.
    #[serde(rename = "crate")]
    pub krate: Option<String>,
    /// Status shown on the page (spec B1).
    pub status: WpStatus,
    /// Merged PR numbers whose subject names this WP.
    pub prs: Vec<u32>,
    /// Open PR numbers whose title names this WP.
    pub open: Vec<u32>,
    /// Agent-hour budget, which sizes its treemap tile.
    pub hours: u32,
}

/// A work package's shown status.
///
/// ```
/// assert_eq!(serde_json::to_string(&syrp_progress::WpStatus::Active).unwrap(), "\"active\"");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WpStatus {
    /// Marked done on the board.
    Done,
    /// Not done, but code has merged or a PR is open.
    Active,
    /// Not started.
    Planned,
    /// Not started and its wave is disabled.
    Parked,
}

/// An open PR, as `gh pr list --json number,title` prints it.
///
/// ```
/// let prs: Vec<syrp_progress::OpenPr> = serde_json::from_str(r#"[{"number": 7, "title": "x"}]"#).unwrap();
/// assert_eq!(prs[0].number, 7);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct OpenPr {
    /// PR number.
    pub number: u32,
    /// PR title.
    pub title: String,
}

#[derive(Deserialize)]
struct WaveFile {
    id: String,
    #[serde(default)]
    phase: u32,
    title: String,
    #[serde(default = "yes")]
    enabled: bool,
    #[serde(default)]
    packages: Vec<String>,
}

fn yes() -> bool {
    true
}

/// Sort key for a wave id: `1.10` is after `1.9`, which sorting the text would get wrong.
///
/// ```
/// assert!(syrp_progress::wave_key("1.9") < syrp_progress::wave_key("1.10"));
/// assert!(syrp_progress::wave_key("0.2") < syrp_progress::wave_key("1.9"));
/// ```
pub fn wave_key(id: &str) -> (u32, u32) {
    let (major, minor) = id.split_once('.').unwrap_or((id, "0"));
    (
        major.parse().unwrap_or(u32::MAX),
        minor.parse().unwrap_or(u32::MAX),
    )
}

#[derive(Deserialize)]
struct PackageFile {
    id: String,
    title: String,
    #[serde(rename = "crate")]
    krate: Option<String>,
    #[serde(default)]
    status: String,
    #[serde(default)]
    budget: Budget,
}

#[derive(Deserialize, Default)]
struct Budget {
    #[serde(default)]
    agent_hours: u32,
}

/// The `[WP-x.y-nn]` prefix of a PR subject or title, if any.
fn wp_prefix(s: &str) -> Option<&str> {
    let rest = s.strip_prefix('[')?;
    let id = &rest[..rest.find(']')?];
    id.starts_with("WP-").then_some(id)
}

fn read_tomls<T: serde::de::DeserializeOwned>(dir: &Path) -> Result<Vec<T>, ProgressError> {
    let board = |path: &Path, detail: String| ProgressError::Board {
        path: path.to_path_buf(),
        detail,
    };
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| board(dir, e.to_string()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .collect();
    paths.sort();
    paths
        .iter()
        .map(|p| {
            let text = std::fs::read_to_string(p).map_err(|e| board(p, e.to_string()))?;
            toml::from_str(&text).map_err(|e| board(p, e.to_string()))
        })
        .collect()
}

/// Reads the board and resolves each package's shown status (spec B1).
///
/// ```no_run
/// let waves = syrp_progress::load_board(std::path::Path::new("."), &[], &[]).unwrap();
/// println!("{} waves", waves.len());
/// ```
pub fn load_board(
    root: &Path,
    merged: &[MergedPr],
    open: &[OpenPr],
) -> Result<Vec<Wave>, ProgressError> {
    let packages: BTreeMap<String, PackageFile> =
        read_tomls::<PackageFile>(&root.join("work/packages"))?
            .into_iter()
            .map(|p| (p.id.clone(), p))
            .collect();
    let mut waves: Vec<WaveFile> = read_tomls(&root.join("work/waves"))?;
    // File names sort as strings, so 1.10 would come before 1.9: order by the id's numbers.
    waves.sort_by_key(|w| wave_key(&w.id));
    let mut out = Vec::new();
    for w in waves {
        let mut wps = Vec::new();
        for id in &w.packages {
            let p = packages.get(id).ok_or_else(|| ProgressError::Board {
                path: root.join(format!("work/waves/{}.toml", w.id)),
                detail: format!("wave {} lists unknown package {id}", w.id),
            })?;
            let prs: Vec<u32> = merged
                .iter()
                .filter(|m| m.wp.as_deref() == Some(id))
                .map(|m| m.n)
                .collect();
            let open: Vec<u32> = open
                .iter()
                .filter(|o| wp_prefix(&o.title) == Some(id))
                .map(|o| o.number)
                .collect();
            let status = if p.status == "done" {
                WpStatus::Done
            } else if !prs.is_empty() || !open.is_empty() {
                WpStatus::Active
            } else if !w.enabled {
                WpStatus::Parked
            } else {
                WpStatus::Planned
            };
            wps.push(Wp {
                id: p.id.clone(),
                title: p.title.clone(),
                krate: p.krate.clone(),
                status,
                prs,
                open,
                hours: p.budget.agent_hours.max(1),
            });
        }
        out.push(Wave {
            id: w.id,
            phase: w.phase,
            title: w.title,
            enabled: w.enabled,
            wps,
        });
    }
    Ok(out)
}

/// Builds the whole snapshot for the repository at `root`.
///
/// `open` lists open PRs; pass an empty slice when GitHub is not reachable.
///
/// ```no_run
/// let p = syrp_progress::load(std::path::Path::new("."), &[]).unwrap();
/// println!("{} PRs merged at {}", p.prs.len(), p.commit);
/// ```
pub fn load(root: &Path, open: &[OpenPr]) -> Result<Progress, ProgressError> {
    let (commit, prs) = history::read(root)?;
    let waves = load_board(root, &prs, open)?;
    let active: BTreeMap<&str, WpStatus> = waves
        .iter()
        .flat_map(|w| &w.wps)
        .map(|wp| (wp.id.as_str(), wp.status))
        .collect();
    let modules = parity::load(root, |id| {
        matches!(active.get(id), Some(WpStatus::Done | WpStatus::Active))
    })?;
    Ok(Progress {
        commit,
        waves,
        modules,
        prs,
    })
}

/// Renders the snapshot as one self-contained HTML page (spec B4).
///
/// The JSON is embedded in a script element with every `<` written as the JSON escape `\u003c`,
/// so no text from the board or a PR title can close that element or open another tag. The
/// page script escapes those strings again before it puts them into the document.
///
/// ```
/// let p = syrp_progress::Progress { commit: "</script>".into(), ..Default::default() };
/// let html = syrp_progress::render_html(&p);
/// assert!(html.contains(r#""commit":"\u003c/script>""#));
/// ```
pub fn render_html(p: &Progress) -> String {
    let json = serde_json::to_string(p).expect("snapshot always serialises");
    TEMPLATE.replacen("/*DATA*/", &json.replace('<', "\\u003c"), 1)
}

// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! JUCE modules and per-class status from the inventory and ledger (spec B2).

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ProgressError;

/// One JUCE module: its classes, each with a status.
///
/// Cells serialise as `["ClassName", "status"]` pairs to keep the page data small.
///
/// ```
/// use syrp_progress::{ClassStatus, Module};
/// let m = Module { module: "juce_core".into(), complete: true, cells: vec![("Array".into(), ClassStatus::Todo)] };
/// assert_eq!(serde_json::to_string(&m.cells).unwrap(), r#"[["Array","todo"]]"#);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Module {
    /// Module name, for example `juce_core`.
    pub module: String,
    /// Whether the inventory lists every class of the module yet.
    pub complete: bool,
    /// One entry per class, in inventory order.
    pub cells: Vec<(String, ClassStatus)>,
}

/// Where one JUCE class stands.
///
/// ```
/// assert_eq!(serde_json::to_string(&syrp_progress::ClassStatus::Inwork).unwrap(), "\"inwork\"");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ClassStatus {
    /// Ledger row is `done` or `mapped`: backed by CI evidence.
    Verified,
    /// Ledger row is `partial`, or its owning WP has merged code or an open PR.
    Inwork,
    /// A deliberate gap or deviation with a written reason.
    Gap,
    /// Not started.
    Todo,
}

#[derive(Deserialize)]
struct Inventory {
    modules: Vec<InvModule>,
}

#[derive(Deserialize)]
struct InvModule {
    module: String,
    #[serde(default)]
    complete: bool,
    #[serde(default)]
    classes: Vec<InvClass>,
}

#[derive(Deserialize)]
struct InvClass {
    class: String,
}

#[derive(Deserialize)]
struct Ledger {
    #[serde(default)]
    rows: Vec<Row>,
}

#[derive(Deserialize)]
struct Row {
    juce: RowJuce,
    status: String,
    owner_wp: Option<String>,
}

#[derive(Deserialize)]
struct RowJuce {
    module: String,
    class: String,
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, ProgressError> {
    let err = |detail: String| ProgressError::Parity {
        path: path.to_path_buf(),
        detail,
    };
    let text = std::fs::read_to_string(path).map_err(|e| err(e.to_string()))?;
    serde_json::from_str(&text).map_err(|e| err(e.to_string()))
}

/// Status of one ledger row; `wp_active` says whether a WP has merged code or an open PR.
fn status_of(row: &Row, wp_active: &impl Fn(&str) -> bool) -> ClassStatus {
    match row.status.as_str() {
        "done" | "mapped" => ClassStatus::Verified,
        "gap" | "deviation" => ClassStatus::Gap,
        "partial" => ClassStatus::Inwork,
        _ if row.owner_wp.as_deref().is_some_and(wp_active) => ClassStatus::Inwork,
        _ => ClassStatus::Todo,
    }
}

/// Reads the inventory and the ledger (falling back to the seed while the ledger is empty).
pub(crate) fn load(
    root: &Path,
    wp_active: impl Fn(&str) -> bool,
) -> Result<Vec<Module>, ProgressError> {
    let inv: Inventory = read_json(&root.join("parity/inventory/juce-9.0.4-classes.json"))?;
    let mut ledger: Ledger = read_json(&root.join("parity/ledger.json"))?;
    if ledger.rows.is_empty() {
        let seed = root.join("parity/inventory/ledger-seed.json");
        if seed.exists() {
            ledger = read_json(&seed)?;
        }
    }
    let rows: BTreeMap<(&str, &str), &Row> = ledger
        .rows
        .iter()
        .map(|r| ((r.juce.module.as_str(), r.juce.class.as_str()), r))
        .collect();
    Ok(inv
        .modules
        .iter()
        .map(|m| Module {
            module: m.module.clone(),
            complete: m.complete,
            cells: m
                .classes
                .iter()
                .map(|c| {
                    let st = rows
                        .get(&(m.module.as_str(), c.class.as_str()))
                        .map_or(ClassStatus::Todo, |r| status_of(r, &wp_active));
                    (c.class.clone(), st)
                })
                .collect(),
        })
        .collect())
}

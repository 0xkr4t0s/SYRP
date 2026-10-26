// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Similarity scoring and the JSON report (B2, B4).
//!
//! A file's score is the fraction of its winnowed fingerprints whose hash is in the
//! database: 0.0 means nothing shared, 1.0 means every fingerprint matched. A file fails when
//! its score is strictly above the threshold, which yields a `SYRP-CR-0001` finding.
//!
//! Report JSON (`schema` = [`SCHEMA`]):
//!
//! ```json
//! {
//!   "schema": "syrp.cleanroom-report/v1",
//!   "normaliser": "syrp-cfamily-v1", "k": 12, "w": 8, "threshold": 0.3,
//!   "verdict": "fail",
//!   "files": [
//!     { "path": "crates/x/src/a.rs", "score": 0.5, "matched": 4, "fingerprints": 8,
//!       "verdict": "fail", "matched_lines": [[10, 14]] }
//!   ]
//! }
//! ```
//!
//! `matched_lines` are merged, 1-based, inclusive line ranges of the scanned file only.
//!
//! ```
//! use syrp_provenance::{database::Database, report::scan};
//! let src = "fn mix(a: f32, b: f32) -> f32 { a * 0.5 + b * 0.5 }";
//! let db = Database::from_sources(&[src], 5, 3);
//! let r = scan(&db, &[("copy.rs".into(), src.into())], 0.3);
//! assert!(!r.passed());
//! assert_eq!(r.findings()[0].code, "SYRP-CR-0001");
//! ```

use serde::Serialize;

use crate::Finding;
use crate::database::Database;
use crate::scanner::{NORMALISER, fingerprints};

/// The `schema` value of a version-1 report.
pub const SCHEMA: &str = "syrp.cleanroom-report/v1";

/// Pass or fail.
///
/// ```
/// use syrp_provenance::report::Verdict;
/// assert_eq!(serde_json::to_string(&Verdict::Fail).unwrap(), "\"fail\"");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    /// At or below the threshold.
    Pass,
    /// Above the threshold.
    Fail,
}

/// The result for one scanned file.
///
/// ```
/// use syrp_provenance::{database::Database, report::{score_file, Verdict}};
/// let db = Database::from_sources(&[], 3, 2);
/// let f = score_file(&db, "a.rs", "let x = 1;", 0.1);
/// assert_eq!((f.score, f.verdict), (0.0, Verdict::Pass));
/// ```
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FileReport {
    /// Path as given to the scanner.
    pub path: String,
    /// Matched fingerprints divided by fingerprints (0.0 when the file has none).
    pub score: f64,
    /// Fingerprints found in the database.
    pub matched: usize,
    /// Fingerprints the file produced.
    pub fingerprints: usize,
    /// Threshold verdict for this file.
    pub verdict: Verdict,
    /// Merged inclusive line ranges covered by matched fingerprints.
    pub matched_lines: Vec<[u32; 2]>,
}

/// The whole report.
///
/// ```
/// use syrp_provenance::{database::Database, report::scan};
/// let r = scan(&Database::from_sources(&[], 3, 2), &[], 0.2);
/// assert!(r.to_json().contains("syrp.cleanroom-report/v1"));
/// ```
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Report {
    /// Always [`SCHEMA`].
    pub schema: &'static str,
    /// Normaliser used for fingerprinting.
    pub normaliser: &'static str,
    /// Tokens per n-gram (from the database).
    pub k: usize,
    /// Winnowing window (from the database).
    pub w: usize,
    /// Scores strictly above this fail.
    pub threshold: f64,
    /// Fail if any file failed.
    pub verdict: Verdict,
    /// One entry per scanned file, in input order.
    pub files: Vec<FileReport>,
}

impl Report {
    /// Whether every file passed.
    ///
    /// ```
    /// use syrp_provenance::{database::Database, report::scan};
    /// assert!(scan(&Database::from_sources(&[], 3, 2), &[], 0.2).passed());
    /// ```
    pub fn passed(&self) -> bool {
        self.verdict == Verdict::Pass
    }

    /// One `SYRP-CR-0001` finding per failing file.
    ///
    /// ```
    /// use syrp_provenance::{database::Database, report::scan};
    /// assert!(scan(&Database::from_sources(&[], 3, 2), &[], 0.2).findings().is_empty());
    /// ```
    pub fn findings(&self) -> Vec<Finding> {
        self.files
            .iter()
            .filter(|f| f.verdict == Verdict::Fail)
            .map(|f| {
                Finding::new(
                    "SYRP-CR-0001",
                    f.path.clone(),
                    format!(
                        "similarity {:.3} is above the threshold {:.3} ({} of {} fingerprints matched)",
                        f.score, self.threshold, f.matched, f.fingerprints
                    ),
                )
            })
            .collect()
    }

    /// Pretty JSON in the format described in the module docs.
    ///
    /// ```
    /// use syrp_provenance::{database::Database, report::scan};
    /// let json = scan(&Database::from_sources(&[], 3, 2), &[], 0.2).to_json();
    /// assert!(json.contains("\"verdict\": \"pass\""));
    /// ```
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("report serialises")
    }
}

/// Scores one file against the database.
///
/// ```
/// use syrp_provenance::{database::Database, report::score_file};
/// let db = Database::from_sources(&["a b c d e"], 2, 1);
/// assert_eq!(score_file(&db, "x", "a b c d e", 0.5).score, 1.0);
/// ```
pub fn score_file(db: &Database, path: &str, source: &str, threshold: f64) -> FileReport {
    let fps = fingerprints(source, db.k(), db.w());
    let mut ranges: Vec<[u32; 2]> = Vec::new();
    let mut matched = 0;
    for f in fps.iter().filter(|f| db.contains(f.hash)) {
        matched += 1;
        ranges.push([f.first_line, f.last_line]);
    }
    ranges.sort_unstable();
    let mut merged: Vec<[u32; 2]> = Vec::new();
    for r in ranges {
        match merged.last_mut() {
            Some(last) if r[0] <= last[1] + 1 => last[1] = last[1].max(r[1]),
            _ => merged.push(r),
        }
    }
    let score = if fps.is_empty() {
        0.0
    } else {
        matched as f64 / fps.len() as f64
    };
    FileReport {
        path: path.to_owned(),
        score,
        matched,
        fingerprints: fps.len(),
        // A NaN threshold or score is incomparable and fails closed instead of passing.
        verdict: match score.partial_cmp(&threshold) {
            Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal) => Verdict::Pass,
            _ => Verdict::Fail,
        },
        matched_lines: merged,
    }
}

/// Scores every `(path, source)` pair and builds the report.
///
/// ```
/// use syrp_provenance::{database::Database, report::scan};
/// let db = Database::from_sources(&["q w e r t y"], 3, 2);
/// let r = scan(&db, &[("a.rs".into(), "z x c v".into())], 0.25);
/// assert_eq!(r.files.len(), 1);
/// ```
pub fn scan(db: &Database, files: &[(String, String)], threshold: f64) -> Report {
    let files: Vec<FileReport> = files
        .iter()
        .map(|(p, s)| score_file(db, p, s, threshold))
        .collect();
    let verdict = if files.iter().any(|f| f.verdict == Verdict::Fail) {
        Verdict::Fail
    } else {
        Verdict::Pass
    };
    Report {
        schema: SCHEMA,
        normaliser: NORMALISER,
        k: db.k(),
        w: db.w(),
        threshold,
        verdict,
        files,
    }
}

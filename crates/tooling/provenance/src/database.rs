// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The fingerprint database format (B3).
//!
//! A quarantined CI job, the only process allowed to read JUCE source, builds this file and
//! hands it to the scanner. This crate only reads it. The file is UTF-8 JSON:
//!
//! ```json
//! {
//!   "schema": "syrp.cleanroom-fingerprints/v1",
//!   "normaliser": "syrp-cfamily-v1",
//!   "k": 12,
//!   "w": 8,
//!   "hashes": ["00c0ffee00c0ffee", "..."]
//! }
//! ```
//!
//! - `schema` must be exactly [`SCHEMA`].
//! - `normaliser` must equal [`crate::scanner::NORMALISER`]; the producer must tokenise with
//!   the same rules (it may link this crate and call [`Database::from_sources`]).
//! - `k` (tokens per n-gram) and `w` (winnowing window) are positive integers. The scanner
//!   fingerprints files under test with the database's own `k` and `w`.
//! - `hashes` are the winnowed FNV-1a 64 fingerprint hashes, each as exactly 16 lowercase
//!   hex digits (strings, so that no JSON reader rounds them). Order and duplicates do not
//!   matter.
//!
//! The database deliberately holds no source text, file names or line numbers, so it carries
//! no JUCE expression into the clean room.
//!
//! ```
//! use syrp_provenance::database::Database;
//! let json = r#"{"schema":"syrp.cleanroom-fingerprints/v1","normaliser":"syrp-cfamily-v1",
//!               "k":4,"w":2,"hashes":["0000000000000001"]}"#;
//! let db = Database::from_json(json).unwrap();
//! assert!(db.contains(1));
//! ```

use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::Error;
use crate::scanner::{NORMALISER, fingerprints};

/// The `schema` value of a version-1 database.
pub const SCHEMA: &str = "syrp.cleanroom-fingerprints/v1";

#[derive(Serialize, Deserialize)]
struct Wire {
    schema: String,
    normaliser: String,
    k: usize,
    w: usize,
    hashes: Vec<String>,
}

/// A loaded fingerprint database.
///
/// ```
/// let db = syrp_provenance::database::Database::from_sources(&["a b c d"], 2, 1);
/// assert_eq!((db.k(), db.w()), (2, 1));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Database {
    k: usize,
    w: usize,
    hashes: HashSet<u64>,
}

impl Database {
    /// Builds a database by fingerprinting `sources` with `k` and `w`. Tests use it with
    /// synthetic corpora; the quarantined producer job may use it too.
    ///
    /// ```
    /// let db = syrp_provenance::database::Database::from_sources(&["x y z"], 3, 1);
    /// assert_eq!(db.len(), 1);
    /// ```
    pub fn from_sources(sources: &[&str], k: usize, w: usize) -> Self {
        let (k, w) = (k.max(1), w.max(1));
        let hashes = sources
            .iter()
            .flat_map(|s| fingerprints(s, k, w))
            .map(|f| f.hash)
            .collect();
        Database { k, w, hashes }
    }

    /// Parses and validates the JSON format described in the module docs.
    ///
    /// ```
    /// let err = syrp_provenance::database::Database::from_json("{}").unwrap_err();
    /// assert_eq!(err.code(), "SYRP-CR-0002");
    /// ```
    pub fn from_json(text: &str) -> Result<Self, Error> {
        let wire: Wire = serde_json::from_str(text)
            .map_err(|e| Error::Database(format!("not a fingerprint database: {e}")))?;
        if wire.schema != SCHEMA {
            return Err(Error::Database(format!(
                "schema is {:?}, expected {SCHEMA:?}",
                wire.schema
            )));
        }
        if wire.normaliser != NORMALISER {
            return Err(Error::Database(format!(
                "normaliser is {:?}, this scanner uses {NORMALISER:?}",
                wire.normaliser
            )));
        }
        if wire.k == 0 || wire.w == 0 {
            return Err(Error::Database("k and w must be at least 1".into()));
        }
        let mut hashes = HashSet::with_capacity(wire.hashes.len());
        for h in &wire.hashes {
            let ok = h.len() == 16 && h.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
            let v = u64::from_str_radix(h, 16).ok().filter(|_| ok);
            hashes.insert(v.ok_or_else(|| Error::Database(format!("bad hash {h:?}")))?);
        }
        Ok(Database {
            k: wire.k,
            w: wire.w,
            hashes,
        })
    }

    /// Reads and parses a database file.
    ///
    /// ```
    /// let err = syrp_provenance::database::Database::load("/no/such/file".as_ref());
    /// assert_eq!(err.unwrap_err().code(), "SYRP-CR-0002");
    /// ```
    pub fn load(path: &Path) -> Result<Self, Error> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| Error::Database(format!("cannot read {}: {e}", path.display())))?;
        Self::from_json(&text)
    }

    /// Serialises to the JSON format, with hashes sorted so output is reproducible.
    ///
    /// ```
    /// use syrp_provenance::database::Database;
    /// let db = Database::from_sources(&["p q r s"], 2, 2);
    /// assert_eq!(Database::from_json(&db.to_json()).unwrap(), db);
    /// ```
    pub fn to_json(&self) -> String {
        let mut sorted: Vec<u64> = self.hashes.iter().copied().collect();
        sorted.sort_unstable();
        let wire = Wire {
            schema: SCHEMA.into(),
            normaliser: NORMALISER.into(),
            k: self.k,
            w: self.w,
            hashes: sorted.iter().map(|h| format!("{h:016x}")).collect(),
        };
        serde_json::to_string_pretty(&wire).expect("database serialises")
    }

    /// Tokens per n-gram.
    ///
    /// ```
    /// assert_eq!(syrp_provenance::database::Database::from_sources(&[], 7, 3).k(), 7);
    /// ```
    pub fn k(&self) -> usize {
        self.k
    }

    /// Winnowing window size.
    ///
    /// ```
    /// assert_eq!(syrp_provenance::database::Database::from_sources(&[], 7, 3).w(), 3);
    /// ```
    pub fn w(&self) -> usize {
        self.w
    }

    /// Number of distinct hashes.
    ///
    /// ```
    /// assert!(syrp_provenance::database::Database::from_sources(&[], 7, 3).is_empty());
    /// ```
    pub fn len(&self) -> usize {
        self.hashes.len()
    }

    /// Whether the database holds no hashes.
    ///
    /// ```
    /// assert!(syrp_provenance::database::Database::from_sources(&[""], 1, 1).is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.hashes.is_empty()
    }

    /// Whether `hash` is in the database.
    ///
    /// ```
    /// assert!(!syrp_provenance::database::Database::from_sources(&[], 1, 1).contains(0));
    /// ```
    pub fn contains(&self, hash: u64) -> bool {
        self.hashes.contains(&hash)
    }
}

//! Offline ranking of index entries by name, alias and doc text (B4).

use crate::{ApiEntry, ApiIndex};

/// One search result: the entry and its score (higher is better).
///
/// ```
/// let index = syrp_docgen::ApiIndex::new(Vec::new());
/// let hits: Vec<syrp_docgen::SearchHit<'_>> = syrp_docgen::search(&index, "x", 5);
/// assert!(hits.is_empty());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchHit<'a> {
    /// The matching entry.
    pub entry: &'a ApiEntry,
    /// Sum of the per-term scores.
    pub score: u32,
}

/// Ranks entries against a whitespace-separated query and returns at most `limit` hits.
///
/// Matching is case-insensitive. Every term must match the entry somewhere. Per term, the best
/// of these counts: exact name 100, exact alias 90, name prefix 60, name substring 40, alias
/// substring 35, path substring 20, summary substring 10, signature substring 5. Ties are
/// broken by shorter path, then alphabetically, so results are deterministic.
///
/// ```
/// use syrp_docgen::{ApiEntry, ApiIndex, ItemKind, search};
/// let e = ApiEntry { crate_name: "c".into(), path: "c::Gain".into(), kind: ItemKind::Struct,
///     signature: "pub struct Gain".into(), summary: "A gain stage.".into(),
///     aliases: vec!["juce::dsp::Gain".into()], deprecated: None, source: None };
/// let index = ApiIndex::new(vec![e]);
/// assert_eq!(search(&index, "GAIN", 5)[0].entry.path, "c::Gain");
/// ```
#[doc(alias = "docs search")]
pub fn search<'a>(index: &'a ApiIndex, query: &str, limit: usize) -> Vec<SearchHit<'a>> {
    let terms: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if terms.is_empty() {
        return Vec::new();
    }
    let mut hits: Vec<SearchHit<'a>> = index
        .entries
        .iter()
        .filter_map(|entry| {
            let fields = Fields::of(entry);
            terms
                .iter()
                .map(|t| fields.score(t))
                .try_fold(0u32, |acc, s| (s > 0).then_some(acc + s))
                .map(|score| SearchHit { entry, score })
        })
        .collect();
    hits.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then(a.entry.path.len().cmp(&b.entry.path.len()))
            .then(a.entry.path.cmp(&b.entry.path))
            .then(a.entry.kind.cmp(&b.entry.kind))
    });
    hits.truncate(limit);
    hits
}

/// Lower-cased searchable text of one entry.
struct Fields {
    name: String,
    aliases: Vec<String>,
    path: String,
    summary: String,
    signature: String,
}

impl Fields {
    fn of(e: &ApiEntry) -> Self {
        Self {
            name: e
                .path
                .rsplit("::")
                .next()
                .unwrap_or_default()
                .to_lowercase(),
            aliases: e.aliases.iter().map(|a| a.to_lowercase()).collect(),
            path: e.path.to_lowercase(),
            summary: e.summary.to_lowercase(),
            signature: e.signature.to_lowercase(),
        }
    }

    fn score(&self, term: &str) -> u32 {
        let alias_tail = |a: &String| a.rsplit("::").next().is_some_and(|tail| tail == term);
        if self.name == term {
            100
        } else if self.aliases.iter().any(|a| a == term || alias_tail(a)) {
            90
        } else if self.name.starts_with(term) {
            60
        } else if self.name.contains(term) {
            40
        } else if self.aliases.iter().any(|a| a.contains(term)) {
            35
        } else if self.path.contains(term) {
            20
        } else if self.summary.contains(term) {
            10
        } else if self.signature.contains(term) {
            5
        } else {
            0
        }
    }
}

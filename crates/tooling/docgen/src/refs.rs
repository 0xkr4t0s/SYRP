//! `check-refs` (B4): every Rust path a doc or spec cites must be in the API index.

use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::{ApiIndex, DocError};

/// One cited path that the API index does not contain.
///
/// ```
/// let r = syrp_docgen::UnresolvedRef { file: "a.md".into(), line: 3, path: "syrp_x::Y".into() };
/// assert_eq!(r.to_string(), "a.md:3: syrp_x::Y");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedRef {
    /// The doc or spec, as it was passed in.
    pub file: PathBuf,
    /// 1-based line of the citation.
    pub line: usize,
    /// The cited path, for example `syrp_widgets_slider::Slider`.
    pub path: String,
}

impl fmt::Display for UnresolvedRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.file.display(), self.line, self.path)
    }
}

/// Lists the Rust paths in `text` that the index does not contain.
///
/// A citation is any `a::b` path (identifiers joined by `::`, anywhere in the text, code
/// included) whose first segment is a Syrp crate: `syrp`, a name starting with `syrp_`, or a
/// crate present in the index. Paths rooted elsewhere (`std::fs`, `juce::Slider`, `crate::x`)
/// are not checked. A citation resolves when it is exactly the path of an index entry; a
/// generic argument list (`Gain::<f32>`) ends the path before it. Results are in line order.
///
/// ```
/// use syrp_docgen::{ApiEntry, ApiIndex, ItemKind, find_unresolved_refs};
/// let e = ApiEntry { crate_name: "syrp_core".into(), path: "syrp_core::Gain".into(),
///     kind: ItemKind::Struct, signature: "pub struct Gain".into(), summary: String::new(),
///     aliases: vec![], deprecated: None, source: None };
/// let index = ApiIndex::new(vec![e]);
/// let text = "Use `syrp_core::Gain`, not `syrp_core::Gian` or juce::dsp::Gain.";
/// let bad = find_unresolved_refs(&index, "guide.md".as_ref(), text);
/// assert_eq!(bad.len(), 1);
/// assert_eq!(bad[0].path, "syrp_core::Gian");
/// ```
pub fn find_unresolved_refs(index: &ApiIndex, file: &Path, text: &str) -> Vec<UnresolvedRef> {
    let known: HashSet<&str> = index.entries.iter().map(|e| e.path.as_str()).collect();
    syrp_citations(index, text)
        .into_iter()
        .filter(|(_, path)| !known.contains(path))
        .map(|(line, path)| UnresolvedRef {
            file: file.to_path_buf(),
            line,
            path: path.to_owned(),
        })
        .collect()
}

/// Checks every file in `files` against the index (the `check-refs` gate).
///
/// Returns how many Syrp paths were cited and all resolved. Errors:
/// [`DocError::UnresolvedRefs`] (SYRP-DOC-0006) listing every unresolved citation across all
/// files; [`DocError::SourceRead`] (SYRP-DOC-0007) when a file cannot be read.
///
/// ```
/// use syrp_docgen::{ApiIndex, Diagnostic, check_refs};
/// let err = check_refs(&ApiIndex::new(vec![]), &["no/such/spec.md"]).unwrap_err();
/// assert_eq!(err.code().as_str(), "SYRP-DOC-0007");
/// ```
#[doc(alias = "check-refs")]
pub fn check_refs<P: AsRef<Path>>(index: &ApiIndex, files: &[P]) -> Result<usize, DocError> {
    let mut unresolved = Vec::new();
    let mut cited = 0;
    for file in files {
        let file = file.as_ref();
        let text = std::fs::read_to_string(file).map_err(|e| DocError::SourceRead {
            path: file.to_path_buf(),
            detail: e.to_string(),
        })?;
        cited += syrp_citations(index, &text).len();
        unresolved.extend(find_unresolved_refs(index, file, &text));
    }
    if unresolved.is_empty() {
        Ok(cited)
    } else {
        Err(DocError::UnresolvedRefs { refs: unresolved })
    }
}

/// Every path in `text` rooted at a Syrp crate, with its 1-based line.
fn syrp_citations<'t>(index: &ApiIndex, text: &'t str) -> Vec<(usize, &'t str)> {
    let crates: HashSet<&str> = index
        .entries
        .iter()
        .map(|e| e.crate_name.as_str())
        .collect();
    let is_syrp = |root: &str| root == "syrp" || root.starts_with("syrp_") || crates.contains(root);
    let lines = text.lines().enumerate();
    lines
        .flat_map(|(i, line)| cited_paths(line).map(move |p| (i + 1, p)))
        .filter(|(_, p)| is_syrp(p.split("::").next().unwrap_or_default()))
        .collect()
}

/// Yields every `ident::ident(::ident)*` path in one line, in order.
fn cited_paths(line: &str) -> impl Iterator<Item = &str> {
    let is_word = |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | ':');
    line.split(move |c: char| !is_word(c)).filter_map(|token| {
        // A leading `::` means the token continues a path we cannot see (or is `::std`).
        if token.starts_with(':') {
            return None;
        }
        let token = token.trim_end_matches(':');
        let segments: Vec<&str> = token.split("::").collect();
        let ident = |s: &&str| {
            !s.is_empty() && !s.contains(':') && !s.starts_with(|c: char| c.is_ascii_digit())
        };
        (segments.len() >= 2 && segments.iter().all(ident)).then_some(token)
    })
}

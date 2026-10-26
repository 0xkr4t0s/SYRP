//! The error catalogue: parsing `errors/<domain>.toml` and rendering its docs page.
//!
//! Needs the `catalogue` feature. A catalogue file is a list of `[[error]]` tables with the
//! keys `code`, `title`, `severity` (`error`, `warning` or `info`), `cause`, `fix` and an
//! optional `example`. Every value is a string. Any other key is rejected, so a typo cannot
//! silently drop text.
//!
//! ```
//! use syrp_error::catalogue;
//!
//! let entries = catalogue::parse(r#"
//!     [[error]]
//!     code = "SYRP-RT-0001"
//!     title = "Queue full"
//!     severity = "error"
//!     cause = "The producer outran the consumer."
//!     fix = "Make the queue larger."
//! "#)?;
//! assert_eq!(entries[0].code, "SYRP-RT-0001");
//! let page = catalogue::render_markdown(&entries);
//! assert!(page.starts_with("# RT errors"));
//! # Ok::<(), catalogue::ParseError>(())
//! ```

use std::borrow::Cow;
use std::collections::HashSet;
use std::fmt;
use std::fmt::Write as _;

use crate::{Diagnostic, ErrorCode, Severity};

/// One `[[error]]` entry of a catalogue file.
///
/// ```
/// let entries = syrp_error::catalogue::parse(r#"
///     [[error]]
///     code = "SYRP-RT-0002"
///     title = "t"
///     severity = "warning"
///     cause = "c"
///     fix = "f"
///     example = "e"
/// "#).unwrap();
/// assert_eq!(entries[0].severity, syrp_error::Severity::Warning);
/// assert_eq!(entries[0].example.as_deref(), Some("e"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The code, already checked against `SYRP-<DOMAIN>-<NNNN>`.
    pub code: String,
    /// A short title.
    pub title: String,
    /// How serious the error is.
    pub severity: Severity,
    /// Why it happens.
    pub cause: String,
    /// How to fix it.
    pub fix: String,
    /// An optional worked example.
    pub example: Option<String>,
}

impl Entry {
    /// The domain part of the code, for example `"CLI"`.
    ///
    /// ```
    /// let e = &syrp_error::catalogue::parse(r#"
    ///     [[error]]
    ///     code = "SYRP-CLI-0001"
    ///     title = "t"
    ///     severity = "error"
    ///     cause = "c"
    ///     fix = "f"
    /// "#).unwrap()[0];
    /// assert_eq!(e.domain(), "CLI");
    /// ```
    #[must_use]
    pub fn domain(&self) -> &str {
        // `parse` only builds entries with valid codes, so these offsets are in range.
        &self.code[5..self.code.len() - 5]
    }
}

/// Why a catalogue file was rejected. Implements [`Diagnostic`] with `SYRP-ERROR-*` codes.
///
/// ```
/// use syrp_error::catalogue::{parse, ParseError};
/// use syrp_error::Diagnostic;
///
/// let err = parse("not = [valid").unwrap_err();
/// assert!(matches!(err, ParseError::Toml(_)));
/// assert_eq!(err.code().as_str(), "SYRP-ERROR-0001");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParseError {
    /// The text is not TOML, or its top level is not just an `error` array of tables.
    Toml(String),
    /// An entry lacks a required key, has a non-string value or has an unknown key.
    InvalidEntry {
        /// 1-based position of the entry in the file.
        index: usize,
        /// What is wrong.
        problem: String,
    },
    /// A code is not `SYRP-<DOMAIN>-<NNNN>`.
    MalformedCode(String),
    /// A severity is not `error`, `warning` or `info`.
    UnknownSeverity {
        /// The entry's code.
        code: String,
        /// The rejected value.
        severity: String,
    },
    /// The same code appears twice.
    DuplicateCode(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Toml(msg) => write!(f, "catalogue is not a valid [[error]] list: {msg}"),
            Self::InvalidEntry { index, problem } => write!(f, "entry {index}: {problem}"),
            Self::MalformedCode(code) => write!(f, "malformed error code {code:?}"),
            Self::UnknownSeverity { code, severity } => {
                write!(f, "{code}: unknown severity {severity:?}")
            }
            Self::DuplicateCode(code) => write!(f, "duplicate error code {code}"),
        }
    }
}

impl std::error::Error for ParseError {}

impl Diagnostic for ParseError {
    fn code(&self) -> ErrorCode {
        match self {
            Self::Toml(_) => ErrorCode::new("SYRP-ERROR-0001"),
            Self::InvalidEntry { .. } => ErrorCode::new("SYRP-ERROR-0002"),
            Self::MalformedCode(_) => ErrorCode::new("SYRP-ERROR-0003"),
            Self::UnknownSeverity { .. } => ErrorCode::new("SYRP-ERROR-0004"),
            Self::DuplicateCode(_) => ErrorCode::new("SYRP-ERROR-0005"),
        }
    }

    fn hint(&self) -> Option<Cow<'static, str>> {
        Some(match self {
            Self::Toml(_) => "the file holds only [[error]] tables; see errors/cli.toml".into(),
            Self::InvalidEntry { .. } => {
                "each entry needs string keys code, title, severity, cause, fix (example optional)"
                    .into()
            }
            Self::MalformedCode(_) => "codes look like SYRP-CLI-0203".into(),
            Self::UnknownSeverity { .. } => "use error, warning or info".into(),
            Self::DuplicateCode(_) => {
                "codes are permanent: give the new entry the next free number".into()
            }
        })
    }
}

const REQUIRED: [&str; 5] = ["code", "title", "severity", "cause", "fix"];

/// Parses one catalogue file. Rejects invalid TOML, malformed entries, malformed codes,
/// unknown severities and duplicate codes. A file with no entries yields an empty list.
///
/// ```
/// use syrp_error::catalogue::{parse, ParseError};
///
/// let entry = r#"
///     [[error]]
///     code = "SYRP-RT-0001"
///     title = "t"
///     severity = "error"
///     cause = "c"
///     fix = "f"
/// "#;
/// assert_eq!(parse(entry)?.len(), 1);
/// let twice = format!("{entry}{entry}");
/// assert_eq!(parse(&twice), Err(ParseError::DuplicateCode("SYRP-RT-0001".into())));
/// # Ok::<(), ParseError>(())
/// ```
///
/// # Errors
/// The first problem found, as a [`ParseError`].
pub fn parse(src: &str) -> Result<Vec<Entry>, ParseError> {
    let mut table: toml::Table =
        toml::from_str(src).map_err(|e| ParseError::Toml(e.message().to_owned()))?;
    let list = match table.remove("error") {
        None => Vec::new(),
        Some(toml::Value::Array(list)) => list,
        Some(_) => {
            return Err(ParseError::Toml(
                "`error` must be an array of tables".into(),
            ));
        }
    };
    if let Some(key) = table.keys().next() {
        return Err(ParseError::Toml(format!("unknown top-level key `{key}`")));
    }
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(list.len());
    for (i, value) in list.into_iter().enumerate() {
        let index = i + 1;
        let invalid = |problem: String| ParseError::InvalidEntry { index, problem };
        let toml::Value::Table(mut fields) = value else {
            return Err(invalid("not a table".into()));
        };
        let mut take = |key: &str| -> Result<Option<String>, ParseError> {
            match fields.remove(key) {
                None => Ok(None),
                Some(toml::Value::String(s)) => Ok(Some(s)),
                Some(_) => Err(invalid(format!("`{key}` must be a string"))),
            }
        };
        let mut req = [const { String::new() }; REQUIRED.len()];
        for (slot, key) in req.iter_mut().zip(REQUIRED) {
            *slot = take(key)?.ok_or_else(|| invalid(format!("missing `{key}`")))?;
        }
        let example = take("example")?;
        if let Some(key) = fields.keys().next() {
            return Err(invalid(format!("unknown key `{key}`")));
        }
        let [code, title, severity, cause, fix] = req;
        if !ErrorCode::is_valid(&code) {
            return Err(ParseError::MalformedCode(code));
        }
        let Some(severity) = Severity::parse(&severity) else {
            return Err(ParseError::UnknownSeverity { code, severity });
        };
        if !seen.insert(code.clone()) {
            return Err(ParseError::DuplicateCode(code));
        }
        out.push(Entry {
            code,
            title,
            severity,
            cause,
            fix,
            example,
        });
    }
    Ok(out)
}

/// Renders the docs-site page for a domain.
///
/// Call it with one file's entries to get that domain's page, headed `# <DOMAIN> errors`. If
/// the entries span several domains, each domain becomes its own page (a new `#` heading),
/// in order of first appearance; entries keep their order within a domain.
///
/// ```
/// let entries = syrp_error::catalogue::parse(r#"
///     [[error]]
///     code = "SYRP-RT-0001"
///     title = "Queue full"
///     severity = "error"
///     cause = "c"
///     fix = "f"
/// "#).unwrap();
/// let page = syrp_error::catalogue::render_markdown(&entries);
/// assert!(page.contains("## SYRP-RT-0001: Queue full"));
/// ```
#[must_use]
pub fn render_markdown(entries: &[Entry]) -> String {
    let mut domains: Vec<&str> = Vec::new();
    for e in entries {
        if !domains.contains(&e.domain()) {
            domains.push(e.domain());
        }
    }
    let mut out = String::new();
    for (n, domain) in domains.into_iter().enumerate() {
        if n > 0 {
            out.push('\n');
        }
        // Writing to a String cannot fail.
        let _ = writeln!(out, "# {domain} errors\n");
        let _ = writeln!(
            out,
            "<!-- Generated from errors/*.toml by syrp_error::catalogue. Edit the TOML, not this page. -->"
        );
        for e in entries.iter().filter(|e| e.domain() == domain) {
            let _ = writeln!(out, "\n## {}: {}\n", e.code, e.title);
            let _ = writeln!(out, "**Severity:** {}\n", e.severity);
            let _ = writeln!(out, "**Cause.** {}\n", e.cause);
            let _ = writeln!(out, "**Fix.** {}", e.fix);
            if let Some(example) = &e.example {
                let _ = writeln!(out, "\n**Example.** {example}");
            }
        }
    }
    out
}

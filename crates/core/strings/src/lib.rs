//! String utilities that have no `std` equivalent (master plan §5.2, L2 Core).
//!
//! Rust's [`String`] and `&str` replace JUCE's `String`, `StringArray` and `StringRef`. This crate
//! holds the remaining utility classes:
//!
//! - [`base64`]: RFC 4648 §4 Base64 with the standard alphabet and padding (JUCE's `Base64`).
//! - [`StringPool`]: interning of equal text into one shared [`Arc<str>`](std::sync::Arc).
//! - [`TextDiff`]: a list of character-position insertions and removals that turn one text into
//!   another.
//!
//! - [`LocalisedStrings`]: a parsed translation file (JUCE's `LocalisedStrings`), and the
//!   process-wide [`set_current_mappings`] and [`translate`] (JUCE's `TRANS`).
//!
//! ```
//! use syrp_strings::{base64, StringPool, TextDiff};
//! assert_eq!(base64::encode(b"hi"), "aGk=");
//! let pool = StringPool::new();
//! assert!(std::sync::Arc::ptr_eq(&pool.get("a"), &pool.get("a")));
//! assert_eq!(TextDiff::new("cat", "cart").apply("cat"), "cart");
//! ```

pub mod base64;
mod current;
mod diff;
mod localised;
mod pool;

pub use current::{current_mappings, set_current_mappings, translate};

pub use diff::{Change, TextDiff};
pub use localised::LocalisedStrings;
pub use pool::StringPool;

use std::fmt;

use syrp_error::{Diagnostic, ErrorCode};

/// What went wrong in a [`StringsError`].
///
/// ```
/// use syrp_strings::{base64, StringsErrorKind};
/// assert_eq!(base64::decode("a").unwrap_err().kind(), StringsErrorKind::Base64Padding);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StringsErrorKind {
    /// A character outside the Base64 alphabet (`SYRP-STRINGS-0001`).
    Base64Character,
    /// Base64 text with a bad length, misplaced `=` or non-zero unused bits (`SYRP-STRINGS-0002`).
    Base64Padding,
    /// A buffer sized by the input could not be allocated (`SYRP-STRINGS-0003`).
    OutOfMemory,
    /// A translation-file line that is not a header, comment or mapping (`SYRP-STRINGS-0004`).
    TranslationSyntax,
    /// Merging translations for two different languages (`SYRP-STRINGS-0005`).
    LanguageMismatch,
}

/// The error type of this crate: a [`StringsErrorKind`] plus, where it applies, a position.
///
/// It implements [`Diagnostic`], so every error carries a catalogued `SYRP-STRINGS-*` code.
///
/// ```
/// use syrp_error::Diagnostic;
/// use syrp_strings::base64;
/// let err = base64::decode("ab!d").unwrap_err();
/// assert_eq!(err.code().as_str(), "SYRP-STRINGS-0001");
/// assert_eq!(err.position(), Some(2));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StringsError {
    kind: StringsErrorKind,
    position: Option<usize>,
    line: Option<usize>,
}

impl StringsError {
    pub(crate) const fn new(kind: StringsErrorKind, position: Option<usize>) -> Self {
        Self {
            kind,
            position,
            line: None,
        }
    }

    pub(crate) const fn at_line(kind: StringsErrorKind, line: usize, position: usize) -> Self {
        Self {
            kind,
            position: Some(position),
            line: Some(line),
        }
    }

    /// What went wrong.
    ///
    /// ```
    /// use syrp_strings::{base64, StringsErrorKind};
    /// assert_eq!(base64::decode("@@@@").unwrap_err().kind(), StringsErrorKind::Base64Character);
    /// ```
    pub const fn kind(&self) -> StringsErrorKind {
        self.kind
    }

    /// The byte offset where the problem was found, if the error has one. For a translation
    /// file it counts from the start of [`StringsError::line`].
    ///
    /// ```
    /// use syrp_strings::base64;
    /// assert_eq!(base64::decode("AAAA AAA").unwrap_err().position(), Some(4));
    /// ```
    pub const fn position(&self) -> Option<usize> {
        self.position
    }

    /// The 1-based line number in a translation file, for `SYRP-STRINGS-0004`.
    ///
    /// ```
    /// let err = syrp_strings::LocalisedStrings::parse("\"a\" = \"b\"\nnonsense", false).unwrap_err();
    /// assert_eq!(err.line(), Some(2));
    /// ```
    pub const fn line(&self) -> Option<usize> {
        self.line
    }
}

impl fmt::Display for StringsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            kind,
            position,
            line,
        } = self;
        f.write_str(match kind {
            StringsErrorKind::Base64Character => "character outside the Base64 alphabet",
            StringsErrorKind::Base64Padding => "bad Base64 padding or length",
            StringsErrorKind::OutOfMemory => "out of memory for a string operation",
            StringsErrorKind::TranslationSyntax => "malformed translation line",
            StringsErrorKind::LanguageMismatch => {
                "cannot merge translations for different languages"
            }
        })?;
        match (line, position) {
            (Some(l), Some(p)) => write!(f, " at line {l}, byte {p}"),
            (Some(l), None) => write!(f, " at line {l}"),
            (None, Some(p)) => write!(f, " at byte {p}"),
            (None, None) => Ok(()),
        }
    }
}

impl std::error::Error for StringsError {}

impl Diagnostic for StringsError {
    fn code(&self) -> ErrorCode {
        ErrorCode::new(match self.kind {
            StringsErrorKind::Base64Character => "SYRP-STRINGS-0001",
            StringsErrorKind::Base64Padding => "SYRP-STRINGS-0002",
            StringsErrorKind::OutOfMemory => "SYRP-STRINGS-0003",
            StringsErrorKind::TranslationSyntax => "SYRP-STRINGS-0004",
            StringsErrorKind::LanguageMismatch => "SYRP-STRINGS-0005",
        })
    }
}

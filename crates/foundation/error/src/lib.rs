//! The one error vocabulary every Syrp crate uses.
//!
//! Every failure in Syrp maps to a catalogued [`ErrorCode`] of the form `SYRP-<DOMAIN>-<NNNN>`,
//! whose cause and fix live in `errors/<domain>.toml` and are shown by
//! `cargo syrp explain <code>`. Error types implement [`Diagnostic`]; code that just needs a
//! ready-made error uses [`Report`].
//!
//! Default features have no dependencies. Optional features:
//! - `serde`: [`Report`] serialises to the CLI envelope's diagnostic object.
//! - `catalogue`: [`catalogue`] parses `errors/*.toml` and renders the docs pages.
//!
//! ```
//! use syrp_error::{Diagnostic, ErrorCode, Report, Severity};
//!
//! const NOT_FOUND: ErrorCode = ErrorCode::new("SYRP-CLI-0001");
//! let report = Report::new(NOT_FOUND, "not inside a Syrp repository").with_hint("pass --root");
//! assert_eq!(report.code(), NOT_FOUND);
//! assert_eq!(report.severity(), Severity::Error);
//! assert_eq!(report.docs_url(), "https://docs.syrp.dev/errors/SYRP-CLI-0001");
//! ```

use std::borrow::Cow;
use std::fmt;

#[cfg(feature = "catalogue")]
pub mod catalogue;
mod report;

pub use report::Report;

/// The base of every error's documentation page; [`ErrorCode::docs_url`] appends the code.
///
/// ```
/// assert!(syrp_error::DOCS_BASE_URL.ends_with('/'));
/// ```
pub const DOCS_BASE_URL: &str = "https://docs.syrp.dev/errors/";

/// A catalogued error code such as `SYRP-CLI-0001`.
///
/// The shape is `SYRP-`, then a domain of one or more ASCII capitals, then `-` and exactly four
/// ASCII digits. [`ErrorCode::new`] checks it; in a `const` a malformed code fails the build.
///
/// ```
/// use syrp_error::ErrorCode;
///
/// const CODE: ErrorCode = ErrorCode::new("SYRP-WP-0003");
/// assert_eq!(CODE.as_str(), "SYRP-WP-0003");
/// assert_eq!(CODE.domain(), "WP");
/// assert_eq!(CODE.to_string(), "SYRP-WP-0003");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ErrorCode(&'static str);

impl ErrorCode {
    /// Wraps `code`, panicking if it is not `SYRP-<DOMAIN>-<NNNN>`.
    ///
    /// Use it in a `const` so the check runs at compile time:
    ///
    /// ```
    /// use syrp_error::ErrorCode;
    /// const OK: ErrorCode = ErrorCode::new("SYRP-RT-0042");
    /// # let _ = OK;
    /// ```
    ///
    /// ```compile_fail
    /// use syrp_error::ErrorCode;
    /// const BAD: ErrorCode = ErrorCode::new("SYRP-rt-42"); // fails const evaluation
    /// ```
    ///
    /// # Panics
    /// If `code` is malformed (see [`ErrorCode::is_valid`]).
    #[must_use]
    pub const fn new(code: &'static str) -> Self {
        assert!(
            Self::is_valid(code),
            "malformed error code: expected SYRP-<DOMAIN>-<NNNN>"
        );
        Self(code)
    }

    /// Returns whether `code` has the `SYRP-<DOMAIN>-<NNNN>` shape, without panicking.
    ///
    /// ```
    /// use syrp_error::ErrorCode;
    /// assert!(ErrorCode::is_valid("SYRP-CLI-0203"));
    /// assert!(!ErrorCode::is_valid("SYRP-CLI-203"));
    /// assert!(!ErrorCode::is_valid("syrp-cli-0203"));
    /// ```
    #[must_use]
    pub const fn is_valid(code: &str) -> bool {
        const PREFIX: &[u8] = b"SYRP-";
        let b = code.as_bytes();
        // Prefix, at least one domain letter, a dash and four digits.
        if b.len() < PREFIX.len() + 1 + 1 + 4 {
            return false;
        }
        let mut i = 0;
        while i < PREFIX.len() {
            if b[i] != PREFIX[i] {
                return false;
            }
            i += 1;
        }
        let dash = b.len() - 5;
        if b[dash] != b'-' {
            return false;
        }
        while i < dash {
            if !b[i].is_ascii_uppercase() {
                return false;
            }
            i += 1;
        }
        i = dash + 1;
        while i < b.len() {
            if !b[i].is_ascii_digit() {
                return false;
            }
            i += 1;
        }
        true
    }

    /// The code as text, for example `"SYRP-CLI-0001"`.
    ///
    /// ```
    /// # use syrp_error::ErrorCode;
    /// assert_eq!(ErrorCode::new("SYRP-CLI-0001").as_str(), "SYRP-CLI-0001");
    /// ```
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }

    /// The domain part, for example `"CLI"` (catalogued in `errors/cli.toml`).
    ///
    /// ```
    /// # use syrp_error::ErrorCode;
    /// assert_eq!(ErrorCode::new("SYRP-CLI-0001").domain(), "CLI");
    /// ```
    #[must_use]
    pub fn domain(self) -> &'static str {
        &self.0[5..self.0.len() - 5]
    }

    /// The documentation page for this code: [`DOCS_BASE_URL`] followed by the code.
    ///
    /// ```
    /// # use syrp_error::ErrorCode;
    /// assert_eq!(
    ///     ErrorCode::new("SYRP-CLI-0001").docs_url(),
    ///     "https://docs.syrp.dev/errors/SYRP-CLI-0001"
    /// );
    /// ```
    #[must_use]
    pub fn docs_url(self) -> String {
        format!("{DOCS_BASE_URL}{}", self.0)
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl AsRef<str> for ErrorCode {
    fn as_ref(&self) -> &str {
        self.0
    }
}

/// How serious a diagnostic is. Matches the envelope's `severity` values.
///
/// ```
/// use syrp_error::Severity;
/// assert_eq!(Severity::Warning.as_str(), "warning");
/// assert_eq!(Severity::parse("info"), Some(Severity::Info));
/// assert!(Severity::Error > Severity::Warning);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Severity {
    /// Informational; nothing is wrong.
    Info,
    /// Something is suspicious but the operation went ahead.
    Warning,
    /// The operation failed. The default.
    #[default]
    Error,
}

impl Severity {
    /// The lowercase name used in the envelope and the catalogue: `error`, `warning` or `info`.
    ///
    /// ```
    /// assert_eq!(syrp_error::Severity::Error.as_str(), "error");
    /// ```
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }

    /// Parses the lowercase name; returns `None` for anything else (case-sensitive).
    ///
    /// ```
    /// use syrp_error::Severity;
    /// assert_eq!(Severity::parse("warning"), Some(Severity::Warning));
    /// assert_eq!(Severity::parse("fatal"), None);
    /// ```
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "info" => Some(Self::Info),
            "warning" => Some(Self::Warning),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An error that carries a catalogued code, a severity, an optional hint and a docs link.
///
/// Every Syrp error type implements this. Only [`Diagnostic::code`] is required; the
/// message is the type's `Display` output.
///
/// ```
/// use std::borrow::Cow;
/// use std::fmt;
/// use syrp_error::{Diagnostic, ErrorCode};
///
/// #[derive(Debug)]
/// struct QueueFull;
///
/// impl fmt::Display for QueueFull {
///     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
///         f.write_str("the queue is full")
///     }
/// }
/// impl std::error::Error for QueueFull {}
///
/// impl Diagnostic for QueueFull {
///     fn code(&self) -> ErrorCode {
///         ErrorCode::new("SYRP-RT-0001")
///     }
///     fn hint(&self) -> Option<Cow<'static, str>> {
///         Some("drain the queue on the consumer side first".into())
///     }
/// }
///
/// assert_eq!(QueueFull.docs_url(), "https://docs.syrp.dev/errors/SYRP-RT-0001");
/// ```
pub trait Diagnostic: std::error::Error {
    /// The catalogued code.
    ///
    /// ```
    /// # use syrp_error::{Diagnostic, ErrorCode, Report};
    /// let r = Report::new(ErrorCode::new("SYRP-CLI-0005"), "bad args");
    /// assert_eq!(r.code().as_str(), "SYRP-CLI-0005");
    /// ```
    fn code(&self) -> ErrorCode;

    /// How serious it is. Defaults to [`Severity::Error`].
    ///
    /// ```
    /// # use syrp_error::{Diagnostic, ErrorCode, Report, Severity};
    /// let r = Report::new(ErrorCode::new("SYRP-CLI-0102"), "old rustc");
    /// assert_eq!(r.severity(), Severity::Error);
    /// ```
    fn severity(&self) -> Severity {
        Severity::Error
    }

    /// A short suggestion for fixing this occurrence. Defaults to `None`.
    ///
    /// ```
    /// # use syrp_error::{Diagnostic, ErrorCode, Report};
    /// let r = Report::new(ErrorCode::new("SYRP-CLI-0005"), "bad args");
    /// assert_eq!(r.hint(), None);
    /// ```
    fn hint(&self) -> Option<Cow<'static, str>> {
        None
    }

    /// The documentation page. Defaults to [`ErrorCode::docs_url`] of [`Diagnostic::code`].
    ///
    /// ```
    /// # use syrp_error::{Diagnostic, ErrorCode, Report};
    /// let r = Report::new(ErrorCode::new("SYRP-CLI-0005"), "bad args");
    /// assert_eq!(r.docs_url(), "https://docs.syrp.dev/errors/SYRP-CLI-0005");
    /// ```
    fn docs_url(&self) -> String {
        self.code().docs_url()
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for ErrorCode {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.0)
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for Severity {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

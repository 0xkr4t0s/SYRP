//! [`VarError`] and its `SYRP-VAR-*` codes (catalogued in `errors/var.toml`).

use std::borrow::Cow;
use std::fmt;

use syrp_error::{Diagnostic, ErrorCode};

/// Code for invalid JSON text.
const INVALID_JSON: ErrorCode = ErrorCode::new("SYRP-VAR-0001");
/// Code for input nested more deeply than [`crate::json::MAX_DEPTH`].
const TOO_DEEP: ErrorCode = ErrorCode::new("SYRP-VAR-0002");
/// Code for a value that cannot be written because it is too deep or cyclic.
const UNWRITABLE: ErrorCode = ErrorCode::new("SYRP-VAR-0003");

/// An error from reading or writing a [`Var`](crate::Var) as JSON.
///
/// It carries a catalogued code (see [`Diagnostic`]) and, for parse errors, the 1-based line
/// and column where the problem was found.
///
/// ```
/// use syrp_var::{Diagnostic, json};
///
/// let error = json::parse("{\n  \"a\": ?\n}").unwrap_err();
/// assert_eq!(error.code().as_str(), "SYRP-VAR-0001");
/// assert_eq!(error.line(), 2);
/// assert!(error.column() > 0);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarError {
    code: ErrorCode,
    message: String,
    line: usize,
    column: usize,
}

impl VarError {
    /// Wraps a `serde_json` parse error; `too_deep` is set when the depth limit stopped it.
    pub(crate) fn parse(error: &serde_json::Error, too_deep: bool) -> Self {
        Self {
            code: if too_deep { TOO_DEEP } else { INVALID_JSON },
            message: error.to_string(),
            line: error.line(),
            column: error.column(),
        }
    }

    /// Wraps a `serde_json` error from writing a value.
    pub(crate) fn write(error: &serde_json::Error) -> Self {
        Self {
            code: UNWRITABLE,
            message: error.to_string(),
            line: 0,
            column: 0,
        }
    }

    /// The 1-based line of a parse error; 0 when there is no position (writing).
    ///
    /// ```
    /// let error = syrp_var::json::parse("[1,\n2,\n]").unwrap_err();
    /// assert_eq!(error.line(), 3);
    /// ```
    #[must_use]
    pub fn line(&self) -> usize {
        self.line
    }

    /// The 1-based column of a parse error; 0 when there is no position (writing).
    ///
    /// ```
    /// let error = syrp_var::json::parse("[1 2]").unwrap_err();
    /// assert_eq!(error.column(), 4);
    /// ```
    #[must_use]
    pub fn column(&self) -> usize {
        self.column
    }
}

impl fmt::Display for VarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for VarError {}

impl Diagnostic for VarError {
    fn code(&self) -> ErrorCode {
        self.code
    }

    fn hint(&self) -> Option<Cow<'static, str>> {
        Some(
            match self.code.as_str() {
                "SYRP-VAR-0002" => "flatten the input: at most 100 levels of nesting",
                "SYRP-VAR-0003" => "remove the property that refers back to an enclosing object",
                _ => "fix the JSON at the reported line and column",
            }
            .into(),
        )
    }
}

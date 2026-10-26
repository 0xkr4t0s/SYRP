//! [`Report`]: a ready-made [`Diagnostic`] built from parts.

use std::borrow::Cow;
use std::fmt;

use crate::{Diagnostic, ErrorCode, Severity};

/// A general-purpose error: a code, a message, and optionally a severity, location and hint.
///
/// Build it with [`Report::new`] and the `with_*` methods. Its `Display` output is the message.
/// With the `serde` feature it serialises to exactly the `diagnostics` item of the
/// `cargo syrp` envelope (`docs/agents/envelope.schema.json`):
/// `{"code", "severity", "message", "location"?, "hint"?, "docs"}`, where `docs` is
/// [`Diagnostic::docs_url`].
///
/// ```
/// use syrp_error::{Diagnostic, ErrorCode, Report, Severity};
///
/// let r = Report::new(ErrorCode::new("SYRP-CLI-0102"), "rustc 1.90 is not the pinned 1.95")
///     .with_severity(Severity::Warning)
///     .with_location("rust-toolchain.toml")
///     .with_hint("run `rustup toolchain install`");
/// assert_eq!(r.to_string(), "rustc 1.90 is not the pinned 1.95");
/// assert_eq!(r.severity(), Severity::Warning);
/// assert_eq!(r.location(), Some("rust-toolchain.toml"));
/// assert_eq!(r.hint().as_deref(), Some("run `rustup toolchain install`"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    code: ErrorCode,
    severity: Severity,
    message: Cow<'static, str>,
    location: Option<String>,
    hint: Option<Cow<'static, str>>,
}

impl Report {
    /// A report with severity [`Severity::Error`] and no location or hint.
    ///
    /// ```
    /// # use syrp_error::{ErrorCode, Report};
    /// let r = Report::new(ErrorCode::new("SYRP-WP-0001"), format!("{} is invalid", "wave.toml"));
    /// assert_eq!(r.message(), "wave.toml is invalid");
    /// ```
    #[must_use]
    pub fn new(code: ErrorCode, message: impl Into<Cow<'static, str>>) -> Self {
        Self {
            code,
            severity: Severity::Error,
            message: message.into(),
            location: None,
            hint: None,
        }
    }

    /// Sets the severity.
    ///
    /// ```
    /// # use syrp_error::{Diagnostic, ErrorCode, Report, Severity};
    /// let r = Report::new(ErrorCode::new("SYRP-WP-0001"), "m").with_severity(Severity::Info);
    /// assert_eq!(r.severity(), Severity::Info);
    /// ```
    #[must_use]
    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    /// Sets where the problem is, free-form: a path, `path:line`, a crate or a test name.
    ///
    /// ```
    /// # use syrp_error::{ErrorCode, Report};
    /// let r = Report::new(ErrorCode::new("SYRP-WP-0001"), "m").with_location("src/lib.rs:12");
    /// assert_eq!(r.location(), Some("src/lib.rs:12"));
    /// ```
    #[must_use]
    pub fn with_location(mut self, location: impl Into<String>) -> Self {
        self.location = Some(location.into());
        self
    }

    /// Sets the hint returned by [`Diagnostic::hint`].
    ///
    /// ```
    /// # use syrp_error::{Diagnostic, ErrorCode, Report};
    /// let r = Report::new(ErrorCode::new("SYRP-WP-0001"), "m").with_hint("fix the file");
    /// assert_eq!(r.hint().as_deref(), Some("fix the file"));
    /// ```
    #[must_use]
    pub fn with_hint(mut self, hint: impl Into<Cow<'static, str>>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// The message, which is also the `Display` output.
    ///
    /// ```
    /// # use syrp_error::{ErrorCode, Report};
    /// assert_eq!(Report::new(ErrorCode::new("SYRP-WP-0001"), "m").message(), "m");
    /// ```
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The location, if one was set.
    ///
    /// ```
    /// # use syrp_error::{ErrorCode, Report};
    /// assert_eq!(Report::new(ErrorCode::new("SYRP-WP-0001"), "m").location(), None);
    /// ```
    #[must_use]
    pub fn location(&self) -> Option<&str> {
        self.location.as_deref()
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Report {}

impl Diagnostic for Report {
    fn code(&self) -> ErrorCode {
        self.code
    }

    fn severity(&self) -> Severity {
        self.severity
    }

    fn hint(&self) -> Option<Cow<'static, str>> {
        self.hint.clone()
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for Report {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let len = 4 + usize::from(self.location.is_some()) + usize::from(self.hint.is_some());
        let mut st = s.serialize_struct("Diagnostic", len)?;
        st.serialize_field("code", &self.code)?;
        st.serialize_field("severity", &self.severity)?;
        st.serialize_field("message", &*self.message)?;
        if let Some(location) = &self.location {
            st.serialize_field("location", location)?;
        }
        if let Some(hint) = &self.hint {
            st.serialize_field("hint", &**hint)?;
        }
        st.serialize_field("docs", &self.docs_url())?;
        st.end()
    }
}

//! The crate's error type and its `SYRP-DOC-*` codes (catalogued in `errors/doc.toml`).

use std::borrow::Cow;
use std::fmt;
use std::path::PathBuf;

use syrp_error::{Diagnostic, ErrorCode};

/// Everything that can go wrong in the docs pipeline. Each variant has a stable code.
///
/// ```
/// use syrp_docgen::{DocError, Diagnostic};
/// let e = DocError::NightlyMissing { toolchain: "nightly".into(), detail: "gone".into() };
/// assert_eq!(e.code().as_str(), "SYRP-DOC-0001");
/// assert!(e.hint().unwrap().contains("rustup toolchain install nightly"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocError {
    /// SYRP-DOC-0001: producing rustdoc JSON needs a nightly toolchain that is not available.
    NightlyMissing {
        /// The toolchain name that was probed, for example `nightly`.
        toolchain: String,
        /// What the probe reported.
        detail: String,
    },
    /// SYRP-DOC-0002: the JSON uses a format version this build of `rustdoc-types` cannot read.
    FormatVersion {
        /// The file that was read.
        path: PathBuf,
        /// `format_version` found in the file.
        found: u32,
        /// The version this crate reads.
        expected: u32,
    },
    /// SYRP-DOC-0003: the rustdoc JSON could not be produced, read or parsed.
    Read {
        /// The file involved.
        path: PathBuf,
        /// The underlying error message.
        detail: String,
    },
}

impl Diagnostic for DocError {
    fn code(&self) -> ErrorCode {
        match self {
            Self::NightlyMissing { .. } => ErrorCode::new("SYRP-DOC-0001"),
            Self::FormatVersion { .. } => ErrorCode::new("SYRP-DOC-0002"),
            Self::Read { .. } => ErrorCode::new("SYRP-DOC-0003"),
        }
    }

    fn hint(&self) -> Option<Cow<'static, str>> {
        Some(match self {
            Self::NightlyMissing { toolchain: t, .. } => {
                format!("run `rustup toolchain install {t} --profile minimal`").into()
            }
            Self::FormatVersion { found, .. } => {
                format!("use a nightly that matches rustdoc-types, or bump it to 0.{found}").into()
            }
            Self::Read { .. } => "run the `cargo +nightly rustdoc` command by hand".into(),
        })
    }
}

impl fmt::Display for DocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            Self::NightlyMissing {
                toolchain: t,
                detail,
            } => {
                format!("rustdoc JSON needs the `{t}` toolchain ({detail})")
            }
            Self::FormatVersion {
                path: p,
                found: n,
                expected: e,
            } => {
                format!(
                    "{} is rustdoc JSON format {n}; syrp-docgen reads {e}",
                    p.display()
                )
            }
            Self::Read { path, detail } => format!("rustdoc JSON {}: {detail}", path.display()),
        };
        write!(f, "{}: {msg}", self.code())
    }
}

impl std::error::Error for DocError {}

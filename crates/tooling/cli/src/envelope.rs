//! The result envelope every command emits, and the stable exit codes (master plan §13.2).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Version tag of the envelope format. Bump only with a `BREAKING.md` entry.
pub const ENVELOPE_SCHEMA: &str = "auricle.envelope/v1";

/// Process exit codes. These numbers are a public contract and never change meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum ExitCode {
    /// Everything passed.
    Ok = 0,
    /// Checks or tests failed.
    Failed = 1,
    /// The command line was invalid.
    Usage = 2,
    /// A required tool or toolchain is missing.
    Environment = 3,
    /// A parity scenario regressed.
    Parity = 4,
    /// A plugin validator failed.
    Validator = 5,
    /// A golden image or audio file did not match.
    Golden = 6,
    /// A frozen contract or public API was violated.
    Contract = 7,
    /// A real-time safety rule was violated.
    RtSafety = 8,
    /// A clean-room or licence gate failed.
    CleanRoom = 9,
    /// A bug in the CLI itself.
    Internal = 10,
}

impl ExitCode {
    /// The numeric process exit status.
    pub fn code(self) -> i32 {
        self as i32
    }

    /// Picks the most important of two exit codes, so a run that hit several kinds of
    /// failure reports the most specific one rather than plain `Failed`.
    pub fn worst(self, other: ExitCode) -> ExitCode {
        fn rank(c: ExitCode) -> u8 {
            match c {
                ExitCode::Ok => 0,
                ExitCode::Failed => 1,
                ExitCode::Internal => 10,
                ExitCode::Usage => 9,
                ExitCode::Environment => 8,
                _ => 5,
            }
        }
        if rank(other) > rank(self) {
            other
        } else {
            self
        }
    }
}

/// How serious a diagnostic is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Blocks success.
    Error,
    /// Worth fixing, does not block.
    Warning,
    /// Information only.
    Info,
}

/// One finding, always carrying a catalogued `AUR-*` code.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Diagnostic {
    /// Catalogued error code, for example `AUR-WP-0003`. `cargo auricle explain <code>` documents it.
    pub code: String,
    /// Severity.
    pub severity: Severity,
    /// What happened, in one sentence.
    pub message: String,
    /// File path, optionally with `:line`, when the finding has a location.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    /// The next thing to try.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// Where the long-form explanation lives.
    pub docs: String,
}

impl Diagnostic {
    /// An error diagnostic. The docs link is derived from the code.
    pub fn error(code: &str, message: impl Into<String>) -> Self {
        Self::new(code, Severity::Error, message)
    }

    /// A warning diagnostic.
    pub fn warning(code: &str, message: impl Into<String>) -> Self {
        Self::new(code, Severity::Warning, message)
    }

    fn new(code: &str, severity: Severity, message: impl Into<String>) -> Self {
        Diagnostic {
            code: code.to_string(),
            severity,
            message: message.into(),
            location: None,
            hint: None,
            docs: format!("cargo auricle explain {code}"),
        }
    }

    /// Attaches a location.
    pub fn at(mut self, location: impl Into<String>) -> Self {
        self.location = Some(location.into());
        self
    }

    /// Attaches a hint.
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

/// The outcome of one command, before the envelope adds timing and naming.
#[derive(Debug, Default)]
pub struct Outcome {
    /// Command-specific result objects.
    pub results: Vec<Value>,
    /// Findings.
    pub diagnostics: Vec<Diagnostic>,
    /// Paths of files the command wrote.
    pub artifacts: Vec<String>,
    /// Set when the command wants a specific failure code; otherwise derived from diagnostics.
    pub exit: Option<ExitCode>,
}

impl Outcome {
    /// An outcome with a single result value.
    pub fn result(value: Value) -> Self {
        Outcome {
            results: vec![value],
            ..Default::default()
        }
    }

    /// The exit code this outcome implies.
    pub fn exit_code(&self) -> ExitCode {
        if let Some(code) = self.exit {
            return code;
        }
        if self
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
        {
            ExitCode::Failed
        } else {
            ExitCode::Ok
        }
    }
}

/// The JSON object printed on stdout for every command.
#[derive(Debug, Serialize, Deserialize)]
pub struct Envelope {
    /// Always [`ENVELOPE_SCHEMA`].
    pub schema: String,
    /// The command, for example `wp ready`.
    pub command: String,
    /// True when `exit_code` is 0.
    pub ok: bool,
    /// See [`ExitCode`].
    pub exit_code: i32,
    /// Wall time.
    pub duration_ms: u128,
    /// Command-specific results.
    pub results: Vec<Value>,
    /// Findings.
    pub diagnostics: Vec<Diagnostic>,
    /// Files written.
    pub artifacts: Vec<String>,
}

impl Envelope {
    /// Wraps an outcome.
    pub fn new(command: &str, outcome: Outcome, duration_ms: u128) -> Self {
        let exit = outcome.exit_code();
        Envelope {
            schema: ENVELOPE_SCHEMA.to_string(),
            command: command.to_string(),
            ok: exit == ExitCode::Ok,
            exit_code: exit.code(),
            duration_ms,
            results: outcome.results,
            diagnostics: outcome.diagnostics,
            artifacts: outcome.artifacts,
        }
    }

    /// A short human-readable rendering for terminals.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for r in &self.results {
            match r {
                Value::String(s) => out.push_str(s),
                other => out.push_str(&serde_json::to_string_pretty(other).unwrap_or_default()),
            }
            out.push('\n');
        }
        for d in &self.diagnostics {
            let sev = match d.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
                Severity::Info => "info",
            };
            out.push_str(&format!("{sev}[{}]: {}\n", d.code, d.message));
            if let Some(loc) = &d.location {
                out.push_str(&format!("  --> {loc}\n"));
            }
            if let Some(h) = &d.hint {
                out.push_str(&format!("  hint: {h}\n"));
            }
        }
        for a in &self.artifacts {
            out.push_str(&format!("wrote {a}\n"));
        }
        out.push_str(&format!(
            "{} ({} ms, exit {})\n",
            if self.ok { "ok" } else { "FAILED" },
            self.duration_ms,
            self.exit_code
        ));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_imply_failure() {
        let mut o = Outcome::default();
        assert_eq!(o.exit_code(), ExitCode::Ok);
        o.diagnostics
            .push(Diagnostic::warning("AUR-CLI-0205", "big"));
        assert_eq!(o.exit_code(), ExitCode::Ok);
        o.diagnostics
            .push(Diagnostic::error("AUR-CLI-0203", "layer"));
        assert_eq!(o.exit_code(), ExitCode::Failed);
    }

    #[test]
    fn specific_codes_beat_generic_failure() {
        assert_eq!(
            ExitCode::Failed.worst(ExitCode::Contract),
            ExitCode::Contract
        );
        assert_eq!(
            ExitCode::Contract.worst(ExitCode::Failed),
            ExitCode::Contract
        );
        assert_eq!(ExitCode::Ok.worst(ExitCode::Failed), ExitCode::Failed);
    }

    #[test]
    fn envelope_shape_is_stable() {
        let env = Envelope::new("doctor", Outcome::result(serde_json::json!({"a": 1})), 5);
        let v = serde_json::to_value(&env).unwrap();
        for key in [
            "schema",
            "command",
            "ok",
            "exit_code",
            "duration_ms",
            "results",
            "diagnostics",
            "artifacts",
        ] {
            assert!(v.get(key).is_some(), "missing {key}");
        }
        assert_eq!(v["schema"], ENVELOPE_SCHEMA);
    }
}

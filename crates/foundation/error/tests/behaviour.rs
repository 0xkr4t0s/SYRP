//! One test (or more) per spec behaviour of WP-0.1-03.

use std::borrow::Cow;

use syrp_error::catalogue::{self, ParseError};
use syrp_error::{Diagnostic, ErrorCode, Report, Severity};

const ENTRY: &str = r#"
[[error]]
code = "SYRP-RT-0001"
title = "Queue full"
severity = "error"
cause = "c"
fix = "f"
"#;

#[test]
fn b1_error_code_is_copy_and_const_constructible() {
    const CODE: ErrorCode = ErrorCode::new("SYRP-CLI-0001");
    let a = CODE;
    let b = a; // Copy: `a` is still usable.
    assert_eq!(a, b);
    assert_eq!(a.as_str(), "SYRP-CLI-0001");
    assert_eq!(a.domain(), "CLI");
    assert_eq!(ErrorCode::new("SYRP-MIDIIO-9999").domain(), "MIDIIO");
    assert_eq!(size_of::<ErrorCode>(), size_of::<&'static str>());
}

#[test]
fn b1_is_valid_accepts_only_the_code_shape() {
    for ok in ["SYRP-CLI-0001", "SYRP-A-0000", "SYRP-ERROR-1234"] {
        assert!(ErrorCode::is_valid(ok), "{ok}");
    }
    for bad in [
        "",
        "SYRP--0001",
        "SYRP-CLI-001",
        "SYRP-CLI-00011",
        "SYRP-cli-0001",
        "AUR-CLI-0001",
        "syrp-CLI-0001",
        "SYRP-C1I-0001",
        "SYRP-CLI_0001",
        "SYRP-CLI-00a1",
        " SYRP-CLI-0001",
        "SYRP-CLI-0001 ",
        "SYRP-ÄB-0001",
    ] {
        assert!(!ErrorCode::is_valid(bad), "{bad:?}");
    }
}

#[test]
#[should_panic(expected = "malformed error code")]
fn b1_new_panics_on_a_malformed_code() {
    let _ = ErrorCode::new("SYRP-CLI-1");
}

#[test]
fn b2_diagnostic_defaults_derive_from_the_code() {
    #[derive(Debug)]
    struct Minimal;
    impl std::fmt::Display for Minimal {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("minimal")
        }
    }
    impl std::error::Error for Minimal {}
    impl Diagnostic for Minimal {
        fn code(&self) -> ErrorCode {
            ErrorCode::new("SYRP-RT-0007")
        }
    }
    let d: &dyn Diagnostic = &Minimal;
    assert_eq!(d.code().as_str(), "SYRP-RT-0007");
    assert_eq!(d.severity(), Severity::Error);
    assert_eq!(d.hint(), None);
    assert_eq!(d.docs_url(), "https://docs.syrp.dev/errors/SYRP-RT-0007");
}

#[test]
fn b2_severity_names_round_trip() {
    for s in [Severity::Error, Severity::Warning, Severity::Info] {
        assert_eq!(Severity::parse(s.as_str()), Some(s));
        assert_eq!(s.to_string(), s.as_str());
    }
    assert_eq!(Severity::parse("Error"), None);
    assert!(Severity::Info < Severity::Warning && Severity::Warning < Severity::Error);
}

#[test]
fn b3_report_builder_implements_diagnostic() {
    let r = Report::new(ErrorCode::new("SYRP-CLI-0102"), "old rustc")
        .with_severity(Severity::Warning)
        .with_location("rust-toolchain.toml")
        .with_hint("rustup toolchain install");
    assert_eq!(r.code(), ErrorCode::new("SYRP-CLI-0102"));
    assert_eq!(r.severity(), Severity::Warning);
    assert_eq!(r.message(), "old rustc");
    assert_eq!(r.to_string(), "old rustc");
    assert_eq!(r.location(), Some("rust-toolchain.toml"));
    assert_eq!(r.hint(), Some(Cow::Borrowed("rustup toolchain install")));
    let boxed: Box<dyn std::error::Error + Send + Sync> = Box::new(r);
    assert_eq!(boxed.to_string(), "old rustc");
}

#[test]
fn b3_report_serialises_to_the_envelope_diagnostic_shape() {
    let full = Report::new(ErrorCode::new("SYRP-CLI-0102"), "old rustc")
        .with_severity(Severity::Warning)
        .with_location("rust-toolchain.toml")
        .with_hint("h");
    assert_eq!(
        serde_json::to_value(&full).unwrap(),
        serde_json::json!({
            "code": "SYRP-CLI-0102",
            "severity": "warning",
            "message": "old rustc",
            "location": "rust-toolchain.toml",
            "hint": "h",
            "docs": "https://docs.syrp.dev/errors/SYRP-CLI-0102",
        })
    );
    // Optional keys are omitted, not null: the schema has additionalProperties false and
    // types location and hint as strings.
    let bare = Report::new(ErrorCode::new("SYRP-CLI-0005"), "m");
    assert_eq!(
        serde_json::to_string(&bare).unwrap(),
        r#"{"code":"SYRP-CLI-0005","severity":"error","message":"m","docs":"https://docs.syrp.dev/errors/SYRP-CLI-0005"}"#
    );
}

#[test]
fn b3_serialised_report_satisfies_the_schema_constraints() {
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../../../../docs/agents/envelope.schema.json")).unwrap();
    let item = &schema["properties"]["diagnostics"]["items"];
    let allowed: Vec<&str> = item["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let required: Vec<&str> = item["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    let severities = &item["properties"]["severity"]["enum"];
    for sev in [Severity::Error, Severity::Warning, Severity::Info] {
        let r = Report::new(ErrorCode::new("SYRP-CLI-0001"), "m")
            .with_severity(sev)
            .with_location("l")
            .with_hint("h");
        let v = serde_json::to_value(&r).unwrap();
        let obj = v.as_object().unwrap();
        for key in obj.keys() {
            assert!(allowed.contains(&key.as_str()), "extra key {key}");
        }
        for key in &required {
            assert!(obj.contains_key(*key), "missing key {key}");
        }
        assert!(severities.as_array().unwrap().contains(&v["severity"]));
    }
}

#[test]
fn b4_parse_reads_entries_and_optional_example() {
    let src = format!("{ENTRY}example = \"e\"\n");
    let entries = catalogue::parse(&src).unwrap();
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    assert_eq!(e.code, "SYRP-RT-0001");
    assert_eq!(e.title, "Queue full");
    assert_eq!(e.severity, Severity::Error);
    assert_eq!((e.cause.as_str(), e.fix.as_str()), ("c", "f"));
    assert_eq!(e.example.as_deref(), Some("e"));
    assert_eq!(e.domain(), "RT");
    assert_eq!(catalogue::parse("# only a comment\n"), Ok(vec![]));
}

#[test]
fn b4_parse_rejects_duplicate_codes() {
    let err = catalogue::parse(&format!("{ENTRY}{ENTRY}")).unwrap_err();
    assert_eq!(err, ParseError::DuplicateCode("SYRP-RT-0001".into()));
    assert_eq!(err.code().as_str(), "SYRP-ERROR-0005");
}

#[test]
fn b4_parse_rejects_malformed_codes() {
    let err = catalogue::parse(&ENTRY.replace("SYRP-RT-0001", "AUR-RT-0001")).unwrap_err();
    assert_eq!(err, ParseError::MalformedCode("AUR-RT-0001".into()));
    assert_eq!(err.code().as_str(), "SYRP-ERROR-0003");
}

#[test]
fn b4_parse_rejects_unknown_severities() {
    let err = catalogue::parse(&ENTRY.replace("\"error\"", "\"fatal\"")).unwrap_err();
    assert_eq!(
        err,
        ParseError::UnknownSeverity {
            code: "SYRP-RT-0001".into(),
            severity: "fatal".into()
        }
    );
    assert_eq!(err.code().as_str(), "SYRP-ERROR-0004");
}

#[test]
fn b4_parse_rejects_bad_structure() {
    let missing = catalogue::parse(&ENTRY.replace("fix = \"f\"", "")).unwrap_err();
    assert!(
        matches!(missing, ParseError::InvalidEntry { index: 1, .. }),
        "{missing}"
    );
    let typo = catalogue::parse(&format!("{ENTRY}exmple = \"e\"\n")).unwrap_err();
    assert!(typo.to_string().contains("exmple"), "{typo}");
    let wrong_type = catalogue::parse(&ENTRY.replace("\"Queue full\"", "3")).unwrap_err();
    assert!(wrong_type.to_string().contains("title"), "{wrong_type}");
    for src in ["[[error", "error = 1", "other = 1", "error = [1]"] {
        let err = catalogue::parse(src).unwrap_err();
        assert!(
            matches!(err, ParseError::Toml(_) | ParseError::InvalidEntry { .. }),
            "{src}: {err}"
        );
        assert!(err.hint().is_some());
        assert!(err.code().as_str().starts_with("SYRP-ERROR-"));
    }
}

#[test]
fn b4_render_markdown_makes_one_page_per_domain() {
    let src = format!(
        "{ENTRY}example = \"ex\"\n{}{}",
        ENTRY.replace("SYRP-RT-0001", "SYRP-CLI-0009"),
        ENTRY.replace("SYRP-RT-0001", "SYRP-RT-0002")
    );
    let page = catalogue::render_markdown(&catalogue::parse(&src).unwrap());
    let headings: Vec<&str> = page.lines().filter(|l| l.starts_with("# ")).collect();
    assert_eq!(headings, ["# RT errors", "# CLI errors"]);
    let rt = page.find("# RT errors").unwrap();
    let cli = page.find("# CLI errors").unwrap();
    let rt1 = page.find("## SYRP-RT-0001: Queue full").unwrap();
    let rt2 = page.find("## SYRP-RT-0002: Queue full").unwrap();
    let cli9 = page.find("## SYRP-CLI-0009: Queue full").unwrap();
    assert!(rt < rt1 && rt1 < rt2 && rt2 < cli && cli < cli9);
    assert!(page.contains("**Severity:** error"));
    assert!(page.contains("**Example.** ex"));
    assert_eq!(catalogue::render_markdown(&[]), "");
}

#[test]
fn b5_errors_are_send_sync_static() {
    fn assert_bounds<T: Send + Sync + 'static>() {}
    assert_bounds::<ErrorCode>();
    assert_bounds::<Severity>();
    assert_bounds::<Report>();
    assert_bounds::<ParseError>();
    assert_bounds::<catalogue::Entry>();
}

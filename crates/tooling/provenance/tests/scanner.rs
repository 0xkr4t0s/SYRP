// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B2-B5: scanner, database format and report, on synthetic corpora only.

use syrp_provenance::database::{Database, SCHEMA};
use syrp_provenance::report::{Verdict, scan, score_file};
use syrp_provenance::scanner::{Dialect, fingerprints, tokenize};

/// A synthetic "reference" corpus written for these tests. It is not derived from any
/// third-party code.
const REFERENCE: &str = r#"
fn blend_frames(left: &[f32], right: &[f32], out: &mut [f32], weight: f32) {
    for index in 0..out.len() {
        out[index] = left[index] * (1.0 - weight) + right[index] * weight;
    }
}
"#;

/// The same code with renamed identifiers, changed literals, comments and reformatting.
const DISGUISED: &str = r#"
// Mixes two buffers.
fn BLEND_FRAMES(Left: &[f32], Right: &[f32], Out: &mut [f32], Weight: f32) {
    for Index in 0..Out.len() { Out[Index] = Left[Index] * (2.5 - Weight)
        + Right[Index] * Weight; /* tail */ }
}
"#;

const UNRELATED: &str = r#"
struct Counter { hits: u64 }
impl Counter {
    fn bump(&mut self) -> u64 { self.hits += 1; self.hits }
    fn reset(&mut self) { self.hits = 0; }
}
"#;

fn texts(src: &str) -> Vec<String> {
    tokenize(src, Dialect::Rust)
        .into_iter()
        .map(|t| t.text)
        .collect()
}

#[test]
fn b2_identifiers_are_lowercased_and_literals_bucketed() {
    assert_eq!(
        texts("Foo_Bar 12 3.5 1e-3 7u8 0xFF 2f32 \"s\" b\"b\" r#\"raw\"# 'c' '\\n' b'x'"),
        [
            "foo_bar", "<int>", "<flt>", "<flt>", "<int>", "<int>", "<flt>", "<str>", "<str>",
            "<str>", "<chr>", "<chr>", "<chr>"
        ]
    );
    assert_eq!(
        texts("x.0.1 1..2"),
        ["x", ".", "<int>", ".", "<int>", "<int>", ".", ".", "<int>"]
    );
}

#[test]
fn b2_comments_whitespace_lifetimes_and_raw_identifiers() {
    assert_eq!(
        texts("a /* x /* nested */ y */ b // tail\n<'Life> r#Type"),
        ["a", "b", "<", "'life", ">", "type"]
    );
    let lines: Vec<u32> = tokenize("a\n\n b\n/*\n*/ c", Dialect::Rust)
        .iter()
        .map(|t| t.line)
        .collect();
    assert_eq!(lines, [1, 3, 5]);
}

#[test]
fn b2_fingerprints_survive_renaming_and_reformatting() {
    let a: Vec<u64> = fingerprints(REFERENCE, 8, 4, Dialect::Rust)
        .iter()
        .map(|f| f.hash)
        .collect();
    let b: Vec<u64> = fingerprints(DISGUISED, 8, 4, Dialect::Rust)
        .iter()
        .map(|f| f.hash)
        .collect();
    assert!(!a.is_empty());
    assert_eq!(a, b);
}

#[test]
fn b2_winnowing_guarantees_a_shared_fingerprint_for_long_matches() {
    let (k, w) = (5, 4);
    // A shared run of k + w - 1 tokens, embedded in different surroundings.
    let shared = "p q r s t u v w";
    let one = format!("alpha beta gamma {shared} delta");
    let two = format!("omega {shared} zeta eta theta iota");
    let h1: Vec<u64> = fingerprints(&one, k, w, Dialect::Rust)
        .iter()
        .map(|f| f.hash)
        .collect();
    let h2: Vec<u64> = fingerprints(&two, k, w, Dialect::Rust)
        .iter()
        .map(|f| f.hash)
        .collect();
    assert!(h1.iter().any(|h| h2.contains(h)));
    // Winnowing keeps fewer fingerprints than there are n-grams.
    let long: String = (0..200).map(|i| format!("t{i} ")).collect();
    assert!(fingerprints(&long, k, w, Dialect::Rust).len() < 200 - k + 1);
}

#[test]
fn b2_similarity_score_per_file() {
    let db = Database::from_sources(&[REFERENCE], 8, 4, Dialect::Rust);
    assert_eq!(score_file(&db, "d.rs", DISGUISED, 0.5).score, 1.0);
    assert_eq!(score_file(&db, "u.rs", UNRELATED, 0.5).score, 0.0);
    let half = format!("{REFERENCE}\n{UNRELATED}");
    let f = score_file(&db, "h.rs", &half, 0.5);
    assert!(f.score > 0.2 && f.score < 0.9, "score {}", f.score);
    assert_eq!(score_file(&db, "e.rs", "", 0.5).fingerprints, 0);
}

#[test]
fn b3_database_round_trips_and_holds_hashes_only() {
    let db = Database::from_sources(&[REFERENCE], 8, 4, Dialect::Rust);
    let json = db.to_json();
    assert!(json.contains(SCHEMA));
    assert!(!json.to_lowercase().contains("blend_frames"));
    assert!(!json.contains("weight"));
    assert_eq!(Database::from_json(&json).unwrap(), db);
}

#[test]
fn b2_cpp_block_comments_do_not_nest() {
    let cpp = "/* Loads every file in dir/* and plays it. */\n\
               int gain = 1; float mix(float a, float b) { return a * 0.5f + b * 0.5f; }\n";
    let tokens = tokenize(cpp, Dialect::Cpp);
    assert_eq!(
        tokens.first().map(|t| (t.text.as_str(), t.line)),
        Some(("int", 2))
    );
    assert!(tokens.len() > 20, "{} tokens", tokens.len());
    // Rust block comments do nest, so the same text is one unterminated comment there.
    assert!(tokenize(cpp, Dialect::Rust).is_empty());
    let nested = "/* outer /* inner */ still comment */ fn x() {}";
    assert_eq!(texts(nested)[0], "fn");
}

#[test]
fn b3_database_rejects_invalid_or_incompatible_input() {
    let ok = r#"{"schema":"syrp.cleanroom-fingerprints/v1","normaliser":"syrp-cfamily-v2","k":4,"w":2,"hashes":["00000000000000aa"]}"#;
    assert!(Database::from_json(ok).is_ok());
    for bad in [
        "not json",
        &ok.replace("/v1", "/v9"),
        &ok.replace("cfamily-v2", "other"),
        &ok.replace("cfamily-v2", "cfamily-v1"),
        &ok.replace("\"k\":4", "\"k\":0"),
        // A database that could never match must fail closed, not pass every file.
        &ok.replace("\"k\":4", "\"k\":1000000"),
        &ok.replace("\"k\":4", "\"k\":65"),
        &ok.replace("\"w\":2", "\"w\":65"),
        &ok.replace("[\"00000000000000aa\"]", "[]"),
        &ok.replace("00000000000000aa", "12"),
        &ok.replace("00000000000000aa", "ABCDEF0123456789"),
    ] {
        assert_eq!(
            Database::from_json(bad).unwrap_err().code(),
            "SYRP-CR-0002",
            "{bad}"
        );
    }
}

#[test]
fn b4_report_json_has_scores_counts_and_verdicts() {
    let db = Database::from_sources(&[REFERENCE], 8, 4, Dialect::Rust);
    let files = vec![
        ("crates/a/src/copy.rs".to_string(), DISGUISED.to_string()),
        ("crates/a/src/own.rs".to_string(), UNRELATED.to_string()),
    ];
    let r = scan(&db, &files, 0.3);
    assert_eq!(r.verdict, Verdict::Fail);
    let v: serde_json::Value = serde_json::from_str(&r.to_json()).unwrap();
    assert_eq!(v["schema"], "syrp.cleanroom-report/v1");
    assert_eq!(v["threshold"], 0.3);
    assert_eq!(v["verdict"], "fail");
    let copy = &v["files"][0];
    assert_eq!(copy["verdict"], "fail");
    assert_eq!(copy["score"], 1.0);
    assert!(copy["matched"].as_u64().unwrap() > 0);
    assert_eq!(copy["matched"], copy["fingerprints"]);
    assert_eq!(copy["matched_lines"], serde_json::json!([[3, 6]]));
    assert_eq!(v["files"][1]["verdict"], "pass");
    assert_eq!(v["files"][1]["matched"], 0);

    let findings = r.findings();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].code, "SYRP-CR-0001");
    assert_eq!(findings[0].subject, "crates/a/src/copy.rs");
}

#[test]
fn b4_threshold_is_strictly_above() {
    let db = Database::from_sources(&[REFERENCE], 8, 4, Dialect::Rust);
    let files = [("d.rs".to_string(), DISGUISED.to_string())];
    assert!(scan(&db, &files, 1.0).passed());
    assert!(!scan(&db, &files, 0.99).passed());
}

#[test]
fn b4_failure_code_is_catalogued() {
    let catalogue = include_str!("../../../../errors/cleanroom.toml");
    let table: toml::Table = catalogue.parse().unwrap();
    let codes: Vec<&str> = table["error"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["code"].as_str().unwrap())
        .collect();
    for code in [
        "SYRP-CR-0001",
        "SYRP-CR-0002",
        "SYRP-CR-0003",
        "SYRP-CR-0004",
    ] {
        assert!(codes.contains(&code), "{code}");
    }
    for src in [
        include_str!("../src/lib.rs"),
        include_str!("../src/provenance.rs"),
        include_str!("../src/report.rs"),
    ] {
        for (i, _) in src.match_indices("SYRP-CR-") {
            let code = &src[i..i + 12];
            if !code[8..].bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
            assert!(codes.contains(&code), "{code} not in errors/cleanroom.toml");
        }
    }
}

#[test]
fn b5_tests_use_inline_synthetic_corpora_only() {
    // No fixture files: every corpus is a string literal written in this test file.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        assert!(
            path.is_file(),
            "unexpected fixture directory {}",
            path.display()
        );
        assert_eq!(path.extension().and_then(|e| e.to_str()), Some("rs"));
    }
}

#[test]
fn b4_nan_threshold_fails_closed() {
    // An unrelated file passes at a real threshold but must not pass a NaN one.
    let db = Database::from_sources(&[REFERENCE], 8, 4, Dialect::Rust);
    let files = [("u.c".to_owned(), UNRELATED.to_owned())];
    assert!(scan(&db, &files, 0.5).passed());
    assert!(!scan(&db, &files, f64::NAN).passed());
}

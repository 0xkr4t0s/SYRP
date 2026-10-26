// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B1: trailer and SPDX checks, including over a throwaway git repository.

use std::path::{Path, PathBuf};
use std::process::Command;

use syrp_provenance::provenance::{check_commit, check_range, check_spdx, crate_licence};

const TRAILERS: &str = "\n\nWP: WP-9.9-99\nAgent-Model: test-model\n";

struct Repo(PathBuf);

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Repo {
    fn new(name: &str) -> Repo {
        let dir = std::env::temp_dir().join(format!("syrp-prov-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let repo = Repo(dir);
        repo.git(&["init", "-q", "-b", "main"]);
        repo
    }

    fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.0)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    }

    fn write(&self, rel: &str, text: &str) {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    fn commit(&self, message: &str) -> String {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "--allow-empty", "-m", message]);
        self.git(&["rev-parse", "HEAD"])
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

/// A workspace with one crate inheriting its licence and one declaring its own.
fn workspace(name: &str) -> Repo {
    let r = Repo::new(name);
    r.write("Cargo.toml", "[workspace]\nmembers = [\"crates/*/*\"]\n[workspace.package]\nlicense = \"AGPL-3.0-only OR BUSL-1.1\"\n");
    r.write(
        "crates/a/inh/Cargo.toml",
        "[package]\nname = \"inh\"\nlicense.workspace = true\n",
    );
    r.write(
        "crates/a/own/Cargo.toml",
        "[package]\nname = \"own\"\nlicense = \"MIT\"\n",
    );
    r.write("crates/a/inh/src/old.rs", "fn old() {}\n");
    r.commit("Base");
    r
}

#[test]
fn b1_trailers_required_on_every_commit() {
    assert!(check_commit("s", &format!("Subject{TRAILERS}")).is_empty());
    assert!(check_commit("s", "Subject\n\nChange-Request: #4\nAgent-Model: m").is_empty());
    let missing_model = check_commit("s", "Subject\n\nWP: WP-1");
    assert_eq!(missing_model.len(), 1);
    assert!(missing_model[0].message.contains("Agent-Model"));
    let missing_wp = check_commit("s", "Subject\n\nAgent-Model: m");
    assert!(missing_wp[0].message.contains("WP"));
    // Trailers must be in the last paragraph and have values.
    assert_eq!(check_commit("s", "WP: x\nAgent-Model: m").len(), 2);
    assert_eq!(
        check_commit("s", "S\n\nWP: x\nAgent-Model: m\n\nMore body").len(),
        2
    );
    assert_eq!(check_commit("s", "S\n\nWP:\nAgent-Model: m").len(), 1);
    assert!(
        missing_wp
            .iter()
            .all(|f| f.code == "SYRP-CR-0003" && f.subject == "s")
    );
}

#[test]
fn b1_spdx_header_must_match_licence() {
    let lic = "AGPL-3.0-only OR BUSL-1.1";
    let good = "// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1\n//! Docs\n";
    assert!(check_spdx("a.rs", good, lic).is_none());
    assert!(check_spdx("a.rs", &format!("\u{feff}{good}"), lic).is_none());
    let wrong = check_spdx("a.rs", "// SPDX-License-Identifier: MIT\n", lic).unwrap();
    assert_eq!(wrong.code, "SYRP-CR-0004");
    assert!(wrong.message.contains("does not match"));
    assert!(check_spdx("a.rs", "\n// SPDX-License-Identifier: MIT\n", "MIT").is_some());
    assert!(check_spdx("a.rs", "", lic).is_some());
}

#[test]
fn b1_crate_licence_comes_from_cargo_toml() {
    let r = workspace("licence");
    let inh = crate_licence(r.path(), "crates/a/inh/src/deep/x.rs").unwrap();
    assert_eq!(inh.as_deref(), Some("AGPL-3.0-only OR BUSL-1.1"));
    let own = crate_licence(r.path(), "crates/a/own/tests/t.rs").unwrap();
    assert_eq!(own.as_deref(), Some("MIT"));
    assert_eq!(crate_licence(r.path(), "tools/x.rs").unwrap(), None);
    r.write("crates/a/bad/Cargo.toml", "[package]\nname = \"bad\"\n");
    let err = crate_licence(r.path(), "crates/a/bad/src/lib.rs").unwrap_err();
    assert_eq!(err.code(), "SYRP-CR-0006");
}

#[test]
fn b1_range_check_passes_on_clean_history() {
    let r = workspace("clean");
    let base = r.git(&["rev-parse", "HEAD"]);
    r.write(
        "crates/a/inh/src/new.rs",
        "// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1\nfn f() {}\n",
    );
    r.write(
        "crates/a/own/src/lib.rs",
        "// SPDX-License-Identifier: MIT\n",
    );
    r.write(
        "crates/a/inh/src/old.rs",
        "fn old() { /* edited, no header needed */ }\n",
    );
    r.write("README.md", "not source\n");
    r.commit(&format!("Add files{TRAILERS}"));
    let report = check_range(r.path(), &format!("{base}..HEAD")).unwrap();
    assert!(report.passed(), "{:?}", report.findings);
    assert_eq!((report.commits_checked, report.files_checked), (1, 2));
}

#[test]
fn b1_range_check_reports_bad_commits_and_headers() {
    let r = workspace("dirty");
    let base = r.git(&["rev-parse", "HEAD"]);
    r.write("crates/a/inh/src/no_header.rs", "fn f() {}\n");
    let bad = r.commit("No trailers here");
    r.write(
        "crates/a/own/src/wrong.rs",
        "// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1\n",
    );
    r.commit(&format!("Second{TRAILERS}"));
    let report = check_range(r.path(), &format!("{base}..HEAD")).unwrap();
    assert_eq!(report.commits_checked, 2);
    let summary: Vec<(&str, &str)> = report
        .findings
        .iter()
        .map(|f| (f.code.as_str(), f.subject.as_str()))
        .collect();
    assert_eq!(
        summary,
        [
            ("SYRP-CR-0003", bad.as_str()),
            ("SYRP-CR-0003", bad.as_str()),
            ("SYRP-CR-0004", "crates/a/inh/src/no_header.rs"),
            ("SYRP-CR-0004", "crates/a/own/src/wrong.rs"),
        ]
    );
}

#[test]
fn b1_bad_range_is_a_git_error() {
    let r = workspace("badrange");
    let err = check_range(r.path(), "nope..HEAD").unwrap_err();
    assert_eq!(err.code(), "SYRP-CR-0005");
}

#[test]
fn b1_option_like_and_empty_ranges_are_rejected() {
    let r = workspace("optrange");
    let out = r.path().join("injected");
    for range in [
        format!("--output={}", out.display()),
        "-p".to_owned(),
        String::new(),
        "  ".to_owned(),
    ] {
        let err = check_range(r.path(), &range).unwrap_err();
        assert_eq!(err.code(), "SYRP-CR-0005", "{range:?}");
    }
    assert!(!out.exists(), "git must never see the range as an option");
}

#[test]
fn b1_range_with_no_commits_fails() {
    let r = workspace("emptyrange");
    let err = check_range(r.path(), "HEAD..HEAD").unwrap_err();
    assert_eq!(err.code(), "SYRP-CR-0005");
}

#[test]
fn b1_every_code_used_is_catalogued() {
    let table: toml::Table = include_str!("../../../../errors/cleanroom.toml")
        .parse()
        .unwrap();
    let codes: Vec<&str> = table["error"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["code"].as_str().unwrap())
        .collect();
    for src in [
        include_str!("../src/lib.rs"),
        include_str!("../src/provenance.rs"),
    ] {
        for (i, _) in src.match_indices("SYRP-CR-") {
            let code = &src[i..i + 12];
            if code[8..].bytes().all(|b| b.is_ascii_digit()) {
                assert!(codes.contains(&code), "{code} not in errors/cleanroom.toml");
            }
        }
    }
}

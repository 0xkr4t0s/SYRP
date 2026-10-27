// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! WP-0.2-16 behaviours, run against a small fixture repository built in a temp directory.

use std::path::{Path, PathBuf};
use std::process::Command;

use syrp_progress::{ClassStatus, Diagnostic, OpenPr, WpStatus};

fn write(root: &Path, rel: &str, text: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn git(root: &Path, args: &[&str], date: &str) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .env("GIT_AUTHOR_DATE", date)
        .env("GIT_COMMITTER_DATE", date)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

/// A repo with two waves (the second disabled), three packages and two merged PRs.
fn fixture(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("syrp-progress-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    write(
        &root,
        "work/waves/0.1.toml",
        "id = \"0.1\"\nphase = 0\ntitle = \"First\"\npackages = [\"WP-0.1-01\", \"WP-0.1-02\", \"WP-0.1-03\"]\n",
    );
    write(
        &root,
        "work/waves/1.9.toml",
        "id = \"1.9\"\nphase = 1\ntitle = \"Later\"\nenabled = false\npackages = [\"WP-1.9-01\"]\n",
    );
    // 1.10 sorts before 1.9 as text, so it also proves the wave order is numeric.
    write(
        &root,
        "work/waves/1.10.toml",
        "id = \"1.10\"\nphase = 1\ntitle = \"Last\"\nenabled = false\npackages = [\"WP-1.10-01\"]\n",
    );
    write(
        &root,
        "work/packages/WP-0.1-01.toml",
        "id = \"WP-0.1-01\"\ntitle = \"Done one\"\nstatus = \"done\"\nbudget = { agent_hours = 24 }\n",
    );
    write(
        &root,
        "work/packages/WP-0.1-02.toml",
        "id = \"WP-0.1-02\"\ntitle = \"Merged increment\"\ncrate = \"crates/x\"\n",
    );
    write(
        &root,
        "work/packages/WP-0.1-03.toml",
        "id = \"WP-0.1-03\"\ntitle = \"Untouched\"\n",
    );
    write(
        &root,
        "work/packages/WP-1.9-01.toml",
        "id = \"WP-1.9-01\"\ntitle = \"Parked\"\n",
    );
    write(
        &root,
        "work/packages/WP-1.10-01.toml",
        "id = \"WP-1.10-01\"\ntitle = \"Parked too\"\n",
    );
    write(
        &root,
        "parity/inventory/juce-9.0.4-classes.json",
        r#"{"modules": [
        {"module": "juce_a", "complete": true, "classes": [{"class": "Done"}, {"class": "Mapped"}, {"class": "Gap"}, {"class": "Owned"}, {"class": "OwnedIdle"}, {"class": "Partial"}, {"class": "Unlisted"}]},
        {"module": "juce_b", "complete": false, "classes": []}]}"#,
    );
    write(
        &root,
        "parity/ledger.json",
        r#"{"baseline": "9.0.4", "rows": []}"#,
    );
    write(
        &root,
        "parity/inventory/ledger-seed.json",
        r#"{"rows": [
        {"id": "juce_a/Done", "juce": {"module": "juce_a", "class": "Done"}, "status": "done"},
        {"id": "juce_a/Mapped", "juce": {"module": "juce_a", "class": "Mapped"}, "status": "mapped"},
        {"id": "juce_a/Gap", "juce": {"module": "juce_a", "class": "Gap"}, "status": "gap", "reason": "r"},
        {"id": "juce_a/Owned", "juce": {"module": "juce_a", "class": "Owned"}, "status": "todo", "owner_wp": "WP-0.1-02"},
        {"id": "juce_a/OwnedIdle", "juce": {"module": "juce_a", "class": "OwnedIdle"}, "status": "todo", "owner_wp": "WP-0.1-03"},
        {"id": "juce_a/Partial", "juce": {"module": "juce_a", "class": "Partial"}, "status": "partial"}]}"#,
    );
    git(
        &root,
        &["init", "-q", "-b", "main"],
        "2026-10-26T09:00:00+11:00",
    );
    git(&root, &["add", "."], "2026-10-26T09:00:00+11:00");
    git(
        &root,
        &["commit", "-qm", "Initial commit"],
        "2026-10-26T09:00:00+11:00",
    );
    git(
        &root,
        &[
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "[WP-0.1-02] x: first increment (#2)",
        ],
        "2026-10-26T10:00:00+11:00",
    );
    git(
        &root,
        &[
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "board: mark done (#3)",
        ],
        "2026-10-26T23:30:00+11:00",
    );
    root
}

fn wp(p: &syrp_progress::Progress, id: &str) -> syrp_progress::Wp {
    p.waves
        .iter()
        .flat_map(|w| &w.wps)
        .find(|w| w.id == id)
        .unwrap()
        .clone()
}

#[test]
fn b1_wp_status_from_board_merges_open_prs_and_disabled_waves() {
    let root = fixture("b1");
    let open = [OpenPr {
        number: 9,
        title: "[WP-0.1-03] x: start".into(),
    }];
    let p = syrp_progress::load(&root, &[]).unwrap();
    assert_eq!(
        p.waves.iter().map(|w| w.id.as_str()).collect::<Vec<_>>(),
        ["0.1", "1.9", "1.10"],
        "waves are ordered by their numbers, not by file name"
    );
    assert_eq!(wp(&p, "WP-0.1-01").status, WpStatus::Done);
    assert_eq!(wp(&p, "WP-0.1-02").status, WpStatus::Active);
    assert_eq!(wp(&p, "WP-0.1-02").prs, [2]);
    assert_eq!(wp(&p, "WP-0.1-03").status, WpStatus::Planned);
    assert_eq!(wp(&p, "WP-1.9-01").status, WpStatus::Parked);
    assert_eq!(wp(&p, "WP-1.10-01").status, WpStatus::Parked);
    let p = syrp_progress::load(&root, &open).unwrap();
    assert_eq!(wp(&p, "WP-0.1-03").status, WpStatus::Active);
    assert_eq!(wp(&p, "WP-0.1-03").open, [9]);
}

#[test]
fn b2_class_status_from_ledger_seed_and_owner_wp() {
    let root = fixture("b2");
    let p = syrp_progress::load(&root, &[]).unwrap();
    let a = &p.modules[0];
    let st: Vec<ClassStatus> = a.cells.iter().map(|c| c.1).collect();
    use ClassStatus::*;
    assert_eq!(st, [Verified, Verified, Gap, Inwork, Todo, Inwork, Todo]);
    assert!(a.complete);
    assert!(!p.modules[1].complete && p.modules[1].cells.is_empty());
}

#[test]
fn b2_ledger_rows_win_over_the_seed_once_present() {
    let root = fixture("b2-ledger");
    write(
        &root,
        "parity/ledger.json",
        r#"{"baseline": "9.0.4", "rows": [
        {"id": "juce_a/Unlisted", "juce": {"module": "juce_a", "class": "Unlisted"}, "status": "done"}]}"#,
    );
    let p = syrp_progress::load(&root, &[]).unwrap();
    let st: Vec<ClassStatus> = p.modules[0].cells.iter().map(|c| c.1).collect();
    assert_eq!(
        st.iter().filter(|s| **s == ClassStatus::Verified).count(),
        1
    );
    assert_eq!(st[6], ClassStatus::Verified);
}

#[test]
fn b3_merged_prs_oldest_first_in_utc_with_wp() {
    let root = fixture("b3");
    let p = syrp_progress::load(&root, &[]).unwrap();
    assert_eq!(p.prs.len(), 2);
    assert_eq!(p.prs[0].n, 2);
    assert_eq!(p.prs[0].t, "2026-10-25T23:00:00Z");
    assert_eq!(p.prs[0].wp.as_deref(), Some("WP-0.1-02"));
    assert_eq!(p.prs[1].title, "board: mark done");
    assert_eq!(p.prs[1].wp, None);
    assert_eq!(p.commit.len(), 7);
}

#[test]
fn b4_page_embeds_data_and_cannot_be_broken_out_of() {
    let root = fixture("b4");
    let open = [OpenPr {
        number: 9,
        title: "[WP-0.1-03] </script><script>alert(1)".into(),
    }];
    let mut p = syrp_progress::load(&root, &open).unwrap();
    p.waves[0].wps[2].title = "</script><b>".into();
    p.waves[0].wps[1].title = r#"<img src=x onerror="alert(1)">'"#.into();
    p.waves[0].title = "<svg onload=alert(1)>".into();
    let html = syrp_progress::render_html(&p);
    assert!(html.starts_with("<!doctype html>"));
    assert!(!html.contains("/*DATA*/"));
    assert_eq!(
        html.matches("</script>").count(),
        1,
        "only the page's own closing tag"
    );
    assert!(html.contains(r#""title":"\u003c/script>\u003cb>""#));
    // The embedded data never contains a raw `<`, whatever the titles hold.
    let data = html
        .split("const D = ")
        .nth(1)
        .unwrap()
        // One line of JSON; `lines` also copes with a CRLF checkout of page.html on Windows.
        .lines()
        .next()
        .unwrap()
        .trim_end_matches(';');
    assert!(!data.contains('<'), "raw < in embedded data: {data}");
    assert!(data.contains(r#"\u003cimg src=x onerror=\"alert(1)\">'"#));
    let back: serde_json::Value = serde_json::from_str(data).unwrap();
    assert_eq!(back["waves"][0]["title"], "<svg onload=alert(1)>");
}

#[test]
fn b4_page_links_to_the_foundation_repository() {
    let html = syrp_progress::render_html(&syrp_progress::Progress::default());
    assert!(html.contains(r#"const REPO = "https://github.com/SYRP-Foundation/SYRP";"#));
    assert!(!html.contains("0xkr4t0s"), "no link to the old repository");
}

#[test]
fn b5_binary_writes_both_files_and_reports_bad_input() {
    let root = fixture("b5");
    let out = root.join("site/progress");
    let open = root.join("open.json");
    std::fs::write(&open, r#"[{"number": 9, "title": "[WP-0.1-03] y"}]"#).unwrap();
    let bin = env!("CARGO_BIN_EXE_syrp-progress");
    let run = |extra: &[&Path]| {
        let mut c = Command::new(bin);
        c.arg("--root").arg(&root).arg("--out").arg(&out);
        if let Some(f) = extra.first() {
            c.arg("--open-prs").arg(f);
        }
        c.output().unwrap()
    };
    let ok = run(&[&open]);
    assert_eq!(
        ok.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out.join("progress.json")).unwrap()).unwrap();
    assert_eq!(json["waves"][0]["wps"][2]["open"][0], 9);
    let html = std::fs::read_to_string(out.join("index.html")).unwrap();
    assert!(html.starts_with("<!doctype html>") && html.contains("\"commit\":"));
    assert!(out.join("index.html").exists());

    std::fs::write(&open, "not json").unwrap();
    let bad = run(&[&open]);
    assert_eq!(bad.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&bad.stderr).contains("SYRP-PROG-0004"));
}

#[test]
fn b5_missing_inputs_are_coded_diagnostics() {
    let root = fixture("b5-missing");
    std::fs::remove_file(root.join("parity/ledger.json")).unwrap();
    let e = syrp_progress::load(&root, &[]).unwrap_err();
    assert_eq!(e.code().as_str(), "SYRP-PROG-0002");
    write(
        &root,
        "work/waves/2.0.toml",
        "id = \"2.0\"\ntitle = \"x\"\npackages = [\"WP-9.9-99\"]\n",
    );
    let e = syrp_progress::load(&root, &[]).unwrap_err();
    assert_eq!(e.code().as_str(), "SYRP-PROG-0001");
    let e =
        syrp_progress::load(&std::env::temp_dir().join("syrp-progress-nowhere"), &[]).unwrap_err();
    assert_eq!(e.code().as_str(), "SYRP-PROG-0003");
}

#[test]
fn b6_same_inputs_give_identical_output() {
    let root = fixture("b6");
    let a = syrp_progress::load(&root, &[]).unwrap();
    let b = syrp_progress::load(&root, &[]).unwrap();
    assert_eq!(
        serde_json::to_string(&a).unwrap(),
        serde_json::to_string(&b).unwrap()
    );
    assert_eq!(
        syrp_progress::render_html(&a),
        syrp_progress::render_html(&b)
    );
}

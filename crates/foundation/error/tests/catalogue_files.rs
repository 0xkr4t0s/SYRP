//! Every real `errors/*.toml` in the repository parses (spec acceptance, B4).

use std::path::Path;

#[test]
fn b4_every_repository_catalogue_file_parses() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../errors");
    let mut files = 0;
    for entry in std::fs::read_dir(&dir).expect("errors/ exists") {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "toml") {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
        let entries = syrp_error::catalogue::parse(&src)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(!entries.is_empty(), "{} has no entries", path.display());
        // One domain per file, so the file renders as exactly one page.
        let domain = entries[0].domain();
        for e in &entries {
            assert_eq!(e.domain(), domain, "{} mixes domains", path.display());
        }
        let page = syrp_error::catalogue::render_markdown(&entries);
        assert!(page.starts_with(&format!("# {domain} errors")));
        assert_eq!(page.lines().filter(|l| l.starts_with("# ")).count(), 1);
        files += 1;
    }
    assert!(files >= 1, "no catalogue files found in {}", dir.display());
}

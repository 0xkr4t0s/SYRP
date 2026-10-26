# WP-0.1-12: Clean-room contamination scanner (quarantined job) and provenance checks

## Purpose
Clean-room enforcement (master plan §10.9). Two parts: provenance checks that run on every PR, and a similarity scanner that runs in a quarantined job.

## Behaviour
- B1. Provenance: a library check that every commit in a range carries `WP:` (or `Change-Request:`) and `Agent-Model:` trailers, and every new source file starts with an SPDX header matching its crate's licence.
- B2. Scanner core: tokenise Rust source into normalised token n-grams (identifiers lowercased, literals bucketed) and compute winnowing fingerprints. Given a fingerprint database file, report the similarity score per file.
- B3. The scanner never reads JUCE source itself. A separate quarantined CI job (written later by the PR manager) produces the fingerprint database from JUCE source; this crate only consumes the database format, which you define and document.
- B4. Report format: JSON with per-file score, matched fingerprint count and threshold verdict. Scores above the threshold fail with `SYRP-CR-0001` (add `errors/cleanroom.toml`).
- B5. Tests use synthetic corpora only.

## Crate setup
Create `crates/tooling/provenance/` with `Cargo.toml` (inherit `version`, `edition`, `rust-version`, `repository`, `authors`, `publish` from the workspace; `[lints] workspace = true`), `[package.metadata.syrp] layer = "L6"`, an `AGENTS.md` following `docs/agents/crate-agents-template.md`, and `src/lib.rs`. Workspace members are globbed, so no root edit is needed.
Licence: `license = "AGPL-3.0-only OR BUSL-1.1"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-0.1-12` first and `cargo syrp wp check WP-0.1-12` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

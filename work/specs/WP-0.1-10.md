# WP-0.1-10: Docs pipeline v1: rustdoc JSON to api-index.json, docs search over the index

## Purpose
The machine-readable API index for agents (master plan §13.1): build `api-index.json` from rustdoc JSON and search it offline.

## Behaviour
- B1. A library function that reads rustdoc JSON (`rustdoc-types`) for each workspace crate and emits `api-index.json`: one entry per public item with path, kind, signature, first doc paragraph, `doc(alias)` values (JUCE names) and source location.
- B2. Publish the entry shape as `docs/agents/api-index.schema.json` (generated with `schemars`, committed).
- B3. Producing rustdoc JSON needs nightly (`-Z unstable-options --output-format json`); detect that and return a clear diagnostic with code `AUR-DOC-0001` (add `errors/doc.toml`) when the toolchain is missing.
- B4. A search function ranking items by name, alias and doc text; a later CLI WP wires it into `cargo auricle docs search`.
- B5. A per-crate slice writer for `llms-<crate>.txt` limited to a token budget (approximate tokens as bytes/4).

## Crate setup
Create `crates/tooling/docgen/` with `Cargo.toml` (inherit `version`, `edition`, `rust-version`, `repository`, `authors` from the workspace; `[lints] workspace = true`), `[package.metadata.auricle] layer = "L6"`, an `AGENTS.md` following `docs/agents/crate-agents-template.md`, and `src/lib.rs`. Workspace members are globbed, so no root edit is needed.
Licence: `license = "AGPL-3.0-only OR BUSL-1.1"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo auricle wp show WP-0.1-10` first and `cargo auricle wp check WP-0.1-10` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

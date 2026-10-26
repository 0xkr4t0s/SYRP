# syrp-docgen

## Purpose
The agent-facing docs pipeline (master plan §13.1). Reads rustdoc JSON with `rustdoc-types`
and builds `api-index.json`: one entry per public item with path, kind, one-line signature,
first doc paragraph, `doc(alias)` values (JUCE names), deprecation and source location.
`api_index_schema()` returns the index's JSON Schema (schemars). `search` ranks entries by
name, alias (JUCE names) and doc text for `cargo syrp docs search`; `write_crate_slice` writes
`llms-<crate>.txt` within a token budget (tokens = bytes / 4, never exceeded).
v2 (WP-0.2-12): `write_crate_brief` (AGENTS.md body plus crate-level API, at most
`BRIEF_TOKEN_BUDGET` = 1,500 tokens); `ErrorCatalogue` (`errors/*.toml` to `errors.json` and
one Markdown page per domain); `MigrationMap` (`docs/migration/*.md` front matter to
`migration-map.json`); `check_refs` (fails on cited `syrp*` paths missing from the index).

## Ledger rows
Tooling rows only (no JUCE class).

## Invariants
- `rustdoc-types` 0.N reads rustdoc JSON format N only; any other `format_version` is
  `SYRP-DOC-0002`, never a parse error. Bump it with the nightly, then refresh the fixture.
- Entries are sorted (crate, path, kind) so a freshness gate can diff the output.
- Only public items: private, `doc(hidden)` and trait-impl members are left out.
- Codes `SYRP-DOC-0001..0007` are in `errors/doc.toml`; `DocError` implements `Diagnostic`.
- Every output is deterministic and sorted (errors by code, migration entries by `juce`).
- Briefs and slices never exceed their budget; AGENTS.md must fit a brief (a test checks ours).

## Thread affinity
Build-time tooling, any thread. `detect_nightly` spawns `rustup`; `generate_rustdoc_json`
spawns `cargo +nightly rustdoc`.

## Stability
experimental (0.x). Shapes: `syrp.api-index/v1`, `syrp.errors/v1`, `syrp.migration-map/v1`.

## How to test
`cargo syrp test --crate syrp-docgen`. With nightly installed,
`b3_nightly_round_trip_when_installed` regenerates the fixture's JSON and checks it indexes
identically. `tests/fixtures/demo.json` is the rustdoc JSON of `tests/fixtures/demo`
(`cargo +nightly rustdoc --lib -- -Z unstable-options --output-format json` there), minus other
crates' `paths`, `external_crates`, target features and synthetic impls. v2 tests
(`tests/v2_outputs.rs`) write their inputs under `CARGO_TARGET_TMPDIR` and also load the real
`errors/` and `docs/migration/`.

## Known gaps
- CLI wiring (`cargo syrp docs search`, `explain SYRP-DOC-*`) and committing
  `docs/agents/api-index.schema.json` need change requests (a test checks it once present).
- Nothing writes the outputs to disk or runs `check-refs` in CI yet: wiring `cargo syrp docs
  brief`, the freshness gate and `check-refs` into the CLI needs change requests. Briefs do not
  yet list the crate's error codes (no crate-to-domain map).
- Migration front matter is single-line `key: value` only (no YAML blocks or method maps).
- `check-refs` treats any `syrp`/`syrp_*`/indexed-crate path as a citation, so planned crates
  (e.g. `syrp_widgets_slider` in `docs/migration/README.md`) fail until they exist.
- No `since` field yet; tuple-struct fields are not indexed.
- `pub use` re-exports of items from other crates are dropped (only same-crate targets are
  resolved), so a facade crate's re-exported API is missing from its index.
- The summary is the first doc paragraph verbatim: a doc comment that opens with a code fence
  gets that fence as its summary.

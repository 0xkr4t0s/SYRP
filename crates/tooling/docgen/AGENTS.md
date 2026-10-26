# syrp-docgen

## Purpose
The agent-facing docs pipeline (master plan §13.1). Reads rustdoc JSON with `rustdoc-types`
and builds `api-index.json`: one entry per public item with path, kind, one-line signature,
first doc paragraph, `doc(alias)` values (JUCE names), deprecation and source location.
`api_index_schema()` returns the index's JSON Schema (schemars).

## Ledger rows
Tooling rows only (no JUCE class).

## Invariants
- `rustdoc-types` 0.N reads rustdoc JSON format N only; any other `format_version` is
  `SYRP-DOC-0002`, never a parse error. Bump it with the nightly, then refresh the fixture.
- Entries are sorted (crate, path, kind) so a freshness gate can diff the output.
- Only public items: private, `doc(hidden)` and trait-impl members are left out.
- Codes `SYRP-DOC-0001..0003` are in `errors/doc.toml`; `DocError` implements `Diagnostic`.

## Thread affinity
Build-time tooling, any thread. `detect_nightly` spawns `rustup`.

## Stability
experimental (0.x). The `api-index.json` shape is `syrp.api-index/v1`.

## How to test
`cargo syrp test --crate syrp-docgen`. `tests/fixtures/demo.json` is the rustdoc JSON of
`tests/fixtures/demo` (`cargo +nightly rustdoc --lib -- -Z unstable-options --output-format json`
there), minus other crates' `paths`, `external_crates`, target features and synthetic impls.

## Known gaps
- B4 search, B5 `llms-<crate>.txt` slices, and running rustdoc for the workspace: next increment.
- CLI wiring (`cargo syrp docs search`, `explain SYRP-DOC-*`) and committing
  `docs/agents/api-index.schema.json` need change requests (a test checks it once present).
- No `since` field yet; tuple-struct fields are not indexed.

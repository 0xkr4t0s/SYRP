# syrp-docgen

## Purpose
The agent-facing docs pipeline (master plan §13.1). Reads rustdoc JSON with `rustdoc-types`
and builds `api-index.json`: one entry per public item with path, kind, one-line signature,
first doc paragraph, `doc(alias)` values (JUCE names), deprecation and source location.
`api_index_schema()` returns the index's JSON Schema (schemars). `search` ranks entries by
name, alias (JUCE names) and doc text for `cargo syrp docs search`; `write_crate_slice` writes
`llms-<crate>.txt` within a token budget (tokens = bytes / 4, never exceeded).

## Ledger rows
Tooling rows only (no JUCE class).

## Invariants
- `rustdoc-types` 0.N reads rustdoc JSON format N only; any other `format_version` is
  `SYRP-DOC-0002`, never a parse error. Bump it with the nightly, then refresh the fixture.
- Entries are sorted (crate, path, kind) so a freshness gate can diff the output.
- Only public items: private, `doc(hidden)` and trait-impl members are left out.
- Codes `SYRP-DOC-0001..0003` are in `errors/doc.toml`; `DocError` implements `Diagnostic`.

## Thread affinity
Build-time tooling, any thread. `detect_nightly` spawns `rustup`; `generate_rustdoc_json`
spawns `cargo +nightly rustdoc`.

## Stability
experimental (0.x). The `api-index.json` shape is `syrp.api-index/v1`.

## How to test
`cargo syrp test --crate syrp-docgen`. With nightly installed,
`b3_nightly_round_trip_when_installed` regenerates the fixture's JSON and checks it indexes
identically. `tests/fixtures/demo.json` is the rustdoc JSON of `tests/fixtures/demo`
(`cargo +nightly rustdoc --lib -- -Z unstable-options --output-format json` there), minus other
crates' `paths`, `external_crates`, target features and synthetic impls.

## Known gaps
- CLI wiring (`cargo syrp docs search`, `explain SYRP-DOC-*`) and committing
  `docs/agents/api-index.schema.json` need change requests (a test checks it once present).
- No `since` field yet; tuple-struct fields are not indexed.
- `pub use` re-exports of items from other crates are dropped (only same-crate targets are
  resolved), so a facade crate's re-exported API is missing from its index.
- The summary is the first doc paragraph verbatim: a doc comment that opens with a code fence
  gets that fence as its summary.

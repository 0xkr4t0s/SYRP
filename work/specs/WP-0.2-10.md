# WP-0.2-10: syrp-mcp v0: MCP server over the syrp-cli library

## Purpose
An MCP server exposing the same closed loop as `cargo syrp` (master plan §13.3), calling the `syrp-cli` library so results are identical.

## Behaviour
- B1. Built on `rmcp`, stdio transport (HTTP later). Binary `syrp-mcp`.
- B2. Tools: `doctor`, `check`, `test`, `explain`, `docs_search`, `wp_show`, `wp_ready`, `review_checklist`. Each returns the CLI envelope JSON as structured content.
- B3. Resources: `syrp://errors/{code}`, `syrp://wp/{id}`.
- B4. Project-internal tools (`wp_*`, `review_*`) are behind a feature `internal`, default on in this repo.
- B5. Integration test that starts the server in-process and calls every tool.

## Crate setup
Create `crates/tooling/mcp/` with `Cargo.toml` (inherit `version`, `edition`, `rust-version`, `repository`, `authors`, `publish` from the workspace; `[lints] workspace = true`), `[package.metadata.syrp] layer = "L6"`, an `AGENTS.md` following `docs/agents/crate-agents-template.md`, and `src/lib.rs`. Workspace members are globbed, so no root edit is needed.
Licence: `license = "AGPL-3.0-only OR BUSL-1.1"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-0.2-10` first and `cargo syrp wp check WP-0.2-10` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

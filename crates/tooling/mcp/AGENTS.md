# syrp-mcp

## Purpose
The MCP server (`syrp-mcp` binary, stdio transport, built on `rmcp`) that exposes the
`cargo syrp` closed loop to agents (master plan §13.3). Every tool builds a `syrp_cli::cli::Cmd`,
runs it through `syrp_cli::cli::dispatch` and returns the CLI's `syrp.envelope/v1` JSON as
structured content, so results match `cargo syrp <command> --json` exactly. Tools: `doctor`,
`check`, `test`, `explain`, `docs_search`, and (feature `internal`) `wp_show`, `wp_ready`,
`review_checklist`. Resources: `syrp://errors/{code}` and (feature `internal`) `syrp://wp/{id}`.

## Ledger rows
Tooling rows only (no JUCE class). Replaces the Projucer's role together with `syrp-cli`.

## Invariants
- No command logic here: every tool and resource goes through `syrp_cli::cli::dispatch` and
  `syrp_cli::envelope::Envelope::new`. If a result differs from the CLI, fix the CLI, not this crate.
- The tool result's structured content is the envelope; `isError` is set exactly when `ok` is false.
- Every tool declares the published envelope schema (`docs/agents/envelope.schema.json`) as its output schema.
- stdout carries only the protocol; the binary writes nothing else there.
- Project-internal tools and the `syrp://wp/{id}` resource exist only with feature `internal` (default on).

## Thread affinity
Async (tokio). CLI commands may run cargo, so they execute on the blocking pool. Not used on audio threads.

## Stability
experimental (0.x). Tool names, parameters and resource URIs follow the master plan and may grow.

## How to test
```sh
cargo syrp test --crate syrp-mcp
cargo test -p syrp-mcp --no-default-features   # public tools only
cargo syrp check
```
The integration tests (`tests/server.rs`) start the server in-process over a duplex pipe and
call every tool with a real rmcp client.

## Known gaps
HTTP transport (`--http`), the Tasks extension for long runs, and the remaining §13.3 tools and
resources (`build`, `render_*`, `parity_*`, `docs_get`, `syrp://api-index`, ...) arrive with the
WPs that add the matching CLI commands. `cargo syrp mcp serve` is not wired yet (it lives in syrp-cli).

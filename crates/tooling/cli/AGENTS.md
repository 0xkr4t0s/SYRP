# auricle-cli

## Purpose
The `cargo auricle` binary and the library behind it. Every command returns an `Outcome` that the
binary wraps in the JSON envelope (`docs/agents/envelope.schema.json`). The MCP server and the
Agent Skill call this library so their results match the CLI exactly.

## Ledger rows
Tooling rows only (no JUCE class). Replaces the Projucer's role.

## Invariants
- One envelope per invocation on stdout; nothing else is printed when output is JSON.
- Exit codes and envelope fields never change meaning. Changing them needs a `BREAKING.md` entry.
- Every `AUR-*` code used in `src/` exists in `errors/*.toml` (a unit test enforces this).
- No interactive prompts, ever.
- `conductor` and `gates` are pure functions of their inputs; all GitHub I/O stays in workflows.

## Thread affinity
Single-threaded CLI. Not used on audio threads.

## Stability
experimental (0.x). The envelope (`auricle.envelope/v1`) and exit codes are stable.

## How to test
```sh
cargo auricle test --crate auricle-cli
cargo auricle check
```

## Known gaps
Commands from master plan §13.2 not yet present: `new`, `add`, `build`, `render-audio`,
`render-ui`, `snapshot`, `parity`, `validate`, `host`, `inspect`, `rt-check`, `bench`, `fuzz`,
`migrate`, `license`, `mcp serve`. Each arrives with the WP that needs it. `test` uses
`cargo test`; nextest support is a follow-up.

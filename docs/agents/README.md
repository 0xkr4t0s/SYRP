# Agent side

This half of the docs is written for coding agents: terse, command-first, and backed by files you
can parse. When the site is built (`cargo syrp docs build`), it publishes under `/agents/`:

| File | What |
|---|---|
| `llms.txt` | Index of every page, llmstxt.org layout |
| `llms-full.txt` | Every page concatenated, with `<!-- source: path -->` separators |
| `md/<path>` | Markdown mirror of every page and every crate's AGENTS.md |
| `errors.json` | The error catalogue: code, title, cause, fix, example |
| `work-packages.json` | The work board: packages and waves |
| `crates.json` | Workspace crates with their layers |
| `envelope.schema.json` | JSON Schema of every CLI result |

Coming in Phase 0 work packages: `api-index.json` (WP-0.1-10), crate briefs and
`migration-map.json` (WP-0.2-12), the MCP server (WP-0.2-10).

## Rules of engagement

1. Run `cargo syrp doctor` once per session.
2. Look things up before guessing: `cargo syrp docs search <words>`, `cargo syrp wp show <id>`.
3. After every edit: `cargo syrp check`, then `cargo syrp test --crate <package>`.
4. On any `SYRP-*` code: `cargo syrp explain <code>`, apply the fix, rerun. Never suppress it.
5. Parse JSON, not text. Exit codes are stable: 0 ok, 1 failed, 2 usage, 3 environment,
   4 parity, 5 validator, 6 golden, 7 contract, 8 RT safety, 9 clean room, 10 internal.

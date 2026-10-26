# AGENTS.md

You are working on **Auricle**, a Rust reimplementation of the JUCE framework, built agent-first.
This file is the root guide for every agent. Read it fully before your first edit.

## Mission
Feature parity with JUCE 9.0.4, idiomatic Rust APIs, and tooling that lets agents verify their
own work. The full design is `docs/design/master-plan.md`; open decisions and their defaults are
`docs/design/0002-open-decisions.md`.

## Clean-room rules (non-negotiable)
- Never fetch, open, paste or recall JUCE **source code**, from any site or from memory.
- Allowed: JUCE's public **documentation** for behaviour, your WP spec, the master plan, and
  observed behaviour from parity scenarios.
- Never use or read `aloe-*` crates (self-described JUCE translations). `cargo auricle check`
  rejects them.
- No comments, constants or tables copied from JUCE. Write everything in your own words.
- If you realise you have seen JUCE source for the area you are working on,
  say so in the PR body and stop; the reviewer will have the WP reassigned to another agent.

## Repository map
| Path | What | Who edits |
|---|---|---|
| `crates/<group>/<crate>/` | Workspace crates (globbed members) | The owning WP |
| `work/` | Work packages, waves, specs | Spec room |
| `errors/<domain>.toml` | Error catalogue | Any WP (add codes, never renumber) |
| `parity/` | Ledger schema, scenarios | Spec room; the ledger itself is generated |
| `contracts/` | Frozen public-API snapshots | Contract-change WPs only |
| `docs/` | Docs site sources | Guides: any WP. Design: spec room |
| `skills/auricle/` | Agent Skill | Its WP |
| `agents/`, `.github/`, root files | Pipeline and shared files | PR manager only |

`ownership.toml` is the authority; `cargo auricle wp check-paths <id>` checks your diff.

## The closed loop
```sh
cargo auricle doctor                  # once per session
cargo auricle wp show <id>            # your spec, owned paths, dependencies
cargo auricle docs search <words>     # before writing a helper, look for one
cargo auricle check                   # after every edit
cargo auricle test --crate <package>  # after every edit
cargo auricle explain <AUR-code>      # on any diagnostic
cargo auricle wp check <id>           # acceptance: must exit 0 before you open the PR
```
Output is JSON when piped. Exit codes: 0 ok, 1 failed, 2 usage, 3 environment, 4 parity,
5 validator, 6 golden, 7 contract, 8 RT safety, 9 clean room, 10 internal.

## Definition of done
- Every behaviour (B1, B2...) in the spec has a test named after it (`b2_...`).
- `cargo auricle wp check <id>` exits 0.
- Every public item has a rustdoc comment with an example that compiles.
- New error codes are in `errors/<domain>.toml`. The crate's `AGENTS.md` is current.
- A `changes/<id>-<n>.json` entry: `{"wp": "<id>", "kind": "added|changed|fixed", "summary": "..."}`.

## Conventions
- Naming: Rust API guidelines. JUCE names go in `#[doc(alias = "JuceName")]`.
- Errors: every error type implements `auricle_error::Diagnostic` (once WP-0.1-03 lands) and carries a code.
- Threads: message-thread APIs take `&MessageThread`; audio code never allocates, locks or does I/O.
- `unsafe`: only with a `// SAFETY:` comment, and declare `unsafe_count` in the PR front matter.
- Files: at most 1,200 lines. Macros: none without a written justification in the PR.
- Dependencies: only crates with licences allowed by `deny.toml`. Declare new ones in `deps_added`.

## What you may edit, and change requests
Edit only your WP's paths plus `changes/`, `errors/`, `docs/guide/`, `docs/cookbook/`,
`docs/migration/`. For anything else (workspace `Cargo.toml`, CI, `deny.toml`, another crate),
open an issue with the **Change request** form. The PR manager applies change requests in batches.

## Commits and PRs
- Branch `wp/<id>-<slug>`. PR title `[<id>] <crate>: <what>`. At most 1,200 changed lines.
- PR body: use `.github/pull_request_template.md`; the first line is the front matter marker.
- Every commit ends with trailers:
  ```text
  WP: <id>
  Spec: work/specs/<id>.md
  Agent-Model: <your model id>
  ```

## When you are stuck
Push what you have as a **draft** PR, and comment on the WP issue starting with
`BLOCKED:` followed by the exact question. The spec room answers on the issue. Do not guess at
behaviour the spec does not define.

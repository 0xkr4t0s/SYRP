---
name: syrp
description: Build, test and verify code in the Syrp Rust framework (a JUCE reimplementation). Use when working in the syrp repository, on a Syrp work package (WP), when preparing or revising a Syrp PR, or when a `cargo syrp` command or SYRP-* error code appears.
---

# Syrp

## The closed loop
1. Run `cargo syrp doctor --json` once per session; fix anything it reports first.
2. Look things up instead of guessing: `cargo syrp wp show <id>` (spec, owned paths,
   dependencies), `cargo syrp docs search <words>`, `cargo syrp explain <code>`.
3. After every edit run `scripts/loop.sh <package>`: it runs `cargo syrp check --json`, then
   `cargo syrp test --crate <package> --json`, and prints one JSON line.
4. On any SYRP-* code, run `cargo syrp explain <code>`, apply the fix, and rerun. Never suppress it.
   Every code with its cause and fix is also in `references/errors.json`.
5. Before pushing: `cargo syrp wp check <id> --base origin/main --json` must exit 0, **and**
   `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` must pass (see the rules below).

Output is JSON when piped, or with `--json`. The envelope (`schema: syrp.envelope/v1`) has
`ok`, `exit_code`, `results[]` and `diagnostics[{code, severity, message, hint}]`;
`cargo syrp schema` prints its JSON Schema. Exit codes: 0 ok, 1 failed, 2 usage,
3 environment, 4 parity, 5 validator, 6 golden, 7 contract, 8 RT safety, 9 clean room,
10 internal.

## Hard rules
- Clean room: never fetch or recall JUCE source. Work from the spec, the master plan
  (`docs/design/master-plan.md`, search it) and public JUCE documentation for behaviour only.
- Edit only your WP's owned paths (`wp show` lists them) plus `changes/`, `errors/`,
  `docs/guide/`, `docs/cookbook/`, `docs/migration/`. `cargo syrp wp check-paths <id>` proves it.
- Never allocate, lock or do I/O on the audio thread. Message-thread APIs need a `MessageThread` token.
- Never edit goldens, `contracts/` or ledger evidence. Never skip or delete a test to get green.
- Name every test after the spec behaviour it proves: `b3_try_push_fails_when_full`.

## Rules learned the hard way
These are the gates that most often turn a "green locally" PR red.
1. **The size gate counts everything except `Cargo.lock`.** At most 1,200 changed lines
   (additions plus deletions) per PR: fixtures, generated JSON, docs and `changes/` all count.
   Check with `git diff --shortstat origin/main...HEAD -- . ':!Cargo.lock'`. Over the cap:
   ship a coherent first increment with `"final": false` (`SYRP-PR-0001`).
2. **Every commit needs provenance trailers, merge commits included.** The gate reads each
   commit's body for `WP: <id>` (or `Change-Request: <n>`) and `Agent-Model: <model>`. A plain
   `git merge origin/main` creates a commit without them and fails `SYRP-PR-0006`. Rebase
   instead, or give the merge commit the trailers (recipe `fix-failing-check.md`).
3. **Run rustdoc with `-D warnings` locally.** CI runs
   `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`; `wp check` does not. Broken
   intra-doc links (often to feature-gated items) only show up there.
4. **Files your WP does not own go through a `[CR]` change-request issue.** The workspace
   `Cargo.toml`, CI, `deny.toml`, `ownership.toml` and other crates belong to someone else.
   Revert the edit (`SYRP-WP-0005`) and open an issue from the Change request form (title
   prefix `[CR] `, label `change-request`) with a JSON payload; the PR manager applies it.

## Recipes (`references/recipes/`)
| Recipe | Use it when |
|---|---|
| `start-a-work-package.md` | You were given a WP id. |
| `create-a-crate.md` | Your WP owns a crate that does not exist yet (`SYRP-WP-0008`). |
| `add-error-code.md` | Your code reports a new failure that needs a SYRP-* code. |
| `fix-failing-check.md` | `check`, `test`, `wp check`, CI or a merge gate is red. |
| `respond-to-review.md` | A reviewer requested changes on your PR. |

## References
- `references/errors.json`: every catalogued code (`code`, `title`, `severity`, `cause`, `fix`).
  Regenerate with `scripts/refresh.sh`.

## Scripts
- `scripts/loop.sh [package]`: check, then test (one package or the workspace); one JSON line
  `{ok, exit_code, check, test}`; exits with the worse of the two CLI exit codes.
- `scripts/refresh.sh`: rebuilds `references/errors.json` with
  `cargo syrp docs build --skip-book`.
- `tests/run.sh`: the skill's own tests (`b1_`..`b4_`); `b4_` runs `--help` for every
  `cargo syrp` command this skill quotes.

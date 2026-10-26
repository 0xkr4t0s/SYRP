# WP-0.2-13: Agent-eval harness v0: 10 tasks scored end to end

## Purpose
Measure how well agents use Syrp (master plan §2): a fixed task set scored automatically.

## Behaviour
- B1. `evals/tasks/*.toml`: 10 tasks (for example: explain a given SYRP code, find which WP owns a crate, create a crate skeleton that passes `cargo syrp check`), each with a prompt, a setup script and a deterministic verifier command.
- B2. A runner that executes each task's verifier against a workspace and reports pass/fail and timing as JSON.
- B3. Running an actual agent is pluggable (a command template), so the harness itself needs no API keys in tests.
- B4. A summary KPI (success rate) the exit evaluator can read.

## Crate setup
Create `crates/tooling/agent-eval/` with `Cargo.toml` (inherit `version`, `edition`, `rust-version`, `repository`, `authors` from the workspace; `[lints] workspace = true`), `[package.metadata.syrp] layer = "L6"`, an `AGENTS.md` following `docs/agents/crate-agents-template.md`, and `src/lib.rs`. Workspace members are globbed, so no root edit is needed.
Licence: `license = "AGPL-3.0-only OR BUSL-1.1"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-0.2-13` first and `cargo syrp wp check WP-0.2-13` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

# syrp-agent-eval

## Purpose
Agent-eval harness v0 (master plan §2, A20). `evals/tasks/<id>.toml` holds the fixed task set:
`prompt`, `setup`, `verify` (deterministic; exit 0 = pass), `reference` (a known-good
solution), optional `timeout_secs`. `run` gives each task a fresh `<work-dir>/<id>/`, runs
setup, the `Agent`, then the verifier, and returns a `Report` (`syrp.agent-eval/v1`): per-task
pass/fail, failing phase, per-phase timing, a log tail, and `summary.success_rate`, the KPI the
exit evaluator reads. CLI: `syrp-agent-eval run [--agent none|reference|'<template>']
[--only a,b] [--out report.json] [--min-success-rate 0.7]`; `syrp-agent-eval syrp <args>`
runs the Syrp CLI in-process (the default for the scripts' `syrp` function).

## Ledger rows
Tooling rows only (no JUCE class).

## Invariants
- Scripts run as `sh -c` with `set -e` in the workspace, with `EVAL_TASK`, `EVAL_WORKSPACE`,
  `EVAL_PROMPT_FILE`, `SYRP_ROOT`, a fixed git identity, an empty global git config and a
  `syrp` function. Verifiers read `SYRP_ROOT` and never write to it.
- Agent `none` is the baseline (every verifier must fail); `reference` is the self-test (every
  task must pass); a command template (`{prompt_file}`, `{workspace}`, `{root}`, `{task}`,
  shell-quoted) is the opt-in model-in-the-loop mode. Tests never call a model or the network.
- Task failures are results; `EvalError` (`SYRP-EVAL-0001..0004`) is for harness failures.
  Exit: 0 ok, 1 below `--min-success-rate`, 2 usage, 3 environment.

## Thread affinity
Build-time tooling, any thread. Each phase is a child process killed at the time limit.

## Stability
experimental (0.x). Report shape `syrp.agent-eval/v1`.

## How to test
`cargo syrp test --crate syrp-agent-eval` (needs `sh`, `git`, `cargo` on PATH). A real agent,
opt-in and never in CI: `cargo run -p syrp-agent-eval -- run --agent 'my-agent {prompt_file}'`.

## Known gaps
- 10 of A20's ~60 tasks; no turn-count or first-error KPIs; no nightly job (CI change request).
- `SYRP-EVAL-*` are not in `cargo syrp explain` (CLI change request). Untested on Windows.
- A timed-out phase's grandchildren may outlive it.

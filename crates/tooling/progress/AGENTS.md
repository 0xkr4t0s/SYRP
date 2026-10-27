# syrp-progress

## Purpose
Builds the public progress page (WP-0.2-16). `load(root, open_prs)` reads the board
(`work/waves`, `work/packages`), the JUCE class inventory and parity ledger (`parity/`), and the
first-parent git history, and returns a `Progress` snapshot: waves with each WP's shown status,
one cell per JUCE class with its status, and the merged PRs. The `syrp-progress` binary writes it
as `progress.json` and `index.html`. The docs workflow runs it after every
merge to main.

## Ledger rows
Tooling rows only (no JUCE class).

## Invariants
- Read-only over the repo: it never writes outside `--out`.
- WP status: `done` from the board wins; else `active` if a merged subject or open PR title starts
  with `[<id>]`; else `parked` in a disabled wave; else `planned`.
- Class status: ledger `done`/`mapped` is verified, `gap`/`deviation` is gap, `partial` or an
  owner WP that is done or active is in progress, anything else (or no row) is todo.
- `parity/ledger.json` is used once it has rows; until then the seed in `parity/inventory/`.
- Times are UTC. Output is deterministic for the same inputs.
- Codes `SYRP-PROG-0001..0005` are in `errors/progress.toml`; `ProgressError` implements
  `Diagnostic`. The binary exits 3 on any of them.

## Thread affinity
Build-time tooling, any thread. Spawns `git`.

## Stability
experimental

## How to test
`cargo syrp test --crate syrp-progress`. The behaviour tests build a fixture repo in the temp
directory and need `git` on `PATH`.

## Known gaps
- Module phase and difficulty live in `src/page.html`, copied from master plan §3. The treemap
  layout is written in the page; it loads no third-party script.
- Links point at `https://github.com/SYRP-Foundation/SYRP` (the `REPO` constant in `src/page.html`); the
  workspace `repository` key still names the old home, so the page does not read it.
- The page script escapes every value it puts into `innerHTML`, numbers included; tooltips use
  `textContent`.
- Open PRs come only from `--open-prs`; without it, WPs with only an open PR show as planned.

# WP-0.2-16: syrp-progress: progress page and progress.json

## Purpose
A public progress page in the spirit of master plan §9.10 ("the parity ledger is the progress
report"), regenerated after every merge to main. It shows how much of the JUCE 9.0.4 surface
Syrp covers, per module and per class, and how the work packages are moving. Requested by the
project owner; a hand-built draft was reviewed before this spec.

## Inputs (read-only)
- `work/waves/*.toml` and `work/packages/*.toml`.
- `parity/inventory/juce-9.0.4-classes.json` (module and class totals).
- `parity/ledger.json` for class statuses; while it has no rows, `parity/inventory/ledger-seed.json`.
- The first-parent git history of the checked-out branch.
- Optionally, a JSON file listing open PRs (`[{"number": 38, "title": "[WP-0.2-07] ..."}]`), as
  `gh pr list --json number,title` prints it.

## Behaviour
- B1. Work packages are grouped by wave in wave order. A package's shown status is `done` when its
  TOML says so; otherwise `active` when a merged commit subject or an open PR title starts with
  `[<id>]`; otherwise `parked` when its wave is disabled; otherwise `planned`.
- B2. Every module in the inventory gets one cell per class. A class's status is `verified` when
  its ledger row is `done` or `mapped`, `gap` when the row is `gap` or `deviation`, `inwork` when
  the row is `partial` or has an `owner_wp` whose shown status is `done` or `active`, and `todo`
  otherwise. A
  class missing from the ledger is `todo`. A module's `complete` flag is copied from the inventory.
- B3. Merged PRs are the first-parent commits whose subject ends in `(#<n>)`, oldest first, with
  the commit time in UTC, the subject without the suffix, and the WP id from a leading `[WP-...]`.
- B4. `render_html` returns one self-contained HTML page with the data embedded as JSON. Embedded
  text can never end the script element early (`</` is escaped).
- B5. The `syrp-progress` binary takes `--root <repo>`, `--out <dir>` and optional
  `--open-prs <file>`, writes `<out>/progress.json` and `<out>/index.html`, and exits 0. A missing
  or malformed input is a `SYRP-PROG-*` diagnostic and exit code 3.
- B6. Output is deterministic: the same inputs give byte-identical files.

## Page contents
Summary meters (classes verified, WPs done, PRs merged); a treemap of modules sized by class count
with one cell per class; a treemap of work packages grouped by wave and sized by agent-hour budget;
a roadmap strip by phase; a per-module table; WP cards linking their PRs; a cumulative merge chart;
a "how it is counted" note. Module phase and difficulty come from master plan §3, kept in the
crate as data in our own words. No third-party script: the treemap layout is written in the page.

## Out of scope
Publishing. A change request adds the step to `.github/workflows/docs.yml` that runs the binary
into `target/site/progress/` after this WP merges.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-0.2-16` first and `cargo syrp wp check WP-0.2-16` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

# Open decisions and the defaults in use

The master plan (§11) left eleven decisions to the project owner. Work started on 2026-10-26 with
the plan's recommended default for each. Every default is reversible; the "What changes if
overridden" column says what would have to be redone.

| # | Decision | Default in use | Where it shows up | What changes if overridden |
|---|---|---|---|---|
| 1 | Licence | `AGPL-3.0-only OR BUSL-1.1`; interface crates `MIT OR Apache-2.0`; BUSL converts to `Apache-2.0 OR MIT` after 3 years | `LICENSE.md`, `LICENSES/`, crate `license` fields | Licence files and `Cargo.toml` fields. No code. |
| 2 | Threshold details | Revenue plus funding, Affiliates included, trailing 12 months and last fiscal year, 90-day grace, end users always free, education and small non-profits free, no reporting | `LICENSES/BUSL-1.1.txt` parameters | The Additional Use Grant text. |
| 3 | Name | **Syrp**, chosen by the owner on 2026-10-26 (`syrp-*` crates, `cargo syrp`); no trademark filing yet | Everywhere | A mechanical rename; cheapest before any crates.io publish. |
| 4 | Parity baseline | JUCE **9.0.4** | `parity/ledger.schema.json`, WP-0.1-06 | Ledger inventory WP re-run against 8.0.15. |
| 5 | Minimum OS targets | Windows 10 1607, macOS 11, iOS 13, Android API 26, glibc 2.31-class Linux, AudioWorklet browsers | Master plan §6; platform WPs from Phase 2 | Only platform WPs, none of which have started. |
| 6 | VST2 | Deliberate gap (no SDK licence) | WP-0.1-06 marks it `gap` | A wrapper WP in Phase 5 if you hold a licence. |
| 7 | JUCE-shaped C++ facade | No. Ship a migration kit instead | Phase 8 only | Nothing until Phase 8. |
| 8 | Budgets | 12 implementer slots, 24 agent-hours per WP, 3 review rounds, 2 restarts; no monthly spend cap set | `SYRP_SLOTS` variable, WP `budget` | Repository variables and WP files. **A monthly spend cap is still needed from you.** |
| 9 | Escalation contact and channel | GitHub issues labelled `needs-human`, assigned to `@0xkr4t0s`; GitHub notifications as the channel | `conductor.yml` | Add a webhook (Slack, email) to the escalate step. |
| 10 | External contributions | Closed until a CLA is in place (target: after 1.0 or when you decide) | `LICENSE.md`, `CONTRIBUTING.md` | Add a CLA bot and open the repo to PRs. |
| 11 | Renaming JUCE-coined terms | Keep familiar names for now (`ValueTree`, `LookAndFeel`) with `#[doc(alias)]`; revisit after counsel review | API naming in later WPs | Renames in L2/L4 crates; cheapest before Phase 1 ends. |

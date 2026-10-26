# WP-0.1-06: Parity ledger: seeded inventory of every JUCE 9.0.4 public class from public docs

## Purpose
Seed `parity/ledger.json` with one row per public class of JUCE 9.0.4, so phase exits and docs can be computed (master plan §9.10).

## Behaviour
- B1. Write `parity/inventory/juce-9.0.4-classes.json`: every public class grouped by module, built only from the public API documentation pages (docs.juce.com). Record the source URL of each module page.
- B2. Propose ledger rows in `parity/inventory/ledger-seed.json` that validate against `parity/ledger.schema.json`, all with status `todo`, `owner_wp` filled where an existing WP covers the class.
- B3. Mark deliberate gaps listed in master plan §1 (VST2, AudioCDBurner/AudioCDReader, ActiveX, AppleRemote, blocking modal loops, Projucer GUI) with status `gap` and a reason.
- B4. Do not copy documentation prose; one-line summaries in your own words only.
- B5. The PR manager regenerates `parity/ledger.json` from the seed in the merge queue; you do not edit it.

## Acceptance
- The seed validates against the schema (add a test or script under `parity/inventory/`).
- Coverage: every module listed in master plan §3 appears.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-0.1-06` first and `cargo syrp wp check WP-0.1-06` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

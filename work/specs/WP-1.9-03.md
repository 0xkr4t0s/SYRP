# WP-1.9-03: syrp-compat-ui: JUCE-shaped Component and LookAndFeel as custom rows in the syrp-ui node table

## Purpose
The optional compatibility front-end from ADR 0003 D1 and master plan §5.6: JUCE-shaped `Component` and `LookAndFeel` for ported editors, writing to the same data-oriented node table as the native API. It owns no geometry or storage.

**Status: parked.** Blocked until the `syrp-ui` and `syrp-laf` WPs exist and merge (phase 3). `depends_on` gains them then, and wave 1.9 is enabled for this package.

## Behaviour
- B1. `Component` trait with the familiar hooks (`paint`, `paint_over_children`, `resized`, mouse and key handlers, focus callbacks). A compat component is a `Custom` row in the node table; bounds, visibility and z-order live in the table.
- B2. `resized` hooks run inside the table's layout pass; `paint` hooks record into the same display list as native nodes.
- B3. `LookAndFeel` resolved up the tree as JUCE documents, over the `syrp-laf` theme data.
- B4. Every compat object exposes `.handle()` returning the native node handle (ADR 0003 D1).
- B5. Dependency direction: `syrp-compat-ui -> ui, laf` only.

## Crate setup
Create `crates/compat/ui/` with `Cargo.toml` (inherit `version`, `edition`, `rust-version`, `repository`, `authors`, `publish` from the workspace; `[lints] workspace = true`), `[package.metadata.syrp] layer = "L5"`, an `AGENTS.md` following `docs/agents/crate-agents-template.md`, and `src/lib.rs`. Workspace members are globbed, so no root edit is needed.
Licence: `license = "AGPL-3.0-only OR BUSL-1.1"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-1.9-03` first and `cargo syrp wp check WP-1.9-03` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

# WP-1.9-01: syrp-compat-state: JUCE-shaped ValueTree, Value and CachedValue over the syrp-state store

## Purpose
The optional compatibility front-end from ADR 0003 D1 and master plan §5.9: ported code keeps JUCE's ValueTree idioms while the data lives in `syrp-state`. It owns no storage.

**Status: ready.** WP-1.2-03 (the `syrp-state` store, written from the WP-0.2-15 spike result) and WP-1.2-04 (`UndoManager`, which JUCE's `ValueTree` methods take) have merged, and wave 1.9 is enabled for this package. `ValueTree` owns its subscriptions so that listeners die with the wrapper and follow it on assignment (`valueTreeRedirected`), as `docs/design/0005-state-arena-spike.md` recommends.

## Behaviour
- B1. `ValueTree` is a handle (store plus node key). Clone is shallow, `deep_clone()` is explicit, equality is identity, `is_equivalent_to` compares content. Detached subtrees live while referenced (the store's reference counts).
- B2. `ValueTree::Listener`-equivalent trait with the documented callbacks (property changed, child added, child removed, child order changed, parent changed, redirected), delivered from the store's change log in the documented order.
- B3. `Value` and `CachedValue<T>` as observable cells over node properties, with `CachedValue` defaults and the `Var` conversions.
- B4. Every compat object exposes `.handle()` returning the native `syrp-state` key, so code can move to the native API one piece at a time (ADR 0003 D1).
- B5. Dependency direction: `syrp-compat-state -> syrp-state` only. No `syrp-state` type names anything in this crate.
- B6. Tests port the WP-0.2-15 scenarios through the compat API and add `parity/scenarios/` entries for listener ordering.

## Crate setup
Create `crates/compat/state/` with `Cargo.toml` (inherit `version`, `edition`, `rust-version`, `repository`, `authors`, `publish` from the workspace; `[lints] workspace = true`), `[package.metadata.syrp] layer = "L5"`, an `AGENTS.md` following `docs/agents/crate-agents-template.md`, and `src/lib.rs`. Workspace members are globbed, so no root edit is needed.
Licence: `license = "AGPL-3.0-only OR BUSL-1.1"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-1.9-01` first and `cargo syrp wp check WP-1.9-01` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

# WP-0.2-15: ValueTree arena trial: refcounted detach/reattach and listener ordering over a slotmap store

## Purpose
A short spike required by master plan §5.9 and ADR 0003 before the `syrp-state` WP is written. It answers one question: can a slotmap arena with a change log as the source of truth for observers reproduce JUCE's documented ValueTree semantics, in particular a subtree that is detached, kept alive by a reference and re-attached elsewhere, and the documented listener-notification order? The code is throwaway; the write-up is the deliverable.

## Behaviour
- B1. A minimal store: slotmap arena of nodes, each with an interned `u32` type id, a small sorted property list and an ordered child list. Message-thread only (`!Send`). No persistence, no undo, no schema.
- B2. A `TreeRef` handle (store plus node key) with JUCE-shaped operations: `add_child`, `remove_child`, `set_property`, `remove_property`, `parent`, `child`, `num_children`. Cloning a handle is shallow; nodes carry a reference count so a detached subtree lives while any handle refers to it and is reclaimed when the last one drops.
- B3. Every mutation appends a `(node, op)` record to a change log. Listeners subscribe per node; notifications are delivered from the log after the mutation, so a listener cannot re-enter it.
- B4. Scenarios written from public JUCE documentation only, each a test: detach a subtree, hold it, re-attach under another parent, mutate it, and observe the right listeners fire; listeners on ancestors see changes in descendants; a listener added during notification is not called for the current change; reclaiming an orphan frees its slots (assert arena occupancy).
- B5. Micro-benchmarks for property get/set on a node with 4, 16 and 64 properties, compared with the ADR 0003 budget (~5–20 ns per property access).
- B6. Write-up at `docs/design/0005-state-arena-spike.md`: what passed, what did not, the measured numbers, and a recommendation the spec room can turn into the `syrp-state` WP. Scenarios that could not be decided from documentation alone are listed for the differential harness.

## Crate setup
Create `crates/spikes/state-arena/` with `Cargo.toml` (inherit `version`, `edition`, `rust-version`, `repository`, `authors`, `publish` from the workspace; `[lints] workspace = true`), `[package.metadata.syrp] layer = "L2"`, an `AGENTS.md` following `docs/agents/crate-agents-template.md`, and `src/lib.rs`. Workspace members are globbed, so no root edit is needed.
Licence: `license = "AGPL-3.0-only OR BUSL-1.1"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-0.2-15` first and `cargo syrp wp check WP-0.2-15` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

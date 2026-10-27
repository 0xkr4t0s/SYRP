# WP-1.2-07: syrp-state: Synchroniser, change messages from the log and apply_change

## Purpose
Wave 1.2 item 7: `ValueTreeSynchroniser` from master plan §5.9, whose deltas come from the store's change log ("Undo, `ValueTreeSynchroniser` deltas, `inspect`, WebView JSON patches and remote editors all read the same log"). One side turns changes under a root node into byte messages; the other applies them to a replica, for example across `syrp-ipc` or between a plugin and its editor process. Both ends are Syrp, so the message format is Syrp's own. It works in the crate WP-1.2-03 creates, after WP-1.2-13, whose binary encoding it reuses for subtrees and values. Delivery: 2 PRs, each at most 1,200 changed lines (B1–B2, then B3–B5).

## Behaviour
- B1. `Synchroniser::new(store, mt, root: NodeRef, sink: impl FnMut(&MessageThread, &[u8]))` subscribes to `root`'s subtree. Each change log record under `root` (property set or removed, child added, removed or moved) becomes one message, passed to `sink` during listener delivery and in log order. `send_full_sync(mt)` sends one message with the whole subtree. Changes outside `root` and to `root`'s own parent link send nothing. Dropping the synchroniser unsubscribes.
- B2. Messages: a versioned layout with one kind per change (full sync, property set, property removed, child added with its subtree, child removed, child moved), addressing the changed node by its path of child indices from `root`, and encoding identifiers, values and subtrees with the WP-1.2-13 binary encoding. Write the layout into the rustdoc. It is not JUCE's `ValueTreeSynchroniser` byte format, which the documentation does not describe; record that as a gap in the crate `AGENTS.md`.
- B3. `apply_change(store, mt, target_root, &[u8], undo: Option<&mut UndoManager>) -> Result<(), StateError>` applies one message to the replica rooted at `target_root` (a full sync replaces its properties and children in place, keeping the `target_root` node itself). A message that is corrupt, has an unknown version or kind, or names a path or index that does not exist is a coded error, and the replica is unchanged: validate before mutating. With `Some(undo)` the applied change is undoable on the replica.
- B4. Two-way use: when the replica also has a `Synchroniser`, the changes `apply_change` makes are not sent back to where they came from (no echo); tested with two stores wired to each other.
- B5. Proptest: for random operation sequences on the source (the WP-1.2-03 B7 generator, plus undo and redo through WP-1.2-04), a replica started from `send_full_sync` and given every message in order is `is_equivalent`, with strict `Var` equality per property, to the source after every step. Errors continue `errors/state.toml`.

## Not in this WP
Transport (messages go to a caller's sink; `syrp-ipc` or anything else carries them), conflict resolution between two writers, and JUCE wire compatibility.

## Crate setup
The whole crate `crates/core/state/`, which WP-1.2-03 creates; this WP starts after WP-1.2-13 merges and may edit any file there, including `Cargo.toml` and `AGENTS.md` (add this WP's ledger rows, invariants and gaps). No new crate.
Licence: `license = "AGPL-3.0-only OR BUSL-1.1"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-1.2-07` first and `cargo syrp wp check WP-1.2-07` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

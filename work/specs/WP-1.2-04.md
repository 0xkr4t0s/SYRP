# WP-1.2-04: syrp-state: UndoManager, transactions, coalescing and UndoableAction

## Purpose
Undo for the state store, from master plan §5.9 and Wave 1.2 item 4, built on the reversible log records of WP-1.2-03. It works in the crate WP-1.2-03 creates (it may edit any file there). `syrp-compat-state` (WP-1.9-01) passes JUCE's `UndoManager*` arguments through to this type. Delivery: 2 PRs (B1–B3, then B4–B6).

## Behaviour
- B1. Every mutating `Store` call gains an `undo: Option<&mut UndoManager>` argument (or a sibling `_with_undo` method; choose one and apply it to every mutation). With `Some`, the mutation's log records are added to the manager's current transaction; with `None` nothing is recorded. The manager keeps the nodes its records name alive (one count each) until the transaction is discarded.
- B2. Transactions: `begin_new_transaction(name)` starts one; records between two calls form one transaction. `undo(mt)` reverses the last transaction's records in reverse order; `redo(mt)` re-applies them in order. Undo and redo are ordinary mutations: they append log records and notify listeners, and they are not themselves recorded. Performing a new action after an undo clears the redo list. A detached child that a transaction removed is re-attached at its old index by undo.
- B3. Coalescing: within one transaction, consecutive `PropertyChanged` records for the same node and property merge into one that keeps the first old value and the last new value, so a slider drag is one undo step. The `UndoableAction::createCoalescedAction` documentation describes coalescing in general; this merge rule is Syrp's own and is documented as such. A transaction whose records all cancel out (value set back to its original) is kept, with an empty effect, until the next one starts; document this.
- B4. `UndoableAction` trait for work outside the store (`perform`, `undo`, `size_in_units`, optional `coalesce_with`), added with `perform(action)`; actions and store records interleave in order within a transaction.
- B5. Queries and limits as the `UndoManager` documentation describes: `can_undo`, `can_redo`, `undo_description`, `redo_description`, `undo_descriptions`, `number_of_units_taken_up`, `set_max_number_of_stored_units(max_units, min_transactions)` (oldest transactions are dropped first, never fewer than `min_transactions` kept), `clear_undo_history`, `is_performing_undo_redo`, `undo_current_transaction_only`, `transaction_count`. A store record counts as 1 unit plus 1 per 64 bytes of `Var` payload; document it.
- B6. Tests port the scenarios of `docs/design/0005-state-arena-spike.md` with undo and redo between steps, and a proptest: for random operation sequences recorded with one transaction per step, undoing everything returns a tree `is_equivalent` to the start, and redoing everything returns one equivalent to the end.

## Not in this WP
Undo for persistence or for the synchroniser; JUCE-shaped `UndoManager` wrappers (WP-1.9-01 uses this type directly).

## Owned paths
The whole crate `crates/core/state/`, which WP-1.2-03 creates. This WP starts after WP-1.2-03 merges.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-1.2-04` first and `cargo syrp wp check WP-1.2-04` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

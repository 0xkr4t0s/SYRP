# syrp-state

## Purpose
The native state store (master plan §5.9, ADR 0003 D2), built on the design validated by the
WP-0.2-15 spike (`docs/design/0005-state-arena-spike.md`). A `Store` is one document: a slotmap
arena of nodes, each with a type `Identifier`, properties as a `Vec<(Identifier, Var)>` sorted by
identifier, and an ordered `Vec` of children. It is the data core behind JUCE's `ValueTree`
semantics; the JUCE-shaped `ValueTree` lives in `syrp-compat-state` (WP-1.9-01), never here.
`UndoManager` (WP-1.2-04) groups the store's reversible log records into undo transactions;
`syrp-compat-state` passes JUCE's `UndoManager*` arguments straight through to it.

## Ledger rows
juce_data_structures/UndoManager, juce_data_structures/UndoableAction (WP-1.2-04).
`syrp-compat-state` owns the `juce_data_structures/ValueTree` rows.

## Invariants
- A node's count = live `NodeRef`s + `Subscription`s + 1 if it has a parent + 1 per
  undelivered log record naming it (a child change names parent and child). At zero the slot is freed and each child loses its parent link and that count.
  Freeing is iterative (`Inner::free_stack`), never recursive.
- A parent freed while a child is held leaves the child parentless (Syrp choice, spike open
  question 6, pinned by `b3_freed_parent_leaves_held_child_parentless_syrp_choice`).
- Properties stay sorted by `Identifier` (interning order) with no duplicates; children keep
  insertion order; parent and child links agree; no cycles.
- Every fallible call returns a coded `StateError` (`errors/state.toml`, `SYRP-STATE-*`) and
  leaves the store unchanged. Setting a property to an `eq_same_type` value is a no-op.
- No `RefCell` borrow of the store is held while a `NodeRef` or listener is dropped, or while
  a listener runs. Every mutation logs under the borrow, releases it, then calls `flush`.
- A store with no subscriptions skips event building (`drain_unobserved`); with listeners the
  per-record snapshot reuses `Inner::scratch`. `set_property` with `i32`/`f64` values does not
  allocate after warm-up, with or without listeners (`tests/no_alloc.rs`).
- `deep_copy` logs nothing and copies iteratively; `is_equivalent` compares values with
  `Var`'s `==`.
- Delivery (`delivery.rs`): non-reentrant; listener mutations queue behind the current record.
  Targets are the node then ancestors at delivery time (`ParentChanged`: own listeners only),
  snapshotted per record. A drop guard unpins the current record and clears the flag on panic;
  later records keep their pins until the next mutation's `flush` delivers them. Records a
  `LogCursor` reads after delivery hold no pins, so their keys may be dead.
  Listener order is the spike's and is not yet parity-checked.
- The log keeps records until delivered and passed by every `LogCursor`. No-op sets and
  removing an absent property log nothing (Syrp choice, spike open questions 7 and 9).
- `NodeRef` equality is identity (same store `Rc`, same key). `NodeKey` holds no count and no
  store identity.
- No public fields (ADR 0003 D3). No compat layer here (ADR 0003 D1).
- No `as_view()` (ADR 0003 exception): nodes sit behind a `RefCell`, so a view would keep the
  store borrowed and block mutations and handle drops. Reads return owned values instead.
- `NodeKey` has no store identity; every API that takes one (`Store::upgrade`) says so.
- Undo (`undo.rs`): every logged mutation has a `*_with_undo` sibling taking `&mut
  UndoManager`; the plain method records nothing. The sibling sets `Inner::capture_from`, and
  `flush` copies that call's own records (never a listener's) into `Inner::captured`, counting
  their nodes, before any delivery. The manager holds those counts until a transaction is
  discarded (redo cleared, history dropped, manager dropped). A manager binds to one store
  (`SYRP-STATE-0007`). Undo and redo replay records through the plain mutations, so they are
  logged and notified but not recorded; `ParentChanged` is not replayed (the child records
  log their own). An undo whose records no longer apply stops at the error, part-done.
- Coalescing (Syrp rule): a `PropertyChanged` for the node and property of the record just
  before it in the same transaction merges into it (first old value, last new value). A
  transaction that cancels out is kept until the next one starts, then dropped.
- `UndoableAction`s (`action.rs`) interleave with records in a transaction; an action may
  absorb the next one (`coalesce_with`). Units: a record is 1 + payload bytes / 64, an action
  its `size_in_units`. History is trimmed, oldest first, when a transaction starts and on
  `set_max_number_of_stored_units`; at least `min_transactions` and the current one stay.
  Transaction items are dropped outside the store borrow (actions may own handles).
- `Store::is_performing_undo_redo` is set by a guard for the length of a replay, so listeners
  can tell replayed changes; a panicking listener cannot leave it set.

## Thread affinity
`Store`, `NodeRef` and `Properties` are `!Send` and `!Sync` (they hold `Rc<RefCell<_>>`).
Every mutation takes `&MessageThread`; reads do not need the token. `NodeKey` is `Copy + Send`
but only meaningful to its store. Nothing here is for the audio thread.

## Stability
experimental

## How to test
```sh
cargo syrp test --crate syrp-state
PROPTEST_CASES=3000 cargo test -p syrp-state --test invariants   # deeper invariant run
cargo bench -p syrp-state   # Callgrind instruction counts; skips without valgrind
```
`tests/no_alloc.rs` installs `syrp_rt::GuardedAllocator`, so it is its own test binary.

## Benchmarks
Instructions per operation from `cargo bench -p syrp-state` (Callgrind, release, x86-64 Linux,
2026-10-26), by property count. CI does not gate on these yet.

| Case | 4 | 16 | 64 |
|---|---:|---:|---:|
| `property` | 112 | 132 | 152 |
| `set_property` (`f64`), no listeners | 434 | 454 | 474 |
| `set_property` (`f64`), listener on node and parent | 902 | 922 | 942 |

## Known gaps
WP-1.2-03 (B1-B8) and WP-1.2-04 (B1-B6) are complete. The CLI catalogue entry for the new
`errors/state.toml` domain needs a change request (the CLI crate is not owned here).

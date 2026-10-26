# syrp-state

## Purpose
The native state store (master plan §5.9, ADR 0003 D2), built on the design validated by the
WP-0.2-15 spike (`docs/design/0005-state-arena-spike.md`). A `Store` is one document: a slotmap
arena of nodes, each with a type `Identifier`, properties as a `Vec<(Identifier, Var)>` sorted by
identifier, and an ordered `Vec` of children. It is the data core behind JUCE's `ValueTree`
semantics; the JUCE-shaped `ValueTree` lives in `syrp-compat-state` (WP-1.9-01), never here.

## Ledger rows
None. `syrp-compat-state` owns the `juce_data_structures/ValueTree` rows.

## Invariants
- A node's count = live `NodeRef`s + 1 if it has a parent (later: + subscriptions + pending
  log records). At zero the slot is freed and each child loses its parent link and that count.
  Freeing is iterative (`Inner::free_stack`), never recursive.
- A parent freed while a child is held leaves the child parentless (Syrp choice, spike open
  question 6, pinned by `b3_freed_parent_leaves_held_child_parentless_syrp_choice`).
- Properties stay sorted by `Identifier` (interning order) with no duplicates; children keep
  insertion order; parent and child links agree; no cycles.
- Every fallible call returns a coded `StateError` (`errors/state.toml`, `SYRP-STATE-*`) and
  leaves the store unchanged. Setting a property to an `eq_same_type` value is a no-op.
- No `RefCell` borrow of the store is held while a `NodeRef` is dropped.
- `NodeRef` equality is identity (same store `Rc`, same key). `NodeKey` holds no count and no
  store identity.
- No public fields (ADR 0003 D3). No compat layer here (ADR 0003 D1).

## Thread affinity
`Store`, `NodeRef` and `Properties` are `!Send` and `!Sync` (they hold `Rc<RefCell<_>>`).
Every mutation takes `&MessageThread`; reads do not need the token. `NodeKey` is `Copy + Send`
but only meaningful to its store. Nothing here is for the audio thread.

## Stability
experimental

## How to test
```sh
cargo syrp test --crate syrp-state
```

## Known gaps
WP-1.2-03 increment 1 covers spec B1-B3 except these B1 operations, which arrive in
increment 2 with their change-log records: `remove_all_properties`, `remove_all_children`,
`move_child`, `child_with_type`, `child_with_property`, `root`, `is_ancestor_of`. Also to come:
the change log, `LogCursor` and record pins (B4); listeners, `Subscription`, delivery and its
panic guard (B5); the cheap delivery path and `no_alloc` test (B6); `deep_copy`,
`is_equivalent` and the invariant proptest (B7); Callgrind benchmarks, numbers recorded here
(B8). The CLI catalogue entry for the new
`errors/state.toml` domain needs a change request (the CLI crate is not owned here).

# syrp-state-arena-spike

## Purpose
Throwaway spike for WP-0.2-15 (master plan §5.9, ADR 0003): can a slotmap arena with a change
log as the source of truth reproduce the documented semantics of JUCE's `ValueTree` and
`ValueTree::Listener`? The deliverable is `docs/design/0005-state-arena-spike.md`; this code
backs it with tests. Do not depend on it; `syrp-state` replaces it.

## Ledger rows
None (spike).

## Invariants
- A node's `refs` = `TreeRef`s + `Subscription`s + 1 if parented + undelivered log records
  naming it. At zero the slot is freed and each child loses its parent link and one count.
- Properties sorted by `Id`; children in insertion order.
- No `RefCell` borrow is held while a listener runs or a `TreeRef` or closure is dropped.
- Listeners run only from `flush`, after the mutation; listener mutations are queued behind
  the current record. Listener lists are snapshotted per record.

## Thread affinity
Message thread only; everything is `!Send`. Not RT-safe.

## Stability
experimental (throwaway spike)

## How to test
```sh
cargo syrp test --crate syrp-state-arena-spike
cargo bench -p syrp-state-arena-spike     # B5 numbers, wall clock
```

## Known gaps
Out of scope by spec: undo, persistence, schemas, `moveChild`, deep copy, a full `Var`, the
`MessageThread` token. A panicking listener leaves delivery stuck. All for `syrp-state`.

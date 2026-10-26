# syrp-compat-state

## Purpose
The JUCE-shaped porting layer over `syrp-state` (master plan §5.9, ADR 0003 D1): `ValueTree`,
`Listener` (JUCE's `ValueTree::Listener`), `Value` with `ValueListener`, and `CachedValue<T>`
with the `VarConvert` conversions (JUCE's `VariantConverter`). Each
type is a thin handle over the native store and owns no data. Rust method names carry their
JUCE names as `#[doc(alias)]`.

## Ledger rows
juce_data_structures/ValueTree, juce_data_structures/ValueTree::Listener,
juce_data_structures/Value, juce_data_structures/CachedValue.

## Invariants
- No storage: a `ValueTree` is `Option<(Rc<Store>, NodeRef)>`. Clone is shallow, `==` is node
  identity, `deep_clone` and `is_equivalent_to` are the explicit content operations.
- Every compat object exposes `.handle()` returning the native `NodeKey` (ADR 0003 D1).
- Dependencies run compat to core only; `syrp-state` must never depend on this crate
  (`b5_*` test).
- An invalid tree reads as empty (void `Var`, zero counts, invalid trees) and its mutations
  are `Ok(())` no-ops, as in JUCE. Store errors (cycles, adding a child that has a parent,
  mixing stores) are returned as `StateError`, never panics.
- Listeners belong to the wrapper (JUCE V9): one store `Subscription` per listener, held in
  the wrapper, never copied by `Clone`, dropped with the wrapper, moved by `assign` (which then
  calls `redirected`). The wrapper and the subscription closure hold the listener and the store
  weakly, so a listener that keeps a `ValueTree` makes no `Rc` cycle; callers own listeners.
- Callbacks come only from store delivery (`Store::subscribe` order); a listener's `RefCell` is
  never borrowed twice (`try_borrow_mut`, skip when busy).
- `Value` and `CachedValue` hold no data of their own: a tree plus a property id. A standalone
  `Value::new` uses a hidden node in the thread's store. `CachedValue`'s cache is refreshed by
  its own store subscription, so it is current for every route (undo, native calls).
- `ValueTree::new` uses one store per thread, so trees made with it can be joined;
  `new_in` targets an explicit store.

## Thread affinity
Message thread only. Constructors and mutations take `&MessageThread`; `ValueTree` is `!Send`.

## Stability
experimental

## How to test
`cargo test -p syrp-compat-state`; `cargo clippy -p syrp-compat-state --all-targets -- -D warnings`.

## Known gaps
`Value` listeners are synchronous, not asynchronous as in JUCE. Listener order is the store's and is not yet
checked against JUCE (0005 open questions 1 to 9); the `parity/scenarios/` entries for it
belong to the spec room. XML/binary
serialisation is not in this WP.

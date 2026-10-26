# 0005: ValueTree arena spike

- **Status:** spike result, for the spec room. Not a decision record; it feeds the `syrp-state`
  and `syrp-compat-state` work packages.
- **Work package:** WP-0.2-15. Code: `crates/spikes/state-arena/` (`syrp-state-arena-spike`),
  throwaway.
- **Question (master plan §5.9, ADR 0003 D2):** can a slotmap arena, with a change log as the
  source of truth for observers, reproduce JUCE's documented `ValueTree` semantics, in particular
  a subtree that is detached, kept alive by a reference and re-attached elsewhere, and the
  documented rules for which listeners hear a change?

**Short answer: yes.** Every scenario that the public documentation decides passes. Property
reads cost 10–14 ns, inside ADR 0003's 5–20 ns budget, and only 1–2 ns of that is the arena.
Sets cost 27–37 ns without listeners and 117–150 ns with two, and that extra cost is in the
delivery path, not the store. What the documentation does not decide is listed at the end for
the differential harness.

## What was built
- `Store`: a `slotmap::SlotMap<NodeKey, Node>`, an interner (`&str` to `Id(u32)`), the change
  log (a `Vec` with a delivered cursor) and listeners keyed by node, in `Rc<RefCell<…>>` (`!Send`).
  A `Node` has a type `Id`, a `Vec<(Id, Var)>` sorted by `Id`, an ordered `Vec<NodeKey>` of
  children, an optional parent and a count. `TreeRef` (store plus key, shallow clone) has
  `add_child`, `remove_child`, `set_property`, `remove_property`, `property`, `parent`, `child`,
  `num_children` and `subscribe`.
- **Reference counting.** A node's count is the sum of its live handles, its subscriptions,
  one for its parent link, and one for each undelivered log record that names it. When the count
  reaches zero the slot is freed and each child loses its parent link and one count. Log pins
  make removing an otherwise unheld child safe: it lives until its removal has been delivered.
- **Change log and delivery.** Each mutation applies its change, appends `(node, op)` records
  (`PropertyChanged`, `ChildAdded`, `ChildRemoved`, `ParentChanged`), releases its borrow, then
  delivers undelivered records. If a delivery is already running (a listener mutated the tree),
  the outer loop delivers the new records after the current record's listeners finish, so no
  listener runs inside a mutation.
- **Listener targets.** `PropertyChanged`, `ChildAdded` and `ChildRemoved` go to listeners on the
  node and on every ancestor, computed at delivery time. `ParentChanged` goes to the node's own
  listeners only. With no listener in that set no event is built (the fast path below).
- `subscribe` returns a `Subscription` guard that holds a count on the node and unsubscribes on
  drop, the way a `juce::ValueTree` holding a listener holds a reference.

## Documentation the scenarios rely on
Only the public class pages for `ValueTree` and `ValueTree::Listener` on docs.juce.com were used.
No JUCE source was read. Statements are quoted briefly and labelled for the table below.

| Label | Page and member | Statement |
|---|---|---|
| V1 | `ValueTree` class | "they're simply a lightweight reference to a shared data container" |
| V2 | `ValueTree` class | "Creating a copy of another ValueTree simply creates a new reference to the same underlying object" |
| V3 | `ValueTree` class | "A ValueTree can only be a child of one parent at a time" |
| V4 | `addChild` | "Make sure that the child being added has first been removed from any former parent before calling this, or else you'll hit an assertion." |
| V5 | `addChild` | (paraphrased) an index below zero or past the end appends to the end |
| V6 | `removeChild` | "Removes the specified child from this tree's child-list." |
| V8 | `getReferenceCount` | "the total number of references to the shared underlying data structure" |
| V9 | `addListener` | "The listener is added to this specific ValueTree object, and not to the shared object that it refers to." "When this object is deleted, all the listeners will be lost, even if other references to the same ValueTree still exist." |
| V10 | `ValueTree` class | "although the children in a tree have a fixed order, the properties are not guaranteed to be stored in any particular order" |
| L1 | `valueTreePropertyChanged` | called for "property changes in that tree, and also for any of its children, (recursively, at any depth)" |
| L2 | `valueTreeChildAdded`, `valueTreeChildRemoved` | called "for child changes in both that tree and any of its children" |
| L3 | `valueTreeParentChanged` | "happens when the tree to which the listener was registered is added or removed from a parent"; "Unlike the other callbacks, it applies only to the tree to which the listener is registered" |

Neither page says anything about the order in which listeners are called, whether a listener is
called synchronously inside the mutating call, or whether setting a property to its current
value notifies.

## What passed
| Spec | Scenario | Relies on | Test |
|---|---|---|---|
| B1 | Nodes hold a type, properties (sorted internally; V10 lets any order through) and ordered children; insertion at an index | V5, V10 | `b1_nodes_hold_type_sorted_properties_and_ordered_children` |
| B2 | Clones share one node; counts track handles and the parent link; removing the child drops the link | V1, V2, V8 | `b2_handles_are_shallow_and_refcounted` |
| B2 | Adding a child that already has a parent (or an ancestor of the target) is refused | V3, V4 | `b2_add_child_rejects_parented_child_and_cycles` |
| B3 | Every mutation appends a record | spec | `b3_every_mutation_appends_a_record` |
| B3 | A listener's own mutation applies at once and is delivered after the current record | spec | `b3_listener_mutation_is_delivered_after_current_record` |
| B4 | Detach a two-node subtree, hold it by one handle, re-attach under another parent, mutate a grandchild: the old parent hears the removal, the new parent hears the addition and the grandchild's change, the subtree hears its two parent changes and the grandchild change, and nothing else fires | V2, V3, V6, L1, L2, L3 | `b4_detached_subtree_survives_and_reattaches_elsewhere` |
| B4 | Listeners on ancestors hear property and child changes in descendants; only the node's own listener hears its parent change; a change at the root is not sent down | L1, L2, L3 | `b4_ancestor_listeners_see_descendant_changes` |
| B4 | An orphaned three-child subtree keeps its slots while held and frees all four when the last handle drops; a removed child nobody holds is freed once its removal is delivered, or later if a listener keeps a handle | V8 | `b4_reclaiming_an_orphan_frees_its_slots` |
| B4 | A listener stops when its subscription (the stand-in for the `ValueTree` object it was added to) is dropped | V9 | `b4_listener_lives_with_its_subscription` |
| B5 | The measurement code runs for 4, 16 and 64 properties | spec | `b5_property_access_measurement_runs` |
| B6 | This document has the required sections | spec | `b6_write_up_has_the_required_sections` |

Scenarios whose listener *order* is undocumented compare the set of calls, not the sequence.
`b3_listener_added_during_delivery_waits_for_the_next_record` pins the spike's own choice for an
open question (below); it is not a parity test.

## What did not
Nothing that the documentation decides failed. These are the limits found:

1. **Listener identity is per wrapper object in JUCE (V9), per subscription here.** Both lose
   the listener when the holder is deleted, but JUCE's listeners also follow the wrapper on
   `operator=` (`valueTreeRedirected`). That is for `syrp-compat-state`'s `ValueTree` to do by
   owning its subscriptions; it needs nothing from the store.
2. **Delivery is deferred, not depth-first.** For a mutation made outside a listener, every
   listener has run by the time the call returns. A listener's own change is delivered after the
   remaining listeners of the current record, where calling listeners inside the mutation would
   deliver it first. Which JUCE does is open question 2.
3. **Targets are computed at delivery time.** If a listener moves a subtree before a queued record
   about it is delivered, the queued record goes to the new ancestors. Recording the ancestor
   chain with the record would fix it at a cost; see open question 3.
4. **A parent freed while a child is still held** leaves the child parentless without a
   `ParentChanged` record. Whether JUCE notifies in that case is open question 6.
5. **A panicking listener** leaves the delivery flag set, so later mutations are logged but never
   delivered. The real store needs a drop guard around delivery.
6. **Errors.** `add_child` returns `false` where JUCE asserts (V4). The real store should return
   a catalogued error.

## Measurements
Wall clock, `cargo bench -p syrp-state-arena-spike` (release, thin LTO), best of five runs of two
million operations, keys cycled with a counter, three separate invocations (ranges are across
them). Machine: a shared 4-vCPU cloud VM, Intel Xeon at 2.1 GHz. ADR 0003 D7's Callgrind
instruction counts should replace these once the bench gate exists.

| Properties | `property` (get) | Same get on a bare sorted `Vec` | `set_property`, no listeners | `set_property`, listener on node and parent |
|---:|---:|---:|---:|---:|
| 4 | 9.9–10.1 ns | 8.8–8.9 ns | 27–29 ns | 117–130 ns |
| 16 | 11.4 ns | 9.9–10.0 ns | 30–32 ns | 121–135 ns |
| 64 | 13.1–13.7 ns | 10.9–11.6 ns | 34–37 ns | 126–150 ns |

Against the ADR 0003 budget (~5–20 ns per property access, message thread):

- **Get is within budget.** The arena (a `RefCell` borrow plus a slotmap generation check) adds
  1–2 ns over the same binary search on a bare `Vec`. Growth from 4 to 64 properties is about
  3 ns, so the sorted small list is the right shape. (A first harness using `i % props` read
  18–20 ns; the 64-bit divide was most of it.)
- **Set without listeners is 27–37 ns**: search, equality check, a log append that pins the node,
  and a delivery pass that finds no listeners. Before the no-listener fast path it was 57–70 ns,
  because every record built an event with fresh handles.
- **Set with listeners is 117–150 ns**, almost all of it delivery: a hash lookup per ancestor, a
  freshly allocated listener snapshot, two handle clones for the event, a liveness check before
  each call and two dynamic calls. Fine at UI rates, and the part to optimise.

## Recommendation
Write the `syrp-state` WP on this design, with these changes:

1. **Keep:** slotmap arena; interned `u32` identifiers; properties as a small sorted
   `Vec<(Id, Var)>`; ordered child `Vec`; counts made of handles, subscriptions, the parent link
   and undelivered log records; delivery from the log after the mutation, with listeners' own
   mutations queued.
2. **Make delivery cheap:** keep listener lists in a slotmap keyed by node instead of a
   `HashMap`; use a reusable scratch buffer for the per-record snapshot; pass events by
   `NodeKey`, building handles only when a listener asks for one; and keep a per-node count of
   listeners in the subtree's ancestor chain (or a store-wide "anyone listening" count) so
   unobserved stores skip delivery entirely.
3. **Guard delivery** against panicking listeners with a drop guard.
4. **Errors, not booleans**, for a parented child, a cycle or a foreign node (new
   `errors/state.toml` domain).
5. **Keep the log** for undo and sync instead of compacting it after delivery as the spike does;
   decide its retention policy (per transaction, bounded) in the WP.
6. **Compat:** `syrp-compat-state::ValueTree` owns its listener subscriptions so that they die
   with the wrapper (V9) and follow it on assignment (`valueTreeRedirected`).
7. **Bench gate:** Callgrind instruction counts for get, set without listeners and set with two
   listeners at 4, 16 and 64 properties, with budgets taken from the numbers above.
8. **Do not test** any of the open questions below as parity until the harness has observed JUCE.

## Open questions for the differential harness
Each needs a scenario run against JUCE 9.0.4 before `syrp-compat-state` claims parity. The
spike's current behaviour is given so it can be compared.

1. **Is a listener added during a notification called for the change being delivered?** Spike:
   no. Listener lists are snapshotted per record, so it hears the next change.
2. Are listeners called synchronously inside the mutating call, so that a listener's own
   mutation is delivered depth-first before the remaining listeners of the outer change? Spike:
   no, it is queued.
3. When a queued notification is delivered after its subtree has moved, which ancestors hear it?
   Spike: the ancestors at delivery time.
4. Order of listeners: within one tree (registration order?) and between a tree and its ancestors
   (node first, or root first?). Spike: node's listeners in registration order, then each
   ancestor upwards.
5. Order of `valueTreeChildAdded` on the parent and `valueTreeParentChanged` on the child, and the
   same for removal. Spike: the parent's callback first.
6. Does a child get `valueTreeParentChanged` when its parent is destroyed while the child is still
   referenced? Spike: no.
7. Does setting a property to its current value notify, or count as an undoable change? Spike:
   no notification, no record.
8. Is a listener removed during a notification still called for that change if it had not been
   reached yet? Spike: not called.
9. Does removing a property that does not exist notify? Spike: no.

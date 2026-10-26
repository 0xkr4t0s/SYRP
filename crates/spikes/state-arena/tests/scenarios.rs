//! Spec B1-B6. Each B4 scenario cites the public JUCE documentation statements it relies on
//! (class pages for `ValueTree` and `ValueTree::Listener` on docs.juce.com). Anything those pages
//! do not state is not asserted as parity; see the write-up's open-questions list.

use std::cell::RefCell;
use std::rc::Rc;

use syrp_state_arena_spike::{Event, EventKind, Op, Store, Subscription, TreeRef, Var};

type Seen = Rc<RefCell<Vec<String>>>;

fn describe(store: &Store, ev: &Event) -> String {
    let t = store.name(ev.tree.type_id());
    match &ev.kind {
        EventKind::PropertyChanged(id) => format!("prop {t}.{}", store.name(*id)),
        EventKind::ChildAdded(c) => format!("added {t}/{}", store.name(c.type_id())),
        EventKind::ChildRemoved(c, i) => format!("removed {t}/{}@{i}", store.name(c.type_id())),
        EventKind::ParentChanged => format!("parent {t}"),
    }
}

/// Subscribes a listener that records `"<label>: <event>"`.
fn listen(store: &Store, tree: &TreeRef, label: &'static str, seen: &Seen) -> Subscription {
    let (store, seen) = (store.clone(), seen.clone());
    tree.subscribe(move |ev| {
        seen.borrow_mut()
            .push(format!("{label}: {}", describe(&store, ev)))
    })
}

fn take(seen: &Seen) -> Vec<String> {
    std::mem::take(&mut *seen.borrow_mut())
}

fn node(store: &Store, ty: &str) -> TreeRef {
    store.create(store.id(ty))
}

#[test]
fn b1_nodes_hold_type_sorted_properties_and_ordered_children() {
    let s = Store::new();
    let root = node(&s, "Root");
    ["z", "a", "m"]
        .iter()
        .for_each(|p| root.set_property(s.id(p), Var::Int(1)));
    assert_eq!((root.num_properties(), root.type_id()), (3, s.id("Root")));
    let kids = ["A", "B", "C"].map(|t| node(&s, t));
    for (k, at) in [(0, None), (2, None), (1, Some(1))] {
        assert!(root.add_child(&kids[k], at));
    }
    let order: Vec<_> = (0..3).map(|i| root.child(i).unwrap()).collect();
    assert_eq!(order, kids);
}

#[test]
fn b2_handles_are_shallow_and_refcounted() {
    // ValueTree: copies are "a lightweight reference to a shared data container".
    let s = Store::new();
    let (root, a) = (node(&s, "Root"), node(&s, "A"));
    let a2 = a.clone();
    a2.set_property(s.id("x"), Var::Int(7));
    assert_eq!(a.property(s.id("x")), Some(Var::Int(7)));
    assert!(root.add_child(&a, None));
    assert_eq!(a.reference_count(), 3); // two handles plus the parent link
    assert!(root.remove_child(&a));
    assert_eq!(a.reference_count(), 2);
    drop((a, a2));
    assert_eq!(s.live_nodes(), 1);
}

#[test]
fn b2_add_child_rejects_parented_child_and_cycles() {
    // ValueTree::addChild: "Make sure that the child being added has first been removed from
    // any former parent before calling this, or else you'll hit an assertion."
    let s = Store::new();
    let (a, b, c) = (node(&s, "A"), node(&s, "B"), node(&s, "C"));
    assert!(a.add_child(&b, None) && b.add_child(&c, None));
    assert!(!a.add_child(&c, None) && !c.add_child(&a, None) && !a.add_child(&a, None));
    assert!(!a.add_child(&node(&Store::new(), "X"), None));
    assert_eq!(a.num_children(), 1);
}

#[test]
fn b3_every_mutation_appends_a_record() {
    let s = Store::new();
    let (p, c) = (node(&s, "P"), node(&s, "C"));
    p.set_property(s.id("x"), Var::Int(1));
    assert_eq!(s.last_record().unwrap().1, Op::PropertyChanged(s.id("x")));
    p.add_child(&c, None);
    p.remove_child(&c);
    p.remove_property(s.id("x"));
    assert_eq!(s.log_len(), 6); // prop, added + parent, removed + parent, prop
}

#[test]
fn b3_listener_mutation_is_delivered_after_current_record() {
    let s = Store::new();
    let n = node(&s, "N");
    let seen: Seen = Rc::default();
    let n2 = n.clone();
    let (x, y) = (s.id("x"), s.id("y"));
    let _first = n.subscribe(move |ev| {
        if matches!(ev.kind, EventKind::PropertyChanged(id) if id == x) {
            n2.set_property(y, Var::Int(1));
            // The mutation is applied at once; only its notification waits.
            assert_eq!(n2.property(y), Some(Var::Int(1)));
        }
    });
    let _second = listen(&s, &n, "second", &seen);
    n.set_property(x, Var::Int(1));
    assert_eq!(take(&seen), ["second: prop N.x", "second: prop N.y"]);
}

#[test]
fn b4_detached_subtree_survives_and_reattaches_elsewhere() {
    // ValueTree: "Creating a copy of another ValueTree simply creates a new reference to the
    // same underlying object"; "A ValueTree can only be a child of one parent at a time";
    // removeChild: "Removes the specified child from this tree's child-list".
    // Listener::valueTreePropertyChanged: a listener on a tree is called "for any of its
    // children, (recursively, at any depth)". valueTreeParentChanged "happens when the tree to
    // which the listener was registered is added or removed from a parent".
    let s = Store::new();
    let (old, new) = (node(&s, "Old"), node(&s, "New"));
    let sub = node(&s, "Sub");
    let leaf = node(&s, "Leaf");
    sub.add_child(&leaf, None);
    old.add_child(&sub, None);
    let held = old.child(0).unwrap(); // the reference that keeps it alive
    drop((sub, leaf));

    let seen: Seen = Rc::default();
    let _o = listen(&s, &old, "old", &seen);
    let _n = listen(&s, &new, "new", &seen);
    let _h = listen(&s, &held, "sub", &seen);

    assert!(old.remove_child(&held));
    assert_eq!(s.live_nodes(), 4);
    assert_eq!(
        held.child(0).map(|l| s.name(l.type_id())).as_deref(),
        Some("Leaf")
    );
    assert!(new.add_child(&held, None));
    held.child(0)
        .unwrap()
        .set_property(s.id("gain"), Var::Double(0.5));

    let mut got = take(&seen);
    // Which listeners fire is documented; their relative order is not, so compare as sets.
    got.sort();
    let mut want = vec![
        "old: removed Old/Sub@0",
        "sub: parent Sub",
        "new: added New/Sub",
        "sub: parent Sub",
        "sub: prop Leaf.gain",
        "new: prop Leaf.gain",
    ];
    want.sort();
    assert_eq!(got, want);
    assert_eq!(held.parent().unwrap(), new);
}

#[test]
fn b4_ancestor_listeners_see_descendant_changes() {
    // Listener::valueTreePropertyChanged: "property changes in that tree, and also for any of
    // its children, (recursively, at any depth)". valueTreeChildAdded/Removed: "for child
    // changes in both that tree and any of its children". valueTreeParentChanged: "Unlike the
    // other callbacks, it applies only to the tree to which the listener is registered".
    let s = Store::new();
    let (root, mid, leaf) = (node(&s, "Root"), node(&s, "Mid"), node(&s, "Leaf"));
    root.add_child(&mid, None);
    let seen: Seen = Rc::default();
    let _r = listen(&s, &root, "root", &seen);
    let _m = listen(&s, &mid, "mid", &seen);
    let _l = listen(&s, &leaf, "leaf", &seen);

    mid.add_child(&leaf, None);
    let mut got = take(&seen);
    got.sort();
    assert_eq!(
        got,
        [
            "leaf: parent Leaf",
            "mid: added Mid/Leaf",
            "root: added Mid/Leaf"
        ]
    );

    leaf.set_property(s.id("q"), Var::Bool(true));
    let mut got = take(&seen);
    got.sort();
    assert_eq!(
        got,
        ["leaf: prop Leaf.q", "mid: prop Leaf.q", "root: prop Leaf.q"]
    );

    root.set_property(s.id("q"), Var::Bool(true));
    assert_eq!(take(&seen), ["root: prop Root.q"]);
}

#[test]
fn b4_reclaiming_an_orphan_frees_its_slots() {
    // ValueTree::getReferenceCount: "the total number of references to the shared underlying
    // data structure". A subtree nothing refers to is gone; occupancy is our observable for it.
    let s = Store::new();
    let root = node(&s, "Root");
    let sub = node(&s, "Sub");
    for t in ["A", "B", "C"] {
        sub.add_child(&node(&s, t), None);
    }
    root.add_child(&sub, None);
    assert_eq!(s.live_nodes(), 5);
    root.remove_child(&sub);
    assert_eq!(s.live_nodes(), 5, "held by `sub`");
    drop(sub);
    assert_eq!(s.live_nodes(), 1);

    // A removed child with no other handle is reclaimed once its removal has been delivered,
    // and a listener holding it during delivery keeps it alive for exactly that long.
    let c = node(&s, "C");
    root.add_child(&c, None);
    drop(c);
    let kept: Rc<RefCell<Option<TreeRef>>> = Rc::default();
    let k = kept.clone();
    let _sub = root.subscribe(move |ev| {
        if let EventKind::ChildRemoved(c, _) = &ev.kind {
            *k.borrow_mut() = Some(c.clone());
        }
    });
    let c = root.child(0).unwrap();
    root.remove_child(&c);
    drop(c);
    assert_eq!(s.live_nodes(), 2);
    kept.borrow_mut().take();
    assert_eq!(s.live_nodes(), 1);
    s.compact_log();
    assert_eq!(s.log_len(), 0);
}

#[test]
fn b4_listener_lives_with_its_subscription() {
    // ValueTree::addListener: "When this object is deleted, all the listeners will be lost,
    // even if other references to the same ValueTree still exist." Here the subscription plays
    // the part of the ValueTree object the listener was added to.
    let s = Store::new();
    let n = node(&s, "N");
    let seen: Seen = Rc::default();
    let sub = listen(&s, &n, "l", &seen);
    n.set_property(s.id("x"), Var::Int(1));
    drop(sub);
    n.set_property(s.id("x"), Var::Int(2));
    assert_eq!(take(&seen), ["l: prop N.x"]);
}

#[path = "../benches/property_access.rs"]
mod bench;

#[test]
fn b5_property_access_measurement_runs() {
    for props in [4, 16, 64] {
        assert!(bench::measure(props, 2_000).iter().all(|t| t.is_finite()));
    }
}

#[test]
fn b6_write_up_has_the_required_sections() {
    let doc = include_str!("../../../../docs/design/0005-state-arena-spike.md");
    for heading in [
        "## What passed",
        "## What did not",
        "## Measurements",
        "## Recommendation",
        "## Open questions for the differential harness",
    ] {
        assert!(doc.contains(heading), "missing {heading}");
    }
    assert!(doc.contains("listener added during"));
}

#[test]
fn b3_listener_added_during_delivery_waits_for_the_next_record() {
    // Not a parity scenario: JUCE's documentation does not say whether a listener added during a
    // notification is called for the change being delivered. This pins the spike's choice
    // (listener lists are snapshotted per record) so the write-up's description stays true.
    let s = Store::new();
    let n = node(&s, "N");
    let seen: Seen = Rc::default();
    let late: Rc<RefCell<Option<Subscription>>> = Rc::default();
    let (s2, n2, seen2, late2) = (s.clone(), n.clone(), seen.clone(), late.clone());
    let _adder = n.subscribe(move |_| {
        if late2.borrow().is_none() {
            let sub = listen(&s2, &n2, "late", &seen2);
            *late2.borrow_mut() = Some(sub);
        }
    });
    n.set_property(s.id("x"), Var::Int(1));
    assert!(take(&seen).is_empty());
    n.set_property(s.id("x"), Var::Int(2));
    assert_eq!(take(&seen), ["late: prop N.x"]);
    late.borrow_mut().take();
}

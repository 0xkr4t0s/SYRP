//! Behaviour tests for WP-1.2-03 increment 1 (spec B1 to B3).

use syrp_loop_api::{ManualLoop, MessageThread};
use syrp_state::{Diagnostic, NodeRef, StateError, StateErrorKind, Store};
use syrp_var::{Identifier, Var};

fn id(name: &str) -> Identifier {
    Identifier::new(name)
}

fn with_mt(f: impl FnOnce(&MessageThread<'_>)) {
    ManualLoop::new().with_token(f);
}

/// Everything observable about a subtree (types, counts, properties, children, parent links).
fn snapshot(store: &Store, node: &NodeRef) -> String {
    let (ty, refs) = (
        store.node_type(node).unwrap(),
        store.reference_count(node).unwrap(),
    );
    let mut out = format!("{ty}#{refs}(");
    for (k, v) in store.properties(node).unwrap() {
        out += &format!("{k}={v:?},");
    }
    for i in 0..store.num_children(node).unwrap() {
        let c = store.child(node, i).unwrap().unwrap();
        assert_eq!(store.parent(&c).unwrap().as_ref(), Some(node));
        out += &snapshot(store, &c);
    }
    out + ")"
}

fn child_types(store: &Store, node: &NodeRef) -> Vec<Identifier> {
    let n = store.num_children(node).unwrap();
    let child = |i| store.child(node, i).unwrap().unwrap();
    (0..n)
        .map(|i| store.node_type(&child(i)).unwrap())
        .collect()
}

fn expect(r: Result<impl Sized, StateError>, kind: StateErrorKind, code: &str) {
    let err = r.map(drop).unwrap_err();
    assert_eq!((err.kind(), err.code().as_str()), (kind, code));
    assert!(err.to_string().starts_with(code));
}

#[test]
fn b1_nodes_hold_type_sorted_properties_and_ordered_children() {
    with_mt(|mt| {
        let store = Store::new();
        let root = store.create(mt, id("Root"));
        assert_eq!(store.node_type(&root).unwrap(), id("Root"));
        let names = [id("b1_zeta"), id("b1_alpha"), id("b1_mid")];
        for (i, n) in names.iter().rev().enumerate() {
            store.set_property(mt, &root, *n, i as i32).unwrap();
        }
        let keys: Vec<_> = store.properties(&root).unwrap().map(|(k, _)| k).collect();
        let mut sorted = names.to_vec();
        sorted.sort();
        assert_eq!((keys, store.num_properties(&root).unwrap()), (sorted, 3));
        assert!(store.has_property(&root, id("b1_mid")).unwrap());
        store.set_property(mt, &root, id("b1_mid"), "t").unwrap();
        assert_eq!(
            store.property(&root, id("b1_mid")).unwrap(),
            Some(Var::from("t"))
        );
        let removed = store.remove_property(mt, &root, id("b1_mid")).unwrap();
        assert_eq!(removed, Some(Var::from("t")));
        assert_eq!(
            store.remove_property(mt, &root, id("b1_mid")).unwrap(),
            None
        );

        let [a, b, c, d] = ["A", "B", "C", "D"].map(|t| store.create(mt, id(t)));
        store.add_child(mt, &root, &a, None).unwrap();
        store.add_child(mt, &root, &b, Some(0)).unwrap();
        store.add_child(mt, &root, &c, Some(1)).unwrap();
        store.add_child(mt, &root, &d, Some(1000)).unwrap(); // past the end appends
        assert_eq!(child_types(&store, &root), ["B", "C", "A", "D"].map(id));
        assert_eq!(store.index_of(&root, &a).unwrap(), Some(2));
        assert_eq!(store.child(&root, 4).unwrap(), None);
        let g = store.create(mt, id("G"));
        store.add_child(mt, &a, &g, None).unwrap();
        store.remove_child(mt, &root, &a).unwrap();
        assert_eq!(store.parent(&a).unwrap(), None);
        assert_eq!(store.parent(&g).unwrap(), Some(a.clone()));
        assert_eq!(child_types(&store, &root), ["B", "C", "D"].map(id));
    });
}

#[test]
fn b1_handles_are_shallow_with_identity_equality_and_keys_upgrade() {
    with_mt(|mt| {
        let store = Store::new();
        let a = store.create(mt, id("A"));
        let a2 = a.clone();
        store.set_property(mt, &a2, id("x"), 1).unwrap();
        assert_eq!(store.property(&a, id("x")).unwrap(), Some(Var::from(1)));
        assert_eq!(a, a2);
        let b = store.create(mt, id("A")); // same type and properties, other node
        store.set_property(mt, &b, id("x"), 1).unwrap();
        assert_ne!(a, b);
        assert_eq!(store.upgrade(a.key()).unwrap(), a);
        // An eq_same_type value is a no-op; another variant is a change.
        store.set_property(mt, &a, id("x"), 1.0).unwrap();
        assert!(store.property(&a, id("x")).unwrap().unwrap().is_f64());
    });
}

#[test]
fn b2_add_child_errors_leave_the_store_unchanged() {
    with_mt(|mt| {
        let store = Store::new();
        let [root, a, b, other] = ["Root", "A", "B", "Other"].map(|t| store.create(mt, id(t)));
        store.add_child(mt, &root, &a, None).unwrap();
        store.add_child(mt, &a, &b, None).unwrap();
        let state = |s: &Store| (snapshot(s, &root), snapshot(s, &other), s.live_nodes());
        let before = state(&store);
        let r = store.add_child(mt, &other, &b, None);
        expect(r, StateErrorKind::ChildHasParent, "SYRP-STATE-0001");
        for (parent, child) in [(&b, &root), (&root, &root)] {
            let r = store.add_child(mt, parent, child, None);
            expect(r, StateErrorKind::WouldCreateCycle, "SYRP-STATE-0002");
        }
        assert_eq!(state(&store), before);
    });
}

#[test]
fn b2_foreign_nodes_and_dead_keys_are_coded_errors() {
    with_mt(|mt| {
        let (store, other_store) = (Store::new(), Store::new());
        let mine = store.create(mt, id("Mine"));
        let theirs = other_store.create(mt, id("Theirs"));
        let foreign = |r| expect(r, StateErrorKind::ForeignNode, "SYRP-STATE-0003");
        foreign(store.add_child(mt, &mine, &theirs, None));
        foreign(store.add_child(mt, &theirs, &mine, None));
        foreign(store.set_property(mt, &theirs, id("x"), 1));
        foreign(store.remove_child(mt, &mine, &theirs));
        foreign(store.property(&theirs, id("x")).map(drop));
        foreign(store.index_of(&mine, &theirs).map(drop));
        foreign(store.parent(&theirs).map(drop));
        assert_eq!((store.live_nodes(), other_store.live_nodes()), (1, 1));
        assert_eq!(snapshot(&other_store, &theirs), "Theirs#1()");

        let key = theirs.key();
        drop(theirs);
        expect(
            other_store.upgrade(key),
            StateErrorKind::DeadKey,
            "SYRP-STATE-0004",
        );
    });
}

#[test]
fn b2_remove_child_errors_leave_the_store_unchanged() {
    with_mt(|mt| {
        let store = Store::new();
        let [root, a, loose] = ["Root", "A", "Loose"].map(|t| store.create(mt, id(t)));
        store.add_child(mt, &root, &a, None).unwrap();
        let before = snapshot(&store, &root);
        let not_child = |r| expect(r, StateErrorKind::NotAChild, "SYRP-STATE-0005");
        not_child(store.remove_child(mt, &root, &loose));
        not_child(store.remove_child(mt, &a, &root));
        assert_eq!(snapshot(&store, &root), before);
    });
}

#[test]
fn b3_counts_are_handles_plus_the_parent_link() {
    with_mt(|mt| {
        let store = Store::new();
        let root = store.create(mt, id("Root"));
        let child = store.create(mt, id("Child"));
        let count = || store.reference_count(&child).unwrap();
        assert_eq!(count(), 1);
        let extra = child.clone();
        store.add_child(mt, &root, &child, None).unwrap();
        let via_parent = store.child(&root, 0).unwrap().unwrap();
        assert_eq!(count(), 4); // three handles and the parent link
        drop((extra, via_parent));
        store.remove_child(mt, &root, &child).unwrap();
        assert_eq!(count(), 1);
        let key = child.key();
        drop(child);
        assert!(store.upgrade(key).is_err());
        assert_eq!(store.live_nodes(), 1);
    });
}

#[test]
fn b3_detached_subtree_lives_while_held_and_is_reclaimed_after() {
    with_mt(|mt| {
        let store = Store::new();
        let [root, sub, other] = ["Root", "Sub", "Other"].map(|t| store.create(mt, id(t)));
        store.add_child(mt, &root, &sub, None).unwrap();
        for t in ["X", "Y", "Z"] {
            store
                .add_child(mt, &sub, &store.create(mt, id(t)), None)
                .unwrap();
        }
        store.remove_child(mt, &root, &sub).unwrap();
        assert_eq!(store.live_nodes(), 6);
        store.add_child(mt, &other, &sub, None).unwrap(); // re-attached intact
        assert_eq!(child_types(&store, &sub), ["X", "Y", "Z"].map(id));
        store.remove_child(mt, &other, &sub).unwrap();
        drop(sub);
        assert_eq!(store.live_nodes(), 2); // Sub and its three leaves are freed

        let held = store.create(mt, id("Held"));
        store.add_child(mt, &root, &held, None).unwrap();
        let unheld = store.create(mt, id("Unheld"));
        store.add_child(mt, &root, &unheld, None).unwrap();
        let key = unheld.key();
        drop(unheld);
        for _ in 0..2 {
            let first = store.child(&root, 0).unwrap().unwrap();
            store.remove_child(mt, &root, &first).unwrap();
        }
        assert!(store.upgrade(key).is_err()); // freed once its last handle went
        assert_eq!(store.live_nodes(), 3); // Root, Other, Held
        drop((root, other, held));
        assert_eq!(store.live_nodes(), 0);
    });
}

#[test]
fn b3_freed_parent_leaves_held_child_parentless_syrp_choice() {
    with_mt(|mt| {
        let store = Store::new();
        let [parent, kept, gone] = ["Parent", "Kept", "Gone"].map(|t| store.create(mt, id(t)));
        store.add_child(mt, &parent, &kept, None).unwrap();
        store.add_child(mt, &parent, &gone, None).unwrap();
        drop((gone, parent));
        // Syrp choice (spike open question 6): the held child survives with no parent and loses
        // the parent link's count; the unheld sibling is freed.
        assert_eq!(store.parent(&kept).unwrap(), None);
        assert_eq!(
            (store.reference_count(&kept).unwrap(), store.live_nodes()),
            (1, 1)
        );
        let new_parent = store.create(mt, id("New"));
        store.add_child(mt, &new_parent, &kept, None).unwrap();
    });
}

#[test]
fn b3_freeing_a_deep_chain_does_not_recurse() {
    with_mt(|mt| {
        let store = Store::new();
        // Built bottom-up so each cycle check is O(1): the new parent has no ancestors yet.
        let mut root = store.create(mt, id("Chain"));
        for _ in 0..100_000 {
            let parent = store.create(mt, id("Chain"));
            store.add_child(mt, &parent, &root, None).unwrap();
            root = parent;
        }
        assert_eq!(store.live_nodes(), 100_001);
        drop(root);
        assert_eq!(store.live_nodes(), 0);
    });
}

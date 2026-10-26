//! Behaviour tests for WP-1.2-03 increment 2: the rest of B1, B4 and B5.

use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use syrp_loop_api::{ManualLoop, MessageThread};
use syrp_state::{Change, NodeRef, StateErrorKind, Store};
use syrp_var::{Identifier, Var};

fn id(name: &str) -> Identifier {
    Identifier::new(name)
}

fn with_mt(f: impl FnOnce(&MessageThread<'_>)) {
    ManualLoop::new().with_token(f);
}

/// A shared journal that listeners write `tag:change` lines to.
type Journal = Rc<RefCell<Vec<String>>>;

fn short(c: &Change) -> String {
    let s = format!("{c:?}");
    s[..s.find(' ').unwrap_or(s.len())].to_string()
}

fn listen(
    store: &Store,
    mt: &MessageThread<'_>,
    node: &NodeRef,
    tag: &str,
    j: &Journal,
) -> syrp_state::Subscription {
    let (tag, j) = (tag.to_string(), Rc::clone(j));
    store
        .subscribe(mt, node, move |_, _, c| {
            j.borrow_mut().push(format!("{tag}:{}", short(c)))
        })
        .unwrap()
}

#[test]
fn b1_queries_moves_and_bulk_removals() {
    with_mt(|mt| {
        let store = Store::new();
        let [root, a, b, c, g] = ["Root", "A", "B", "C", "G"].map(|t| store.create(mt, id(t)));
        for n in [&a, &b, &c] {
            store.add_child(mt, &root, n, None).unwrap();
        }
        store.add_child(mt, &a, &g, None).unwrap();
        store.set_property(mt, &b, id("tag"), 7).unwrap();
        assert_eq!(
            store.child_with_type(&root, id("C")).unwrap(),
            Some(c.clone())
        );
        assert_eq!(store.child_with_type(&root, id("Z")).unwrap(), None);
        let by_prop = |v: i32| {
            store
                .child_with_property(&root, id("tag"), &Var::from(v))
                .unwrap()
        };
        assert_eq!((by_prop(7), by_prop(8)), (Some(b.clone()), None));
        assert_eq!(
            (store.root(&g).unwrap(), store.root(&root).unwrap()),
            (root.clone(), root.clone())
        );
        let anc = |x: &NodeRef, y: &NodeRef| store.is_ancestor_of(x, y).unwrap();
        assert!(
            anc(&root, &g) && anc(&a, &g) && !anc(&b, &g) && !anc(&g, &root) && !anc(&root, &root)
        );

        store.move_child(mt, &root, 0, usize::MAX).unwrap(); // past the end moves to the end
        store.move_child(mt, &root, 2, 1).unwrap();
        assert_eq!(store.index_of(&root, &a).unwrap(), Some(1));
        assert_eq!(store.index_of(&root, &c).unwrap(), Some(2));
        let r = store.move_child(mt, &root, 3, 0).unwrap_err();
        assert_eq!(
            (r.kind(), syrp_state::Diagnostic::code(&r).as_str()),
            (StateErrorKind::IndexOutOfRange, "SYRP-STATE-0006")
        );

        store.remove_all_properties(mt, &b).unwrap();
        assert_eq!(store.num_properties(&b).unwrap(), 0);
        store.remove_all_children(mt, &root).unwrap();
        assert_eq!(
            (
                store.num_children(&root).unwrap(),
                store.parent(&a).unwrap()
            ),
            (0, None)
        );
        drop((a, b, c, g));
        assert_eq!(store.live_nodes(), 1);
    });
}

#[test]
fn b4_every_mutation_appends_reversible_records_in_sequence() {
    with_mt(|mt| {
        let store = Store::new();
        let [p, c] = ["P", "C"].map(|t| store.create(mt, id(t)));
        let other = store.create(mt, id("O"));
        let mut cursor = store.cursor();
        let (pk, ck, x) = (p.key(), c.key(), id("x"));
        store.set_property(mt, &p, x, 1).unwrap();
        store.set_property(mt, &p, x, 2).unwrap();
        store.remove_property(mt, &p, x).unwrap();
        store.add_child(mt, &p, &c, None).unwrap();
        store.add_child(mt, &p, &other, Some(0)).unwrap();
        store.move_child(mt, &p, 0, 1).unwrap();
        store.remove_child(mt, &p, &c).unwrap();
        let records: Vec<_> = cursor.by_ref().collect();
        assert!(records.windows(2).all(|w| w[1].seq() == w[0].seq() + 1));
        let ch: Vec<&Change> = records.iter().map(|r| r.change()).collect();
        let (one, two) = (Var::from(1), Var::from(2));
        assert!(
            matches!(ch[0], Change::PropertyChanged { node, id, old: None, new, .. } if *node == pk && *id == x && *new == one)
        );
        assert!(
            matches!(ch[1], Change::PropertyChanged { old: Some(o), new, .. } if *o == one && *new == two)
        );
        assert!(
            matches!(ch[2], Change::PropertyRemoved { node, old, .. } if *node == pk && *old == two)
        );
        assert!(
            matches!(ch[3], Change::ChildAdded { parent, child, index: 0, .. } if *parent == pk && *child == ck)
        );
        assert!(matches!(ch[4], Change::ParentChanged { node, .. } if *node == ck));
        assert!(matches!(ch[5], Change::ChildAdded { index: 0, .. }));
        assert!(matches!(ch[7], Change::ChildMoved { from: 0, to: 1, .. }));
        assert!(matches!(ch[8], Change::ChildRemoved { child, index: 0, .. } if *child == ck));
        assert!(matches!(ch[9], Change::ParentChanged { node, .. } if *node == ck));
        assert_eq!(ch.len(), 10);
        // Errors append nothing.
        store.add_child(mt, &p, &other, None).unwrap_err();
        assert!(cursor.next().is_none());
    });
}

#[test]
fn b4_no_op_set_and_missing_remove_append_nothing_syrp_choice() {
    with_mt(|mt| {
        let store = Store::new();
        let n = store.create(mt, id("N"));
        store.set_property(mt, &n, id("x"), 1).unwrap();
        let mut cursor = store.cursor();
        store.set_property(mt, &n, id("x"), 1).unwrap(); // open question 7
        store.remove_property(mt, &n, id("absent")).unwrap(); // open question 9
        store.move_child(mt, &n, 0, 0).unwrap_err();
        assert!(cursor.next().is_none());
    });
}

#[test]
fn b4_records_are_kept_until_every_cursor_has_passed_them() {
    with_mt(|mt| {
        let store = Store::new();
        let n = store.create(mt, id("N"));
        store.set_property(mt, &n, id("x"), 1).unwrap();
        assert_eq!(store.log_len(), 0); // delivered and no cursor: dropped
        let (mut fast, mut slow) = (store.cursor(), store.cursor());
        for v in 2..5 {
            store.set_property(mt, &n, id("x"), v).unwrap();
        }
        assert_eq!(fast.by_ref().count(), 3);
        assert_eq!(store.log_len(), 3);
        slow.next().unwrap();
        assert_eq!(store.log_len(), 2);
        drop(slow);
        assert_eq!(store.log_len(), 0);
    });
}

#[test]
fn b4_undelivered_records_pin_the_nodes_they_name() {
    with_mt(|mt| {
        let store = Store::new();
        let p = store.create(mt, id("P"));
        let child = store.create(mt, id("C"));
        let key = child.key();
        store.add_child(mt, &p, &child, None).unwrap();
        drop(child);
        let alive = Rc::new(RefCell::new(Vec::new()));
        let a = Rc::clone(&alive);
        let _sub = store
            .subscribe(mt, &p, move |s, _, _| {
                a.borrow_mut().push(s.upgrade(key).is_ok())
            })
            .unwrap();
        store.remove_all_children(mt, &p).unwrap();
        // The removed child, held by nothing else, lives while its removal is delivered.
        assert_eq!(*alive.borrow(), [true]);
        assert!(store.upgrade(key).is_err());
        assert_eq!(store.live_nodes(), 1);
    });
}

#[test]
fn b5_listeners_on_node_and_ancestors_in_order_parent_changed_only_own() {
    with_mt(|mt| {
        let store = Store::new();
        let [root, mid, leaf] = ["R", "M", "L"].map(|t| store.create(mt, id(t)));
        store.add_child(mt, &root, &mid, None).unwrap();
        let j: Journal = Rc::default();
        let _subs = [
            listen(&store, mt, &root, "root", &j),
            listen(&store, mt, &leaf, "leaf1", &j),
            listen(&store, mt, &mid, "mid", &j),
            listen(&store, mt, &leaf, "leaf2", &j),
        ];
        store.add_child(mt, &mid, &leaf, None).unwrap();
        store.set_property(mt, &leaf, id("x"), 1).unwrap();
        store.set_property(mt, &root, id("x"), 1).unwrap(); // not sent down
        let expected = [
            "mid:ChildAdded",
            "root:ChildAdded",
            "leaf1:ParentChanged",
            "leaf2:ParentChanged",
            "leaf1:PropertyChanged",
            "leaf2:PropertyChanged",
            "mid:PropertyChanged",
            "root:PropertyChanged",
            "root:PropertyChanged",
        ];
        assert_eq!(*j.borrow(), expected);
    });
}

#[test]
fn b5_listener_mutation_is_delivered_after_the_current_records_listeners() {
    with_mt(|mt| {
        let store = Store::new();
        let n = store.create(mt, id("N"));
        let j: Journal = Rc::default();
        let (j1, target) = (Rc::clone(&j), n.clone());
        let _first = store
            .subscribe(mt, &n, move |s, mt, c| {
                j1.borrow_mut().push(format!("first:{}", short(c)));
                if matches!(c, Change::PropertyChanged { new, .. } if *new == Var::from(1)) {
                    s.set_property(mt, &target, id("x"), 2).unwrap();
                    assert_eq!(s.property(&target, id("x")).unwrap(), Some(Var::from(2)));
                }
            })
            .unwrap();
        let _second = listen(&store, mt, &n, "second", &j);
        store.set_property(mt, &n, id("x"), 1).unwrap();
        let expected = [
            "first:PropertyChanged",
            "second:PropertyChanged",
            "first:PropertyChanged",
            "second:PropertyChanged",
        ];
        assert_eq!(*j.borrow(), expected);
    });
}

#[test]
fn b5_subscription_counts_and_unsubscribes_on_drop() {
    with_mt(|mt| {
        let store = Store::new();
        let n = store.create(mt, id("N"));
        let key = n.key();
        let j: Journal = Rc::default();
        let sub = listen(&store, mt, &n, "n", &j);
        assert_eq!(store.reference_count(&n).unwrap(), 2);
        drop(n);
        let n = store.upgrade(key).unwrap(); // the subscription kept it alive
        drop(sub);
        store.set_property(mt, &n, id("x"), 1).unwrap();
        assert!(j.borrow().is_empty());
        assert_eq!(store.reference_count(&n).unwrap(), 1);
    });
}

#[test]
fn b5_a_panicking_listener_does_not_wedge_the_store() {
    with_mt(|mt| {
        let store = Store::new();
        let [p, c] = ["P", "C"].map(|t| store.create(mt, id(t)));
        let j: Journal = Rc::default();
        let _panics = store
            .subscribe(mt, &p, |_, _, c| {
                assert!(!matches!(c, Change::ChildAdded { .. }), "boom")
            })
            .unwrap();
        let _child = listen(&store, mt, &c, "c", &j);
        let r = catch_unwind(AssertUnwindSafe(|| store.add_child(mt, &p, &c, None)));
        assert!(r.is_err());
        assert!(j.borrow().is_empty()); // ParentChanged not delivered yet
        store.set_property(mt, &c, id("x"), 1).unwrap(); // delivers the rest
        assert_eq!(*j.borrow(), ["c:ParentChanged", "c:PropertyChanged"]);
        assert_eq!(store.reference_count(&c).unwrap(), 3); // handle, parent link, subscription
    });
}

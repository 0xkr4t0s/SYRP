//! Behaviour tests for WP-1.2-04 increment 1 (spec B1 to B3).

use std::cell::RefCell;
use std::rc::Rc;

use syrp_loop_api::{ManualLoop, MessageThread};
use syrp_state::{Change, Diagnostic, NodeRef, StateErrorKind, Store, UndoManager};
use syrp_var::{Identifier, Var};

fn id(name: &str) -> Identifier {
    Identifier::new(name)
}

fn with_mt(f: impl FnOnce(&MessageThread<'_>)) {
    ManualLoop::new().with_token(f);
}

fn types(store: &Store, node: &NodeRef) -> Vec<String> {
    let n = store.num_children(node).unwrap();
    let child = |i| store.child(node, i).unwrap().unwrap();
    (0..n)
        .map(|i| store.node_type(&child(i)).unwrap().to_string())
        .collect()
}

#[test]
fn b1_with_undo_records_and_plain_calls_do_not() {
    with_mt(|mt| {
        let store = Store::new();
        let mut undo = UndoManager::new();
        let node = store.create(mt, id("N"));
        store.set_property(mt, &node, id("plain"), 1).unwrap();
        assert!(!undo.can_undo());
        store
            .set_property_with_undo(mt, &mut undo, &node, id("x"), 1)
            .unwrap();
        // A no-op or a failed call records nothing.
        store
            .set_property_with_undo(mt, &mut undo, &node, id("x"), 1)
            .unwrap();
        let r = store.move_child_with_undo(mt, &mut undo, &node, 0, 1);
        assert_eq!(r.unwrap_err().kind(), StateErrorKind::IndexOutOfRange);
        assert!(undo.undo(mt).unwrap());
        assert!(!undo.can_undo());
        assert_eq!(
            store.property(&node, id("plain")).unwrap(),
            Some(Var::from(1))
        );
        assert!(!store.has_property(&node, id("x")).unwrap());
    });
}

#[test]
fn b1_the_manager_keeps_named_nodes_alive_until_discarded() {
    with_mt(|mt| {
        let store = Store::new();
        let mut undo = UndoManager::new();
        let p = store.create(mt, id("P"));
        let c = store.create(mt, id("C"));
        store.add_child(mt, &p, &c, None).unwrap();
        let key = c.key();
        drop(c);
        store
            .remove_all_children_with_undo(mt, &mut undo, &p)
            .unwrap();
        let held = store.upgrade(key).unwrap(); // kept by the manager's records
        assert_eq!(store.reference_count(&held).unwrap(), 3); // handle, ChildRemoved, ParentChanged
        drop(held);
        drop(undo);
        assert!(store.upgrade(key).is_err());
        assert_eq!(store.live_nodes(), 1);
    });
}

#[test]
fn b1_a_manager_serves_one_store() {
    with_mt(|mt| {
        let (one, two) = (Store::new(), Store::new());
        let mut undo = UndoManager::new();
        let a = one.create(mt, id("A"));
        let b = two.create(mt, id("B"));
        one.set_property_with_undo(mt, &mut undo, &a, id("x"), 1)
            .unwrap();
        let err = two
            .set_property_with_undo(mt, &mut undo, &b, id("x"), 1)
            .unwrap_err();
        assert_eq!(err.kind(), StateErrorKind::ForeignUndoManager);
        assert_eq!(err.code().as_str(), "SYRP-STATE-0007");
        assert!(!two.has_property(&b, id("x")).unwrap()); // unchanged
    });
}

#[test]
fn b2_undo_reverses_in_reverse_order_and_redo_reapplies_with_notifications() {
    with_mt(|mt| {
        let store = Store::new();
        let mut undo = UndoManager::new();
        let [root, a, b] = ["Root", "A", "B"].map(|t| store.create(mt, id(t)));
        let heard: Rc<RefCell<Vec<String>>> = Rc::default();
        let h = Rc::clone(&heard);
        let _sub = store
            .subscribe(mt, &root, move |_, _, c| {
                h.borrow_mut().push(format!("{c:?}"))
            })
            .unwrap();
        undo.begin_new_transaction("build");
        store
            .add_child_with_undo(mt, &mut undo, &root, &a, None)
            .unwrap();
        store
            .add_child_with_undo(mt, &mut undo, &root, &b, Some(0))
            .unwrap();
        store
            .move_child_with_undo(mt, &mut undo, &root, 0, 1)
            .unwrap();
        store
            .set_property_with_undo(mt, &mut undo, &b, id("x"), 1)
            .unwrap();
        assert_eq!(types(&store, &root), ["A", "B"]);
        heard.borrow_mut().clear();

        assert!(undo.undo(mt).unwrap());
        assert!(types(&store, &root).is_empty());
        assert!(!store.has_property(&b, id("x")).unwrap());
        assert!(!heard.borrow().is_empty()); // undo is an ordinary, notified mutation
        assert!(!undo.can_undo() && undo.can_redo()); // and is not recorded itself
        assert_eq!(undo.redo_description(), Some("build"));

        assert!(undo.redo(mt).unwrap());
        assert_eq!(types(&store, &root), ["A", "B"]);
        assert_eq!(store.property(&b, id("x")).unwrap(), Some(Var::from(1)));
        assert!(undo.can_undo() && !undo.can_redo());
    });
}

#[test]
fn b2_a_new_action_after_undo_clears_redo() {
    with_mt(|mt| {
        let store = Store::new();
        let mut undo = UndoManager::new();
        let n = store.create(mt, id("N"));
        for (i, name) in ["one", "two"].into_iter().enumerate() {
            undo.begin_new_transaction(name);
            store
                .set_property_with_undo(mt, &mut undo, &n, id(name), i as i32)
                .unwrap();
        }
        undo.undo(mt).unwrap();
        assert_eq!(
            (undo.undo_description(), undo.redo_description()),
            (Some("one"), Some("two"))
        );
        undo.begin_new_transaction("three");
        store
            .set_property_with_undo(mt, &mut undo, &n, id("three"), 3)
            .unwrap();
        assert!(!undo.can_redo());
        assert!(!undo.redo(mt).unwrap());
        assert!(!store.has_property(&n, id("two")).unwrap());
    });
}

#[test]
fn b2_undo_reattaches_a_removed_detached_child_at_its_old_index() {
    with_mt(|mt| {
        let store = Store::new();
        let mut undo = UndoManager::new();
        let root = store.create(mt, id("Root"));
        for t in ["A", "B", "C"] {
            store
                .add_child(mt, &root, &store.create(mt, id(t)), None)
                .unwrap();
        }
        let b = store.child(&root, 1).unwrap().unwrap();
        store
            .add_child(mt, &b, &store.create(mt, id("Leaf")), None)
            .unwrap();
        store
            .remove_child_with_undo(mt, &mut undo, &root, &b)
            .unwrap();
        drop(b); // only the manager keeps the removed subtree now
        assert_eq!(store.live_nodes(), 5);
        undo.undo(mt).unwrap();
        assert_eq!(types(&store, &root), ["A", "B", "C"]);
        let b = store.child(&root, 1).unwrap().unwrap();
        assert_eq!(types(&store, &b), ["Leaf"]);
    });
}

#[test]
fn b3_consecutive_sets_of_one_property_coalesce_into_one_step() {
    with_mt(|mt| {
        let store = Store::new();
        let mut undo = UndoManager::new();
        let n = store.create(mt, id("N"));
        store.set_property(mt, &n, id("gain"), 0.0).unwrap();
        let mut cursor = store.cursor();
        undo.begin_new_transaction("drag");
        for v in 1..=10 {
            store
                .set_property_with_undo(mt, &mut undo, &n, id("gain"), f64::from(v) / 10.0)
                .unwrap();
        }
        store
            .set_property_with_undo(mt, &mut undo, &n, id("other"), 1)
            .unwrap();
        store
            .set_property_with_undo(mt, &mut undo, &n, id("gain"), 2.0)
            .unwrap(); // not consecutive
        let logged = cursor.by_ref().count();
        undo.undo(mt).unwrap();
        // Three records left after merging, so undo logs three changes.
        let undone: Vec<_> = cursor.by_ref().map(|r| r.change().clone()).collect();
        assert_eq!((logged, undone.len()), (12, 3));
        assert!(
            matches!(&undone[2], Change::PropertyChanged { new, .. } if *new == Var::from(0.0))
        );
        assert_eq!(
            store.property(&n, id("gain")).unwrap(),
            Some(Var::from(0.0))
        );
        assert!(!undo.can_undo());
    });
}

#[test]
fn b3_a_transaction_that_cancels_out_is_kept_until_the_next_starts_syrp_choice() {
    with_mt(|mt| {
        let store = Store::new();
        let mut undo = UndoManager::new();
        let n = store.create(mt, id("N"));
        store.set_property(mt, &n, id("x"), 1).unwrap();
        undo.begin_new_transaction("first");
        store
            .set_property_with_undo(mt, &mut undo, &n, id("y"), 1)
            .unwrap();
        undo.begin_new_transaction("wiggle");
        store
            .set_property_with_undo(mt, &mut undo, &n, id("x"), 5)
            .unwrap();
        store
            .set_property_with_undo(mt, &mut undo, &n, id("x"), 1)
            .unwrap(); // back to start
        assert_eq!(undo.undo_description(), Some("wiggle")); // kept, with no effect
        undo.begin_new_transaction("next");
        store
            .set_property_with_undo(mt, &mut undo, &n, id("z"), 1)
            .unwrap();
        undo.undo(mt).unwrap();
        assert_eq!(undo.undo_description(), Some("first")); // "wiggle" was dropped
    });
}

#[test]
fn b2_work_after_undo_or_redo_starts_a_new_transaction() {
    with_mt(|mt| {
        let store = Store::new();
        let mut undo = UndoManager::new();
        let n = store.create(mt, id("N"));
        undo.begin_new_transaction("a");
        store
            .set_property_with_undo(mt, &mut undo, &n, id("x"), 1)
            .unwrap();
        undo.begin_new_transaction("b");
        store
            .set_property_with_undo(mt, &mut undo, &n, id("y"), 1)
            .unwrap();
        undo.undo(mt).unwrap();
        store
            .set_property_with_undo(mt, &mut undo, &n, id("z"), 1)
            .unwrap(); // no begin
        undo.undo(mt).unwrap(); // reverts z only
        assert!(
            store.has_property(&n, id("x")).unwrap() && !store.has_property(&n, id("z")).unwrap()
        );
        undo.redo(mt).unwrap();
        store
            .set_property_with_undo(mt, &mut undo, &n, id("w"), 1)
            .unwrap();
        undo.undo(mt).unwrap(); // reverts w only
        assert!(
            store.has_property(&n, id("z")).unwrap() && !store.has_property(&n, id("w")).unwrap()
        );
        assert_eq!(undo.undo_description(), Some(""));
    });
}

#[test]
fn b1_a_nested_call_with_another_manager_takes_only_its_own_records() {
    with_mt(|mt| {
        let store = Store::new();
        let mut outer = UndoManager::new();
        let [n, mirror] = ["N", "Mirror"].map(|t| store.create(mt, id(t)));
        let inner = Rc::new(RefCell::new(UndoManager::new()));
        let (inner2, m) = (Rc::clone(&inner), mirror.clone());
        let _sub = store
            .subscribe(mt, &n, move |s, mt, c| {
                if let Change::PropertyChanged { new, .. } = c {
                    let mut u = inner2.borrow_mut();
                    s.set_property_with_undo(mt, &mut u, &m, id("copy"), new.clone())
                        .unwrap();
                }
            })
            .unwrap();
        store
            .set_property_with_undo(mt, &mut outer, &n, id("x"), 7)
            .unwrap();
        assert_eq!(
            store.property(&mirror, id("copy")).unwrap(),
            Some(Var::from(7))
        );
        assert!(outer.undo(mt).unwrap()); // the outer manager recorded its own change
        assert!(!store.has_property(&n, id("x")).unwrap());
        assert!(store.has_property(&mirror, id("copy")).unwrap()); // not the inner one
        assert!(inner.borrow_mut().undo(mt).unwrap());
        assert!(!store.has_property(&mirror, id("copy")).unwrap());
    });
}

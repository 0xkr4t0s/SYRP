//! WP-1.2-04 B6: the spike's scenarios (docs/design/0005) with undo and redo between steps,
//! and the undo/redo proptest.

use std::cell::RefCell;
use std::rc::Rc;

use proptest::prelude::*;
use syrp_loop_api::{ManualLoop, MessageThread};
use syrp_state::{Change, NodeRef, Store, UndoManager};
use syrp_var::Identifier;

fn id(name: &str) -> Identifier {
    Identifier::new(name)
}

/// Deep copies of `nodes`, to compare against later with `is_equivalent`.
fn snapshot(store: &Store, mt: &MessageThread<'_>, nodes: &[&NodeRef]) -> Vec<NodeRef> {
    nodes
        .iter()
        .map(|n| store.deep_copy(mt, n).unwrap())
        .collect()
}

fn same(store: &Store, nodes: &[&NodeRef], snap: &[NodeRef]) -> bool {
    nodes
        .iter()
        .zip(snap)
        .all(|(n, s)| store.is_equivalent(n, s).unwrap())
}

/// Runs `step` as one transaction, then checks that undo restores `nodes` and redo re-applies.
fn step_with_undo(
    store: &Store,
    mt: &MessageThread<'_>,
    undo: &mut UndoManager,
    nodes: &[&NodeRef],
    step: impl FnOnce(&mut UndoManager),
) {
    let before = snapshot(store, mt, nodes);
    undo.begin_new_transaction("step");
    step(undo);
    let after = snapshot(store, mt, nodes);
    assert!(undo.undo(mt).unwrap());
    assert!(
        same(store, nodes, &before),
        "undo restores the step's start"
    );
    assert!(undo.redo(mt).unwrap());
    assert!(same(store, nodes, &after), "redo re-applies the step");
}

#[test]
fn b6_detached_subtree_reattached_elsewhere_with_undo_between_steps() {
    ManualLoop::new().with_token(|mt| {
        let store = Store::new();
        let mut undo = UndoManager::new();
        let [old, new, sub, leaf] = ["Old", "New", "Sub", "Leaf"].map(|t| store.create(mt, id(t)));
        store.add_child(mt, &old, &sub, None).unwrap();
        store.add_child(mt, &sub, &leaf, None).unwrap();
        let heard: Rc<RefCell<Vec<String>>> = Rc::default();
        let subs: Vec<_> = [("old", &old), ("new", &new), ("sub", &sub)]
            .into_iter()
            .map(|(tag, n)| {
                let h = Rc::clone(&heard);
                let f = move |_: &Store, _: &MessageThread<'_>, c: &Change| {
                    let s = format!("{c:?}");
                    h.borrow_mut()
                        .push(format!("{tag}:{}", &s[..s.find(' ').unwrap_or(s.len())]));
                };
                store.subscribe(mt, n, f).unwrap()
            })
            .collect();
        let trees = [&old, &new, &sub];
        drop(leaf);
        step_with_undo(&store, mt, &mut undo, &trees, |u| {
            store.remove_child_with_undo(mt, u, &old, &sub).unwrap();
        });
        step_with_undo(&store, mt, &mut undo, &trees, |u| {
            store.add_child_with_undo(mt, u, &new, &sub, None).unwrap();
        });
        heard.borrow_mut().clear();
        let leaf = store.child(&sub, 0).unwrap().unwrap();
        step_with_undo(&store, mt, &mut undo, &trees, |u| {
            store
                .set_property_with_undo(mt, u, &leaf, id("gain"), 0.5)
                .unwrap();
        });
        // The change, its undo and its redo reach the subtree and its new parent only.
        let expected = [
            "sub:PropertyChanged",
            "new:PropertyChanged",
            "sub:PropertyRemoved", // undo removes the property the step added
            "new:PropertyRemoved",
            "sub:PropertyChanged",
            "new:PropertyChanged",
        ];
        assert_eq!(*heard.borrow(), expected);
        // Undo everything: the subtree is back under its old parent.
        while undo.undo(mt).unwrap() {}
        assert_eq!(store.parent(&sub).unwrap(), Some(old.clone()));
        assert!(!store.has_property(&leaf, id("gain")).unwrap());
        drop(subs);
    });
}

#[test]
fn b6_orphan_kept_by_the_history_is_reclaimed_when_it_is_cleared() {
    ManualLoop::new().with_token(|mt| {
        let store = Store::new();
        let mut undo = UndoManager::new();
        let root = store.create(mt, id("Root"));
        let orphan = store.create(mt, id("Orphan"));
        store.add_child(mt, &root, &orphan, None).unwrap();
        for t in ["A", "B", "C"] {
            store
                .add_child(mt, &orphan, &store.create(mt, id(t)), None)
                .unwrap();
        }
        drop(orphan);
        store
            .remove_all_children_with_undo(mt, &mut undo, &root)
            .unwrap();
        assert_eq!(store.live_nodes(), 5); // the history holds the removed subtree
        undo.undo(mt).unwrap();
        undo.redo(mt).unwrap();
        assert_eq!(store.live_nodes(), 5);
        undo.clear_undo_history();
        assert_eq!(store.live_nodes(), 1);
    });
}

#[derive(Debug, Clone)]
enum Op {
    Set(usize, u8, i32),
    Remove(usize, u8),
    RemoveAll(usize),
    Add(usize, usize, Option<usize>),
    RemoveChild(usize, usize),
    RemoveChildren(usize),
    Move(usize, usize, usize),
}

fn op() -> impl Strategy<Value = Op> {
    let n = || 0..6usize;
    prop_oneof![
        (n(), 0..3u8, -2..3i32).prop_map(|(h, p, v)| Op::Set(h, p, v)),
        (n(), 0..3u8).prop_map(|(h, p)| Op::Remove(h, p)),
        n().prop_map(Op::RemoveAll),
        (n(), n(), proptest::option::of(0..4usize)).prop_map(|(p, c, at)| Op::Add(p, c, at)),
        (n(), 0..4usize).prop_map(|(h, at)| Op::RemoveChild(h, at)),
        n().prop_map(Op::RemoveChildren),
        (n(), 0..4usize, 0..5usize).prop_map(|(h, f, t)| Op::Move(h, f, t)),
    ]
}

/// Applies one op through the `*_with_undo` methods; errors and no-ops are allowed.
fn apply(store: &Store, mt: &MessageThread<'_>, u: &mut UndoManager, pool: &[NodeRef], op: Op) {
    let prop = |p: u8| id(&format!("p{p}"));
    let _ = match op {
        Op::Set(h, p, v) => store.set_property_with_undo(mt, u, &pool[h], prop(p), v),
        Op::Remove(h, p) => store
            .remove_property_with_undo(mt, u, &pool[h], prop(p))
            .map(drop),
        Op::RemoveAll(h) => store.remove_all_properties_with_undo(mt, u, &pool[h]),
        Op::Add(p, c, at) => store.add_child_with_undo(mt, u, &pool[p], &pool[c], at),
        Op::RemoveChild(h, at) => match store.child(&pool[h], at).unwrap() {
            Some(c) => store.remove_child_with_undo(mt, u, &pool[h], &c),
            None => Ok(()),
        },
        Op::RemoveChildren(h) => store.remove_all_children_with_undo(mt, u, &pool[h]),
        Op::Move(h, from, to) => store.move_child_with_undo(mt, u, &pool[h], from, to),
    };
}

proptest! {
    #[test]
    fn b6_undo_all_restores_the_start_and_redo_all_the_end(ops in proptest::collection::vec(op(), 1..40)) {
        ManualLoop::new().with_token(|mt| {
            let store = Store::new();
            let mut undo = UndoManager::new();
            let pool: Vec<NodeRef> = (0..6).map(|i| store.create(mt, id(&format!("T{}", i % 3)))).collect();
            let all: Vec<&NodeRef> = pool.iter().collect();
            let start = snapshot(&store, mt, &all);
            for (i, op) in ops.into_iter().enumerate() {
                undo.begin_new_transaction(format!("step {i}"));
                apply(&store, mt, &mut undo, &pool, op);
            }
            let end = snapshot(&store, mt, &all);
            while undo.can_undo() {
                assert!(undo.undo(mt).unwrap());
            }
            assert!(same(&store, &all, &start), "undo all returns to the start");
            while undo.can_redo() {
                assert!(undo.redo(mt).unwrap());
            }
            assert!(same(&store, &all, &end), "redo all returns to the end");
        });
    }
}

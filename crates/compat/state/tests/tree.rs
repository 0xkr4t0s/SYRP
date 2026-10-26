//! B1, B4 and B5: `ValueTree` as a handle over the `syrp-state` store.

use std::rc::Rc;

use syrp_compat_state::ValueTree;
use syrp_loop_api::{ManualLoop, MessageThread};
use syrp_state::{StateErrorKind, Store, UndoManager};
use syrp_var::Var;

fn with<R>(f: impl FnOnce(&MessageThread<'_>, &Rc<Store>) -> R) -> R {
    ManualLoop::new().with_token(|mt| f(mt, &Rc::new(Store::new())))
}

#[test]
fn b1_clone_is_shallow_and_equality_is_identity() {
    with(|mt, s| {
        let a = ValueTree::new_in(mt, s, "A");
        let b = a.clone();
        b.set_property(mt, "x", 1, None).unwrap();
        assert_eq!(a.property("x"), Var::from(1));
        assert_eq!(a, b);
        assert_eq!(a.reference_count(), 2);
        let twin = ValueTree::new_in(mt, s, "A");
        twin.set_property(mt, "x", 1, None).unwrap();
        assert_ne!(a, twin);
        assert!(a.is_equivalent_to(&twin));
    });
}

#[test]
fn b1_deep_clone_copies_subtree_detached() {
    with(|mt, s| {
        let a = ValueTree::new_in(mt, s, "A");
        let p = ValueTree::new_in(mt, s, "P");
        p.append_child(mt, &a, None).unwrap();
        let c = ValueTree::new_in(mt, s, "C");
        c.set_property(mt, "v", "deep", None).unwrap();
        a.append_child(mt, &c, None).unwrap();
        let copy = a.deep_clone(mt);
        assert!(copy.is_equivalent_to(&a) && copy != a);
        assert!(!copy.parent().is_valid());
        assert_ne!(copy.child(0), c);
        copy.child(0)
            .set_property(mt, "v", "changed", None)
            .unwrap();
        assert_eq!(c.property("v").to_text(), "deep");
        assert!(!copy.is_equivalent_to(&a));
    });
}

#[test]
fn b1_detached_subtree_lives_while_referenced() {
    with(|mt, s| {
        let root = ValueTree::new_in(mt, s, "Root");
        let mid = ValueTree::new_in(mt, s, "Mid");
        root.append_child(mt, &mid, None).unwrap();
        mid.append_child(mt, &ValueTree::new_in(mt, s, "Leaf"), None)
            .unwrap();
        root.remove_child(mt, &mid, None).unwrap();
        assert_eq!(s.live_nodes(), 3);
        assert!(mid.child(0).has_type("Leaf"));
        drop(root);
        assert_eq!(s.live_nodes(), 2);
        drop(mid);
        assert_eq!(s.live_nodes(), 0);
    });
}

#[test]
fn b1_invalid_tree_reads_empty_and_mutations_are_noops() {
    with(|mt, s| {
        let t = ValueTree::default();
        assert!(!t.is_valid() && t.property("x").is_void() && t.num_children() == 0);
        assert_eq!(t.node_type().as_str(), "");
        assert!(!t.parent().is_valid() && !t.root().is_valid() && !t.deep_clone(mt).is_valid());
        t.set_property(mt, "x", 1, None).unwrap();
        t.move_child(mt, 0, 1, None).unwrap();
        let a = ValueTree::new_in(mt, s, "A");
        a.append_child(mt, &t, None).unwrap();
        assert_eq!(a.num_children(), 0);
        assert!(t.is_equivalent_to(&ValueTree::default()) && !t.is_equivalent_to(&a));
        assert_eq!(t.handle(), None);
        assert_eq!(t, ValueTree::default());
    });
}

#[test]
fn b1_properties_mirror_juce_reads() {
    with(|mt, s| {
        let t = ValueTree::new_in(mt, s, "T");
        t.set_property(mt, "a", 1.5, None).unwrap();
        t.set_property(mt, "b", true, None).unwrap();
        assert_eq!(t.num_properties(), 2);
        let names: Vec<_> = (0..3).map(|i| t.property_name(i)).collect();
        assert!(names[0].is_some() && names[1].is_some() && names[2].is_none());
        assert_eq!(t.property_or("missing", Var::from("d")).to_text(), "d");
        assert_eq!(t.property_or("a", Var::from(0)).as_f64(), 1.5);
        t.remove_property(mt, "a", None).unwrap();
        assert!(!t.has_property("a"));
        t.remove_all_properties(mt, None).unwrap();
        assert_eq!(t.num_properties(), 0);
    });
}

#[test]
fn b1_children_navigation() {
    with(|mt, s| {
        let p = ValueTree::new_in(mt, s, "P");
        let kids: Vec<_> = ["A", "B", "C"]
            .iter()
            .map(|n| ValueTree::new_in(mt, s, *n))
            .collect();
        for k in &kids {
            p.append_child(mt, k, None).unwrap();
        }
        assert_eq!(p.children().collect::<Vec<_>>(), kids);
        assert_eq!(p.child_with_name("B"), kids[1]);
        assert!(!p.child_with_name("Z").is_valid());
        assert_eq!(kids[1].sibling(1), kids[2]);
        assert_eq!(kids[1].sibling(-1), kids[0]);
        assert!(!kids[2].sibling(1).is_valid() && !kids[0].sibling(-1).is_valid());
        assert!(!p.sibling(1).is_valid());
        p.move_child(mt, 2, 0, None).unwrap();
        assert_eq!(p.index_of(&kids[2]), Some(0));
        p.remove_child_at(mt, 0, None).unwrap();
        assert_eq!(p.num_children(), 2);
        p.remove_child_at(mt, 9, None).unwrap();
        let z = p.get_or_create_child_with_name(mt, "Z", None).unwrap();
        assert_eq!(z.parent(), p);
        assert_eq!(p.get_or_create_child_with_name(mt, "Z", None).unwrap(), z);
        p.remove_all_children(mt, None).unwrap();
        assert_eq!(p.children().count(), 0);
    });
}

#[test]
fn b1_ancestry_and_root() {
    with(|mt, s| {
        let (a, b, c) = (
            ValueTree::new_in(mt, s, "A"),
            ValueTree::new_in(mt, s, "B"),
            ValueTree::new_in(mt, s, "C"),
        );
        a.append_child(mt, &b, None).unwrap();
        b.append_child(mt, &c, None).unwrap();
        assert_eq!(c.root(), a);
        assert_eq!(a.root(), a);
        assert!(c.is_a_child_of(&a) && c.is_a_child_of(&b));
        assert!(!a.is_a_child_of(&c) && !a.is_a_child_of(&a));
        assert!(!a.is_a_child_of(&ValueTree::default()));
    });
}

#[test]
fn b1_store_errors_are_returned_not_panics() {
    with(|mt, s| {
        let (a, b) = (ValueTree::new_in(mt, s, "A"), ValueTree::new_in(mt, s, "B"));
        a.append_child(mt, &b, None).unwrap();
        let other = ValueTree::new_in(mt, s, "O");
        assert!(other.append_child(mt, &b, None).is_err()); // already has a parent
        assert!(b.append_child(mt, &a, None).is_err()); // cycle
        assert!(a.remove_child(mt, &other, None).is_err()); // not a child
        let e = a.move_child(mt, 5, 0, None).unwrap_err();
        assert_eq!(e.kind(), StateErrorKind::IndexOutOfRange);
        let foreign = ValueTree::new_in(mt, &Rc::new(Store::new()), "F");
        assert!(a.append_child(mt, &foreign, None).is_err());
        assert!(!a.is_equivalent_to(&foreign)); // different types
        assert_eq!(a.num_children(), 1);
    });
}

#[test]
fn b1_undo_manager_argument_records_every_mutation() {
    with(|mt, s| {
        let mut um = UndoManager::new();
        let p = ValueTree::new_in(mt, s, "P");
        let c = ValueTree::new_in(mt, s, "C");
        p.set_property(mt, "x", 1, Some(&mut um)).unwrap();
        p.append_child(mt, &c, Some(&mut um)).unwrap();
        p.append_child(mt, &ValueTree::new_in(mt, s, "D"), Some(&mut um))
            .unwrap();
        p.move_child(mt, 0, 1, Some(&mut um)).unwrap();
        p.remove_child(mt, &c, Some(&mut um)).unwrap();
        p.remove_property(mt, "x", Some(&mut um)).unwrap();
        p.remove_all_children(mt, Some(&mut um)).unwrap();
        p.remove_all_properties(mt, Some(&mut um)).unwrap();
        assert_eq!(p.num_children(), 0);
        while um.undo(mt).unwrap() {}
        assert_eq!(p.num_children(), 0);
        assert_eq!(p.num_properties(), 0);
        while um.redo(mt).unwrap() {}
        um.begin_new_transaction("edit");
        p.set_property(mt, "y", 2, Some(&mut um)).unwrap();
        um.begin_new_transaction("edit");
        p.set_property(mt, "y", 3, Some(&mut um)).unwrap();
        um.undo(mt).unwrap();
        assert_eq!(p.property("y"), Var::from(2));
    });
}

#[test]
fn b1_new_uses_one_store_per_thread() {
    ManualLoop::new().with_token(|mt| {
        let (a, b) = (ValueTree::new(mt, "A"), ValueTree::new(mt, "B"));
        assert!(Rc::ptr_eq(a.store().unwrap(), b.store().unwrap()));
        a.append_child(mt, &b, None).unwrap();
        assert_eq!(b.parent(), a);
    });
}

#[test]
fn b4_handle_is_the_native_key() {
    with(|mt, s| {
        let t = ValueTree::new_in(mt, s, "T");
        let key = t.handle().unwrap();
        let native = s.upgrade(key).unwrap();
        assert_eq!(Some(&native), t.node_ref());
        s.set_property(mt, &native, "n".into(), 4).unwrap();
        assert_eq!(t.property("n"), Var::from(4));
        assert_eq!(ValueTree::from_handle(s, key), t);
        drop((t, native));
        assert!(!ValueTree::from_handle(s, key).is_valid());
    });
}

#[test]
fn b5_dependency_direction_is_compat_to_state_only() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../core/state/Cargo.toml");
    let manifest = std::fs::read_to_string(root).unwrap();
    assert!(
        !manifest.contains("syrp-compat"),
        "syrp-state must not depend on compat"
    );
    let src = concat!(env!("CARGO_MANIFEST_DIR"), "/../../core/state/src");
    for entry in std::fs::read_dir(src).unwrap() {
        let text = std::fs::read_to_string(entry.unwrap().path()).unwrap();
        assert!(
            !text.contains("syrp_compat_state::"),
            "syrp-state names a compat item"
        );
    }
    let own = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).unwrap();
    assert!(own.contains("syrp-state"));
}

/// Builds `Root{props} -> [Voice{i: 0}, Voice{i: 1} -> [Osc{wave}]]` in `store`.
fn build(mt: &MessageThread<'_>, s: &Rc<Store>, props: &[(&str, Var)], wave: &str) -> ValueTree {
    let root = ValueTree::new_in(mt, s, "Root");
    for (k, v) in props {
        root.set_property(mt, *k, v.clone(), None).unwrap();
    }
    for i in 0..2 {
        root.append_child(mt, &ValueTree::new_in(mt, s, "Voice"), None)
            .unwrap();
        root.child(i)
            .set_property(mt, "i", i32::try_from(i).unwrap(), None)
            .unwrap();
    }
    root.child(1)
        .append_child(mt, &ValueTree::new_in(mt, s, "Osc"), None)
        .unwrap();
    root.child(1)
        .child(0)
        .set_property(mt, "wave", wave, None)
        .unwrap();
    root
}

#[test]
fn b1_is_equivalent_to_compares_content_across_stores() {
    ManualLoop::new().with_token(|mt| {
        let (s1, s2) = (Rc::new(Store::new()), Rc::new(Store::new()));
        // s2 interns the identifiers in another order, so its property order differs.
        ValueTree::new_in(mt, &s2, "Root")
            .set_property(mt, "n", 0, None)
            .unwrap();
        let p = |n: Var| [("name", Var::from("lead")), ("n", n)];
        let ab = p(2.into());
        let a = build(mt, &s1, &ab, "saw");
        let b = build(mt, &s2, &[ab[1].clone(), ab[0].clone()], "saw");
        assert!(a.is_equivalent_to(&b) && b.is_equivalent_to(&a));
        assert!(a.is_equivalent_to(&a.deep_clone(mt)));
        assert!(
            !a.is_equivalent_to(&build(mt, &s2, &ab, "sine")),
            "grandchild differs"
        );
        assert!(!a.is_equivalent_to(&build(mt, &s2, &p(3.into()), "saw")));
        assert!(
            !a.is_equivalent_to(&build(mt, &s2, &p("2".into()), "saw")),
            "\"2\" is not 2"
        );
        let fewer = build(mt, &s2, &ab[..1], "saw");
        assert!(!a.is_equivalent_to(&fewer) && !fewer.is_equivalent_to(&a));
        let reordered = build(mt, &s2, &ab, "saw");
        reordered.move_child(mt, 0, 1, None).unwrap();
        assert!(!a.is_equivalent_to(&reordered), "children order matters");
        assert!(!a.is_equivalent_to(&ValueTree::default()));
    });
}

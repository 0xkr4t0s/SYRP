//! B2 (listeners and redirection) and B6 (the WP-0.2-15 spike scenarios through the compat API).
//!
//! JUCE's documentation does not give listener order (spike open questions 2, 4 and 5), so
//! scenarios that the documentation decides compare sets of calls; the `b2_*_syrp_order` test
//! pins the store's own order and is not a parity claim.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use syrp_compat_state::{Listener, ValueTree};
use syrp_loop_api::{ManualLoop, MessageThread};
use syrp_state::Store;
use syrp_var::Identifier;

type Calls = Rc<RefCell<Vec<String>>>;

/// Records every callback as `"<name> <callback> <args>"`, naming trees by their type.
struct Rec {
    name: &'static str,
    calls: Calls,
}

impl Rec {
    fn new(name: &'static str, calls: &Calls) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self {
            name,
            calls: calls.clone(),
        }))
    }
    fn push(&self, what: String) {
        self.calls
            .borrow_mut()
            .push(format!("{} {what}", self.name));
    }
}

thread_local! {
    static KEEP: RefCell<Vec<Rc<dyn std::any::Any>>> = RefCell::default();
}

/// Keeps a listener alive for the rest of the test thread (wrappers hold listeners weakly).
fn kept<T: 'static>(l: Rc<T>) -> Rc<T> {
    KEEP.with(|k| k.borrow_mut().push(l.clone()));
    l
}

fn ty(t: &ValueTree) -> String {
    t.node_type().as_str().to_owned()
}

impl Listener for Rec {
    fn property_changed(&mut self, _: &MessageThread<'_>, t: &ValueTree, id: Identifier) {
        self.push(format!("prop {}.{}", ty(t), id.as_str()));
    }
    fn child_added(&mut self, _: &MessageThread<'_>, p: &ValueTree, c: &ValueTree) {
        self.push(format!("added {}+{}", ty(p), ty(c)));
    }
    fn child_removed(&mut self, _: &MessageThread<'_>, p: &ValueTree, c: &ValueTree, i: usize) {
        self.push(format!("removed {}-{}@{i}", ty(p), ty(c)));
    }
    fn child_order_changed(&mut self, _: &MessageThread<'_>, p: &ValueTree, a: usize, b: usize) {
        self.push(format!("moved {} {a}->{b}", ty(p)));
    }
    fn parent_changed(&mut self, _: &MessageThread<'_>, t: &ValueTree) {
        self.push(format!("parent {}", ty(t)));
    }
    fn redirected(&mut self, _: &MessageThread<'_>, t: &ValueTree) {
        self.push(format!("redirected {}", ty(t)));
    }
}

fn with<R>(f: impl FnOnce(&MessageThread<'_>, &Rc<Store>, &Calls) -> R) -> R {
    let calls = Calls::default();
    ManualLoop::new().with_token(|mt| f(mt, &Rc::new(Store::new()), &calls))
}

fn take(calls: &Calls) -> Vec<String> {
    std::mem::take(&mut *calls.borrow_mut())
}

fn set(calls: &Calls) -> BTreeSet<String> {
    take(calls).into_iter().collect()
}

fn strs(v: &[&str]) -> BTreeSet<String> {
    v.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn b2_every_callback_fires_for_its_change() {
    with(|mt, s, calls| {
        let mut p = ValueTree::new_in(mt, s, "P");
        let l = Rec::new("l", calls);
        p.add_listener(mt, &l);
        let (a, b) = (ValueTree::new_in(mt, s, "A"), ValueTree::new_in(mt, s, "B"));
        p.set_property(mt, "x", 1, None).unwrap();
        p.remove_property(mt, "x", None).unwrap();
        p.append_child(mt, &a, None).unwrap();
        p.append_child(mt, &b, None).unwrap();
        p.move_child(mt, 0, 1, None).unwrap();
        p.remove_child(mt, &a, None).unwrap();
        let want = [
            "l prop P.x",
            "l prop P.x",
            "l added P+A",
            "l added P+B",
            "l moved P 0->1",
            "l removed P-A@1",
        ];
        assert_eq!(take(calls), want);
    });
}

#[test]
fn b2_parent_changed_only_reaches_the_tree_itself() {
    with(|mt, s, calls| {
        let mut root = ValueTree::new_in(mt, s, "Root");
        let mut c = ValueTree::new_in(mt, s, "C");
        let g = ValueTree::new_in(mt, s, "G");
        c.append_child(mt, &g, None).unwrap();
        root.add_listener(mt, &kept(Rec::new("root", calls)));
        let cl = Rec::new("c", calls);
        c.add_listener(mt, &cl);
        root.append_child(mt, &c, None).unwrap();
        let want = strs(&["root added Root+C", "c parent C"]);
        assert_eq!(set(calls), want);
    });
}

#[test]
fn b2_listeners_belong_to_the_wrapper_not_the_node() {
    with(|mt, s, calls| {
        let mut a = ValueTree::new_in(mt, s, "A");
        let l = Rec::new("l", calls);
        a.add_listener(mt, &l);
        a.add_listener(mt, &l); // a second add does nothing
        let clone = a.clone();
        clone.set_property(mt, "x", 1, None).unwrap();
        assert_eq!(take(calls), ["l prop A.x"]);
        drop(a);
        clone.set_property(mt, "x", 2, None).unwrap();
        assert!(take(calls).is_empty(), "listener died with its wrapper");
    });
}

#[test]
fn b2_remove_listener_and_dropped_listener_stop_calls() {
    with(|mt, s, calls| {
        let mut a = ValueTree::new_in(mt, s, "A");
        let (l, m) = (Rec::new("l", calls), Rec::new("m", calls));
        a.add_listener(mt, &l);
        a.add_listener(mt, &m);
        a.remove_listener(&l);
        a.set_property(mt, "x", 1, None).unwrap();
        assert_eq!(take(calls), ["m prop A.x"]);
        drop(m);
        a.set_property(mt, "x", 2, None).unwrap();
        assert!(take(calls).is_empty());
        assert_eq!(
            a.reference_count(),
            2,
            "a dead listener's subscription lingers until reuse"
        );
        a.add_listener(mt, &l);
        assert_eq!(a.reference_count(), 2);
    });
}

#[test]
fn b2_assign_moves_listeners_and_calls_redirected() {
    with(|mt, s, calls| {
        let mut w = ValueTree::new_in(mt, s, "A");
        let b = ValueTree::new_in(mt, s, "B");
        let mut b_owner = b.clone();
        b_owner.add_listener(mt, &kept(Rec::new("own", calls)));
        let l = Rec::new("l", calls);
        w.add_listener(mt, &l);
        let old = w.clone();
        w.assign(mt, &b);
        assert_eq!(take(calls), ["l redirected B"]);
        old.set_property(mt, "x", 1, None).unwrap();
        assert!(take(calls).is_empty(), "no longer on the old node");
        b.set_property(mt, "x", 1, None).unwrap();
        assert_eq!(set(calls), strs(&["l prop B.x", "own prop B.x"]));
        w.assign(mt, &b);
        assert!(take(calls).is_empty(), "same node: no redirect");
        w.assign(mt, &ValueTree::default());
        assert_eq!(take(calls), ["l redirected "]);
        w.assign(mt, &old);
        old.set_property(mt, "y", 1, None).unwrap();
        assert_eq!(take(calls), ["l redirected A", "l prop A.y"]);
    });
}

#[test]
fn b2_listener_on_invalid_tree_waits_for_assign() {
    with(|mt, s, calls| {
        let mut w = ValueTree::default();
        w.add_listener(mt, &kept(Rec::new("l", calls)));
        let a = ValueTree::new_in(mt, s, "A");
        w.assign(mt, &a);
        a.set_property(mt, "x", 1, None).unwrap();
        assert_eq!(take(calls), ["l redirected A", "l prop A.x"]);
    });
}

#[test]
fn b2_delivery_order_node_first_then_ancestors_syrp_order() {
    with(|mt, s, calls| {
        let mut p = ValueTree::new_in(mt, s, "P");
        let mut c = ValueTree::new_in(mt, s, "C");
        p.append_child(mt, &c, None).unwrap();
        p.add_listener(mt, &kept(Rec::new("p1", calls)));
        c.add_listener(mt, &kept(Rec::new("c1", calls)));
        c.add_listener(mt, &kept(Rec::new("c2", calls)));
        p.add_listener(mt, &kept(Rec::new("p2", calls)));
        c.set_property(mt, "x", 1, None).unwrap();
        let want = ["c1 prop C.x", "c2 prop C.x", "p1 prop C.x", "p2 prop C.x"];
        assert_eq!(take(calls), want);
        p.remove_child(mt, &c, None).unwrap();
        assert_eq!(
            take(calls),
            [
                "p1 removed P-C@0",
                "p2 removed P-C@0",
                "c1 parent C",
                "c2 parent C"
            ]
        );
    });
}

/// Mutates its tree once from inside a callback.
struct Echo(ValueTree, Calls);

impl Listener for Echo {
    fn property_changed(&mut self, mt: &MessageThread<'_>, t: &ValueTree, id: Identifier) {
        self.1.borrow_mut().push(format!("echo {}", id.as_str()));
        if id.as_str() == "x" {
            t.set_property(mt, "y", 1, None).unwrap();
        }
        assert!(self.0.has_property("x"));
    }
}

#[test]
fn b6_listener_mutation_is_delivered_after_current_record() {
    with(|mt, s, calls| {
        let mut a = ValueTree::new_in(mt, s, "A");
        let echo = Rc::new(RefCell::new(Echo(a.clone(), calls.clone())));
        a.add_listener(mt, &echo);
        a.add_listener(mt, &kept(Rec::new("l", calls)));
        a.set_property(mt, "x", 1, None).unwrap();
        assert_eq!(
            take(calls),
            ["echo x", "l prop A.x", "echo y", "l prop A.y"]
        );
    });
}

#[test]
fn b6_detached_subtree_survives_and_reattaches_elsewhere() {
    with(|mt, s, calls| {
        let mut old_p = ValueTree::new_in(mt, s, "Old");
        let mut new_p = ValueTree::new_in(mt, s, "New");
        let mut sub = ValueTree::new_in(mt, s, "Sub");
        let grand = ValueTree::new_in(mt, s, "Grand");
        sub.append_child(mt, &grand, None).unwrap();
        old_p.append_child(mt, &sub, None).unwrap();
        old_p.add_listener(mt, &kept(Rec::new("old", calls)));
        new_p.add_listener(mt, &kept(Rec::new("new", calls)));
        sub.add_listener(mt, &kept(Rec::new("sub", calls)));
        old_p.remove_child(mt, &sub, None).unwrap();
        new_p.append_child(mt, &sub, None).unwrap();
        grand.set_property(mt, "v", 1, None).unwrap();
        let want = strs(&[
            "old removed Old-Sub@0",
            "sub parent Sub",
            "new added New+Sub",
            "new prop Grand.v",
            "sub prop Grand.v",
        ]);
        let got = take(calls);
        assert_eq!(got.len(), 6, "{got:?}");
        assert_eq!(got.into_iter().collect::<BTreeSet<_>>(), want);
    });
}

#[test]
fn b6_ancestor_listeners_see_descendant_changes_not_vice_versa() {
    with(|mt, s, calls| {
        let mut root = ValueTree::new_in(mt, s, "Root");
        let mut mid = ValueTree::new_in(mt, s, "Mid");
        let leaf = ValueTree::new_in(mt, s, "Leaf");
        root.append_child(mt, &mid, None).unwrap();
        root.add_listener(mt, &kept(Rec::new("root", calls)));
        mid.add_listener(mt, &kept(Rec::new("mid", calls)));
        mid.append_child(mt, &leaf, None).unwrap();
        leaf.set_property(mt, "v", 1, None).unwrap();
        root.set_property(mt, "r", 1, None).unwrap();
        let want = strs(&[
            "root added Mid+Leaf",
            "mid added Mid+Leaf",
            "root prop Leaf.v",
            "mid prop Leaf.v",
            "root prop Root.r",
        ]);
        assert_eq!(set(calls), want);
    });
}

#[test]
fn b6_handles_are_shallow_and_refcounted() {
    with(|mt, s, _| {
        let p = ValueTree::new_in(mt, s, "P");
        let c = ValueTree::new_in(mt, s, "C");
        let c2 = c.clone();
        assert_eq!(c.reference_count(), 2);
        p.append_child(mt, &c, None).unwrap();
        assert_eq!(c.reference_count(), 3);
        p.remove_child(mt, &c, None).unwrap();
        assert_eq!(c2.reference_count(), 2);
    });
}

#[test]
fn b6_nodes_hold_type_properties_and_ordered_children() {
    with(|mt, s, _| {
        let p = ValueTree::new_in(mt, s, "P");
        for (i, n) in ["A", "B", "C"].into_iter().enumerate() {
            p.set_property(mt, n, i32::try_from(i).unwrap(), None)
                .unwrap();
        }
        let names: BTreeSet<_> = (0..3).filter_map(|i| p.property_name(i)).collect();
        assert_eq!(names.len(), 3, "any order, each once");
        let (a, b, c) = (
            ValueTree::new_in(mt, s, "A"),
            ValueTree::new_in(mt, s, "B"),
            ValueTree::new_in(mt, s, "C"),
        );
        p.append_child(mt, &a, None).unwrap();
        p.add_child(mt, &c, Some(99), None).unwrap(); // past the end appends
        p.add_child(mt, &b, Some(1), None).unwrap();
        assert_eq!(
            p.children().map(|t| ty(&t)).collect::<Vec<_>>(),
            ["A", "B", "C"]
        );
    });
}

#[test]
fn b6_add_child_rejects_parented_child_and_cycles() {
    with(|mt, s, _| {
        let (a, b, c) = (
            ValueTree::new_in(mt, s, "A"),
            ValueTree::new_in(mt, s, "B"),
            ValueTree::new_in(mt, s, "C"),
        );
        a.append_child(mt, &b, None).unwrap();
        b.append_child(mt, &c, None).unwrap();
        assert!(c.append_child(mt, &b, None).is_err() && c.append_child(mt, &a, None).is_err());
        assert!(a.append_child(mt, &c, None).is_err());
        assert!(a.append_child(mt, &a, None).is_err());
    });
}

#[test]
fn b6_reclaiming_an_orphan_frees_its_slots() {
    with(|mt, s, calls| {
        let mut root = ValueTree::new_in(mt, s, "Root");
        let orphan = ValueTree::new_in(mt, s, "O");
        for n in ["X", "Y", "Z"] {
            orphan
                .append_child(mt, &ValueTree::new_in(mt, s, n), None)
                .unwrap();
        }
        root.append_child(mt, &orphan, None).unwrap();
        root.add_listener(mt, &kept(Rec::new("root", calls)));
        root.remove_child(mt, &orphan, None).unwrap();
        assert_eq!(s.live_nodes(), 5);
        drop(orphan);
        assert_eq!(s.live_nodes(), 1);
        let unheld = ValueTree::new_in(mt, s, "U");
        root.append_child(mt, &unheld, None).unwrap();
        drop(unheld);
        root.remove_child_at(mt, 0, None).unwrap();
        assert_eq!(s.live_nodes(), 1, "freed once its removal was delivered");
        assert!(take(calls).contains(&"root removed Root-U@0".to_owned()));
    });
}

#[test]
fn b6_listener_lives_with_its_wrapper() {
    with(|mt, s, calls| {
        let a = ValueTree::new_in(mt, s, "A");
        let mut w = a.clone();
        w.add_listener(mt, &kept(Rec::new("w", calls)));
        a.set_property(mt, "x", 1, None).unwrap();
        drop(w);
        a.set_property(mt, "x", 2, None).unwrap();
        assert_eq!(take(calls), ["w prop A.x"]);
    });
}

//! B7: `deep_copy`, `is_equivalent`, and the invariant proptest (ADR 0003 D7).

use std::collections::{HashMap, HashSet};

use proptest::prelude::*;
use syrp_loop_api::{ManualLoop, MessageThread};
use syrp_state::{NodeKey, NodeRef, Store, Subscription};
use syrp_var::{Identifier, Var};

fn id(name: &str) -> Identifier {
    Identifier::new(name)
}

#[test]
fn b7_deep_copy_is_detached_and_equivalent() {
    ManualLoop::new().with_token(|mt| {
        let store = Store::new();
        let [root, a, b, g] = ["R", "A", "B", "G"].map(|t| store.create(mt, id(t)));
        let host = store.create(mt, id("Host"));
        store.add_child(mt, &host, &root, None).unwrap();
        for (p, c) in [(&root, &a), (&root, &b), (&a, &g)] {
            store.add_child(mt, p, c, None).unwrap();
        }
        store.set_property(mt, &g, id("x"), 1).unwrap();
        store.set_property(mt, &root, id("name"), "r").unwrap();
        let copy = store.deep_copy(mt, &root).unwrap();
        assert_ne!(copy, root);
        assert_eq!(store.parent(&copy).unwrap(), None);
        assert!(store.is_equivalent(&copy, &root).unwrap());
        assert_eq!(store.live_nodes(), 9);
        // The copy is independent of the original.
        let g_copy = store
            .child(&store.child(&copy, 0).unwrap().unwrap(), 0)
            .unwrap()
            .unwrap();
        assert_ne!(g_copy, g);
        store.set_property(mt, &g_copy, id("x"), 2).unwrap();
        assert!(!store.is_equivalent(&copy, &root).unwrap());
        assert_eq!(store.property(&g, id("x")).unwrap(), Some(Var::from(1)));
        drop((copy, g_copy));
        assert_eq!(store.live_nodes(), 5);
    });
}

#[test]
fn b7_equivalence_ignores_property_order_but_not_child_order() {
    ManualLoop::new().with_token(|mt| {
        let store = Store::new();
        let [p, q] = ["N", "N"].map(|t| store.create(mt, id(t)));
        let (x, y) = (id("b7_x"), id("b7_y"));
        store.set_property(mt, &p, x, 1).unwrap();
        store.set_property(mt, &p, y, 2).unwrap();
        store.set_property(mt, &q, y, 2).unwrap();
        store.set_property(mt, &q, x, 1).unwrap();
        assert!(store.is_equivalent(&p, &q).unwrap());
        for (n, order) in [(&p, ["A", "B"]), (&q, ["B", "A"])] {
            for t in order {
                store
                    .add_child(mt, n, &store.create(mt, id(t)), None)
                    .unwrap();
            }
        }
        assert!(!store.is_equivalent(&p, &q).unwrap());
        store.move_child(mt, &q, 0, 1).unwrap();
        assert!(store.is_equivalent(&p, &q).unwrap());
        let r = store.create(mt, id("Other"));
        assert!(!store.is_equivalent(&p, &r).unwrap());
    });
}

#[derive(Debug, Clone)]
enum Op {
    Create(u8),
    Set(usize, u8, i32),
    Remove(usize, u8),
    Add(usize, usize, Option<usize>),
    RemoveChild(usize, usize),
    Move(usize, usize, usize),
    TakeChild(usize, usize),
    DropHandle(usize),
    Subscribe(usize),
    Unsubscribe(usize),
}

fn op() -> impl Strategy<Value = Op> {
    let i = || 0..8usize;
    prop_oneof![
        (0..3u8).prop_map(Op::Create),
        (i(), 0..4u8, -2..3i32).prop_map(|(h, p, v)| Op::Set(h, p, v)),
        (i(), 0..4u8).prop_map(|(h, p)| Op::Remove(h, p)),
        (i(), i(), proptest::option::of(0..4usize)).prop_map(|(p, c, at)| Op::Add(p, c, at)),
        (i(), 0..4usize).prop_map(|(h, at)| Op::RemoveChild(h, at)),
        (i(), 0..4usize, 0..5usize).prop_map(|(h, f, t)| Op::Move(h, f, t)),
        (i(), 0..4usize).prop_map(|(h, at)| Op::TakeChild(h, at)),
        i().prop_map(Op::DropHandle),
        i().prop_map(Op::Subscribe),
        i().prop_map(Op::Unsubscribe),
    ]
}

/// Applies one op; indices past the end of `held` or `subs` wrap, and errors are allowed.
fn apply(
    store: &Store,
    mt: &MessageThread<'_>,
    held: &mut Vec<NodeRef>,
    subs: &mut Vec<(NodeKey, Subscription)>,
    op: Op,
) {
    let pick = |held: &[NodeRef], h: usize| held.get(h % held.len().max(1)).cloned();
    let prop = |p: u8| id(&format!("p{p}"));
    match op {
        Op::Create(t) => held.push(store.create(mt, id(&format!("T{t}")))),
        Op::Set(h, p, v) => drop(pick(held, h).map(|n| store.set_property(mt, &n, prop(p), v))),
        Op::Remove(h, p) => drop(pick(held, h).map(|n| store.remove_property(mt, &n, prop(p)))),
        Op::Add(p, c, at) => {
            if let (Some(p), Some(c)) = (pick(held, p), pick(held, c)) {
                let _ = store.add_child(mt, &p, &c, at);
            }
        }
        Op::RemoveChild(h, at) | Op::TakeChild(h, at) => {
            let Some(n) = pick(held, h) else { return };
            let Some(c) = store.child(&n, at).unwrap() else {
                return;
            };
            if matches!(op, Op::RemoveChild(..)) {
                store.remove_child(mt, &n, &c).unwrap();
            } else {
                held.push(c);
            }
        }
        Op::Move(h, from, to) => drop(pick(held, h).map(|n| store.move_child(mt, &n, from, to))),
        Op::DropHandle(h) if !held.is_empty() => drop(held.swap_remove(h % held.len())),
        Op::Subscribe(h) => {
            if let Some(n) = pick(held, h) {
                subs.push((n.key(), store.subscribe(mt, &n, |_, _, _| {}).unwrap()));
            }
        }
        Op::Unsubscribe(s) if !subs.is_empty() => drop(subs.swap_remove(s % subs.len())),
        Op::DropHandle(_) | Op::Unsubscribe(_) => {}
    }
}

/// Checks every invariant: links agree, no cycles, sorted properties, counts equal a census of
/// handles, subscriptions, parent links and pending records, and every live node is accounted for.
fn check(store: &Store, held: &[NodeRef], subs: &[(NodeKey, Subscription)]) {
    assert_eq!(
        store.log_len(),
        0,
        "no cursor and nothing pending once a call returns"
    );
    let mut census: HashMap<NodeKey, u32> = HashMap::new();
    for k in held
        .iter()
        .map(NodeRef::key)
        .chain(subs.iter().map(|(k, _)| *k))
    {
        *census.entry(k).or_default() += 1;
    }
    let mut seen = HashSet::new();
    let mut stack: Vec<NodeKey> = census.keys().copied().collect();
    while let Some(key) = stack.pop() {
        if !seen.insert(key) {
            continue;
        }
        let n = store.upgrade(key).unwrap();
        let parent = store.parent(&n).unwrap();
        let expected = census.get(&key).copied().unwrap_or(0) + u32::from(parent.is_some()) + 1;
        assert_eq!(
            store.reference_count(&n).unwrap(),
            expected,
            "count of {key:?}"
        );
        if let Some(p) = &parent {
            assert!(
                store.index_of(p, &n).unwrap().is_some(),
                "parent lists the child"
            );
        }
        let (mut up, mut steps) = (parent, 0);
        while let Some(p) = up {
            steps += 1;
            assert!(steps <= store.live_nodes(), "cycle above {key:?}");
            up = store.parent(&p).unwrap();
        }
        let keys: Vec<Identifier> = store.properties(&n).unwrap().map(|(k, _)| k).collect();
        assert!(
            keys.windows(2).all(|w| w[0] < w[1]),
            "properties sorted and unique"
        );
        for i in 0..store.num_children(&n).unwrap() {
            let c = store.child(&n, i).unwrap().unwrap();
            assert_eq!(
                store.parent(&c).unwrap().as_ref(),
                Some(&n),
                "child links back"
            );
            stack.push(c.key());
        }
    }
    assert_eq!(
        seen.len(),
        store.live_nodes(),
        "every live node is held, subscribed or attached"
    );
}

proptest! {
    #[test]
    fn b7_random_operations_keep_every_invariant(ops in proptest::collection::vec(op(), 1..60)) {
        ManualLoop::new().with_token(|mt| {
            let store = Store::new();
            let (mut held, mut subs) = (Vec::new(), Vec::new());
            for op in ops {
                apply(&store, mt, &mut held, &mut subs, op);
                check(&store, &held, &subs);
            }
            drop((held, subs));
            assert_eq!((store.live_nodes(), store.log_len()), (0, 0));
        });
    }
}

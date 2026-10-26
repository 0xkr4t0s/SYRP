//! Spec B5: property get/set on nodes with 4, 16 and 64 properties, wall clock, compared with
//! the ADR 0003 budget of about 5-20 ns per property access. `tests/scenarios.rs` mounts this
//! file to smoke-test `measure`.

use std::hint::black_box;
use std::time::Instant;

use syrp_state_arena_spike::{Id, Store, Var};

/// Nanoseconds per operation: get, the same get on a bare sorted `Vec` (no `RefCell`, no
/// arena), set with no listeners, and set with one listener on the node and one on its parent.
pub fn measure(props: usize, iters: u32) -> [f64; 4] {
    let store = Store::new();
    let parent = store.create(store.id("Parent"));
    let node = store.create(store.id("Node"));
    parent.add_child(&node, None);
    let ids: Vec<Id> = (0..props).map(|i| store.id(&format!("p{i}"))).collect();
    for id in &ids {
        node.set_property(*id, Var::Double(1.0));
    }
    let bare: Vec<(Id, Var)> = ids.iter().map(|id| (*id, Var::Double(1.0))).collect();
    // Keys are cycled with a counter, not `i % props`: a 64-bit divide would dominate.
    let time = |op: &mut dyn FnMut(u32, Id)| {
        let (start, mut k) = (Instant::now(), 0);
        for i in 0..iters {
            op(i, ids[k]);
            k = if k + 1 == props { 0 } else { k + 1 };
            if i % 1024 == 1023 {
                store.compact_log();
            }
        }
        start.elapsed().as_nanos() as f64 / f64::from(iters)
    };
    let get = time(&mut |_, id| drop(black_box(node.property(black_box(id)))));
    let bare_get = time(&mut |_, id| {
        let found = bare.binary_search_by_key(&black_box(id), |(k, _)| *k);
        drop(black_box(found.ok().map(|i| bare[i].1.clone())));
    });
    let set = time(&mut |i, id| node.set_property(id, Var::Double(f64::from(i))));
    let _a = node.subscribe(|ev| {
        black_box(ev);
    });
    let _b = parent.subscribe(|ev| {
        black_box(ev);
    });
    let set_listened = time(&mut |i, id| node.set_property(id, Var::Double(f64::from(i) + 0.5)));
    [get, bare_get, set, set_listened]
}

#[allow(
    dead_code,
    reason = "the smoke test includes this file and only calls `measure`"
)]
fn main() {
    println!("props   get ns   bare-vec get ns   set ns   set+2 listeners ns");
    for props in [4, 16, 64] {
        // Best of five runs, to drop scheduler noise.
        let mut best = [f64::INFINITY; 4];
        for _ in 0..5 {
            for (b, t) in best.iter_mut().zip(measure(props, 2_000_000)) {
                *b = b.min(t);
            }
        }
        let [g, bg, s, sl] = best;
        println!("{props:>5} {g:>8.1} {bg:>17.1} {s:>8.1} {sl:>20.1}");
    }
}

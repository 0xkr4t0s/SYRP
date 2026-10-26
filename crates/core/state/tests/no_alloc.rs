//! B6: `set_property` with `i32` or `f64` values allocates nothing after warm-up, with and
//! without listeners. A separate test binary, because it installs the guarded allocator.

use std::cell::Cell;
use std::rc::Rc;

use syrp_loop_api::ManualLoop;
use syrp_state::Store;
use syrp_var::{Identifier, Var};

#[global_allocator]
static ALLOC: syrp_rt::GuardedAllocator = syrp_rt::GuardedAllocator;

#[test]
fn b6_set_property_allocates_nothing_after_warm_up() {
    ManualLoop::new().with_token(|mt| {
        let store = Store::new();
        let parent = store.create(mt, Identifier::new("P"));
        let node = store.create(mt, Identifier::new("N"));
        store.add_child(mt, &parent, &node, None).unwrap();
        let ids: Vec<Identifier> = (0..16)
            .map(|i| Identifier::new(&format!("b6_{i}")))
            .collect();
        let run = |n: i32| {
            for (i, id) in ids.iter().enumerate() {
                let v = if i % 2 == 0 {
                    Var::from(n)
                } else {
                    Var::from(f64::from(n))
                };
                store.set_property(mt, &node, *id, v).unwrap();
            }
        };
        run(0); // warm-up: properties exist, the log's buffer has grown
        syrp_rt::no_alloc(|| (1..50).for_each(run));

        let heard = Rc::new(Cell::new(0_u32));
        let listen = |n: &syrp_state::NodeRef| {
            let h = Rc::clone(&heard);
            store
                .subscribe(mt, n, move |_, _, _| h.set(h.get() + 1))
                .unwrap()
        };
        let _subs = (listen(&node), listen(&parent));
        run(50); // warm-up for the delivery path: the snapshot buffer has grown
        syrp_rt::no_alloc(|| (51..100).for_each(run));
        assert_eq!(heard.get(), 2 * 16 * 50);
    });
}

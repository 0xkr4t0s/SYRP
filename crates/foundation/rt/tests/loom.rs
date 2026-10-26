// SPDX-License-Identifier: MIT OR Apache-2.0
//! loom model of the lock-free SPSC queue (B3, B7). Run with:
//! `RUSTFLAGS="--cfg loom" cargo test -p syrp-rt --release --test loom`
#![cfg(loom)]

use loom::thread;
use syrp_rt::spsc;

#[test]
fn b3_loom_spsc_delivers_every_value_once_in_order_across_wraparound() {
    loom::model(|| {
        let (mut tx, mut rx) = spsc::channel::<usize>(2);
        let producer = thread::spawn(move || {
            for i in 0..3 {
                let mut v = i;
                while let Err(back) = tx.try_push(v) {
                    v = back;
                    thread::yield_now();
                }
            }
        });
        let mut next = 0;
        while next < 3 {
            match rx.try_pop() {
                Some(v) => {
                    assert_eq!(v, next);
                    next += 1;
                }
                None => thread::yield_now(),
            }
        }
        producer.join().unwrap();
        assert_eq!(rx.try_pop(), None);
    });
}

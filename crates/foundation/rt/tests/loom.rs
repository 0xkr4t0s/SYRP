// SPDX-License-Identifier: MIT OR Apache-2.0
//! loom models of the lock-free code: the SPSC queue (B3) and RtHandoff (B5); see B7. Run with:
//! `RUSTFLAGS="--cfg loom" cargo test -p syrp-rt --release --test loom`
#![cfg(loom)]

use loom::cell::UnsafeCell;
use loom::thread;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use syrp_rt::{Collector, RtHandoff, spsc};

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

/// Counts its own drops so the model can check nothing leaks or is dropped twice. The value
/// sits in a loom cell so loom flags a read that is not ordered after the publishing write.
struct Counted(Arc<AtomicUsize>, UnsafeCell<usize>);
impl Counted {
    fn new(drops: &Arc<AtomicUsize>, value: usize) -> Box<Self> {
        Box::new(Self(Arc::clone(drops), UnsafeCell::new(value)))
    }
    fn get(&self) -> usize {
        // SAFETY: the cell is written only by `new`; this is a plain read.
        self.1.with(|p| unsafe { *p })
    }
}
impl Drop for Counted {
    fn drop(&mut self) {
        self.0.fetch_add(1, SeqCst);
    }
}

#[test]
fn b5_loom_handoff_reader_sees_published_values_and_reclaims_each_once() {
    loom::model(|| {
        let drops = Arc::new(AtomicUsize::new(0));
        let mut collector = Collector::new();
        let (writer, mut reader) = RtHandoff::new(Counted::new(&drops, 0), collector.handle(2));
        let d = Arc::clone(&drops);
        let publisher = thread::spawn(move || {
            writer.publish(Counted::new(&d, 1));
            writer.publish(Counted::new(&d, 2));
            writer
        });
        let a = reader.latest().get();
        let b = reader.latest().get();
        assert!(a <= b, "values never go backwards: {a} then {b}");
        let writer = publisher.join().unwrap();
        assert_eq!(reader.latest().get(), 2, "the last published value wins");
        // Values 0 and 1 are gone: deferred by the reader or dropped by the writer.
        collector.collect();
        assert_eq!(drops.load(SeqCst), 2);
        drop((writer, reader));
        collector.collect();
        assert_eq!(drops.load(SeqCst), 3);
    });
}

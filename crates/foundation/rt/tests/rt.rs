// SPDX-License-Identifier: MIT OR Apache-2.0
//! Behaviour tests for syrp-rt (WP-0.1-04). Runs under Miri too (`cargo +nightly miri test`).
#![cfg(not(loom))]

use std::sync::atomic::Ordering::{AcqRel, Acquire, Relaxed, Release};
use std::sync::{Arc, Mutex};
use std::thread::{self, ThreadId};
use syrp_rt::spsc::{Consumer, Producer};
use syrp_rt::*;

#[global_allocator]
static ALLOC: GuardedAllocator = GuardedAllocator;

/// Fewer iterations under Miri, which is slow.
const N: usize = if cfg!(miri) { 200 } else { 100_000 };

fn rt_safe<T: RtSafe>() {}

#[test]
fn b1_rt_safe_covers_primitives_arrays_tuples_and_crate_types() {
    rt_safe::<u8>();
    rt_safe::<i128>();
    rt_safe::<f32>();
    rt_safe::<f64>();
    rt_safe::<bool>();
    rt_safe::<char>();
    rt_safe::<()>();
    rt_safe::<[f32; 8]>();
    rt_safe::<[[i16; 2]; 3]>();
    rt_safe::<(u32,)>();
    rt_safe::<(f32, [u8; 4], (bool, char), i64, u16, usize, isize, f64)>();
    rt_safe::<AtomicF32>();
    rt_safe::<AtomicF64>();
    rt_safe::<Producer<f32>>();
    rt_safe::<Consumer<(u8, f64)>>();
    rt_safe::<DeferredDrop>();
    rt_safe::<HandoffReader<Vec<f32>>>();
}

#[test]
fn b2_atomic_floats_round_trip_bits_and_support_every_operation() {
    let a = AtomicF32::new(1.0);
    no_alloc(|| {
        a.store(-0.0, Release);
        assert!(a.load(Acquire).is_sign_negative());
        assert_eq!(
            a.swap(f32::INFINITY, AcqRel).to_bits(),
            (-0.0_f32).to_bits()
        );
        let nan = f32::from_bits(0x7fc0_1234);
        a.store(nan, Relaxed);
        assert_eq!(a.load(Relaxed).to_bits(), 0x7fc0_1234);
        assert_eq!(
            a.fetch_update(AcqRel, Acquire, |_| Some(2.0))
                .map(f32::to_bits),
            Ok(nan.to_bits())
        );
        assert_eq!(a.fetch_update(AcqRel, Acquire, |_| None), Err(2.0));
    });
    assert_eq!(a.into_inner(), 2.0);

    let b = AtomicF64::new(0.1);
    assert_eq!(b.swap(0.2, AcqRel), 0.1);
    assert_eq!(b.fetch_update(AcqRel, Acquire, |x| Some(x * 2.0)), Ok(0.2));
    assert_eq!(b.load(Acquire), 0.4);
    assert_eq!(format!("{b:?}"), "0.4");
    assert_eq!(AtomicF64::default().into_inner(), 0.0);
}

#[test]
fn b3_try_push_fails_with_value_when_full_and_try_pop_is_none_when_empty() {
    let (mut tx, mut rx) = spsc::channel::<String>(2);
    let (a, b, c) = ("a".to_owned(), "b".to_owned(), "c".to_owned());
    let first = no_alloc(|| {
        assert_eq!(rx.try_pop(), None);
        assert!(rx.is_empty() && !tx.is_full());
        assert_eq!(tx.try_push(a), Ok(()));
        assert_eq!(tx.try_push(b), Ok(()));
        assert!(tx.is_full());
        let c = tx.try_push(c).unwrap_err();
        assert_eq!(c, "c");
        let first = rx.try_pop().unwrap();
        assert_eq!(tx.try_push(c), Ok(())); // wraps into the freed slot
        first
    });
    assert_eq!(first, "a");
    assert_eq!(rx.try_pop().as_deref(), Some("b"));
    assert_eq!(rx.try_pop().as_deref(), Some("c"));
    assert_eq!(rx.try_pop(), None);

    let (mut tx, mut rx) = spsc::channel(0); // always full and always empty
    assert_eq!(tx.try_push(1), Err(1));
    assert!(tx.is_full() && rx.is_empty() && rx.try_pop().is_none());
}

#[test]
fn b3_values_cross_threads_in_order_and_leftovers_are_dropped() {
    let (mut tx, mut rx) = spsc::channel::<usize>(7);
    let producer = thread::spawn(move || {
        for i in 0..N {
            let mut v = i;
            while let Err(back) = tx.try_push(v) {
                v = back;
                thread::yield_now();
            }
        }
    });
    let mut expected = 0;
    while expected < N {
        match rx.try_pop() {
            Some(v) => {
                assert_eq!(v, expected);
                expected += 1;
            }
            None => thread::yield_now(),
        }
    }
    producer.join().unwrap();

    let marker = Arc::new(());
    let (mut tx, rx) = spsc::channel(4);
    for _ in 0..3 {
        tx.try_push(Arc::clone(&marker)).unwrap();
    }
    drop(rx);
    drop(tx); // the last end frees the three queued clones
    assert_eq!(Arc::strong_count(&marker), 1);
}

/// Records the thread its destructor ran on.
struct DropSpy(Arc<Mutex<Vec<ThreadId>>>);
impl Drop for DropSpy {
    fn drop(&mut self) {
        self.0.lock().unwrap().push(thread::current().id());
    }
}

#[test]
fn b4_deferred_values_are_dropped_by_collect_on_the_collector_thread() {
    let drops = Arc::new(Mutex::new(Vec::new()));
    let mut collector = Collector::new();
    let mut defer = collector.handle(2);
    let spies = (
        Box::new(DropSpy(Arc::clone(&drops))),
        Box::new(DropSpy(Arc::clone(&drops))),
    );
    let extra = Box::new(3_u8);
    let audio = thread::spawn(move || {
        let back = no_alloc(|| {
            assert!(defer.defer(spies.0).is_ok());
            assert!(defer.defer(spies.1).is_ok());
            assert!(defer.is_full());
            defer.defer(extra).unwrap_err() // given back, not dropped here
        });
        assert_eq!(*back, 3);
        defer
    });
    let defer = audio.join().unwrap();
    assert!(drops.lock().unwrap().is_empty());
    assert_eq!(collector.collect(), 2);
    let me = thread::current().id();
    assert_eq!(*drops.lock().unwrap(), vec![me, me]);
    assert!(!defer.is_full());
    assert_eq!(collector.collect(), 0);
}

#[test]
fn b4_dropped_handles_are_reclaimed_by_collect_after_their_values() {
    let marker = Arc::new(());
    let mut collector = Collector::new();
    let mut kept = collector.handle(1);
    for _ in 0..3 {
        let mut d = collector.handle(2);
        d.defer(Box::new(Arc::clone(&marker))).unwrap();
        drop(d); // deferred values must survive the handle
    }
    assert_eq!(format!("{collector:?}"), "Collector { handles: 4 }");
    assert_eq!(Arc::strong_count(&marker), 4);
    assert_eq!(collector.collect(), 3);
    assert_eq!(Arc::strong_count(&marker), 1);
    assert_eq!(format!("{collector:?}"), "Collector { handles: 1 }");
    kept.defer(Box::new(Arc::clone(&marker))).unwrap();
    assert_eq!(collector.collect(), 1);
    assert_eq!(format!("{collector:?}"), "Collector { handles: 1 }");
}

#[test]
fn b5_reader_switches_to_latest_without_allocating_and_old_values_are_collected() {
    let drops = Arc::new(Mutex::new(Vec::new()));
    let spy = || Box::new(DropSpy(Arc::clone(&drops)));
    let mut collector = Collector::new();
    let (writer, mut reader) = RtHandoff::new(spy(), collector.handle(1));
    let first = std::ptr::from_ref(reader.current()).addr();

    writer.publish(spy());
    writer.publish(spy()); // replaces the unread one: dropped by the writer
    assert_eq!(drops.lock().unwrap().len(), 1);
    let audio = thread::spawn(move || {
        let switched = no_alloc(|| std::ptr::from_ref(reader.latest()).addr() != first);
        assert!(switched);
        reader
    });
    let mut reader = audio.join().unwrap();
    assert_eq!(
        drops.lock().unwrap().len(),
        1,
        "nothing freed on the audio thread"
    );

    writer.publish(spy());
    let held: *const DropSpy = reader.current();
    // The DeferredDrop is full (capacity 1), so the reader must keep its value.
    assert!(no_alloc(|| std::ptr::eq(reader.latest(), held)));
    assert_eq!(collector.collect(), 1); // the initial value
    assert_eq!(drops.lock().unwrap().len(), 2);
    assert!(!std::ptr::eq(reader.latest(), held));
    drop((writer, reader));
    assert_eq!(collector.collect(), 1);
    assert_eq!(drops.lock().unwrap().len(), 4);
}

#[test]
fn b5_unread_value_is_freed_when_both_sides_drop() {
    let marker = Arc::new(());
    let mut collector = Collector::new();
    let (writer, reader) = RtHandoff::new(Box::new(Arc::clone(&marker)), collector.handle(1));
    writer.publish(Box::new(Arc::clone(&marker)));
    drop((reader, writer));
    assert_eq!(Arc::strong_count(&marker), 1);
}

#[test]
fn b6_no_alloc_passes_through_results_and_nests() {
    let v = [1_u32, 2, 3];
    let sum = no_alloc(|| no_alloc(|| v.iter().sum::<u32>()) + 1);
    assert_eq!(sum, 7);
    let after = Box::new([0_u8; 16]); // allowed again after leaving the guard
    assert_eq!(after.len(), 16);
}

/// Name of the env var that turns `b6_allocation_inside_no_alloc_aborts` into the child.
const CHILD: &str = "SYRP_RT_B6_CHILD";

#[test]
#[cfg(debug_assertions)]
#[cfg_attr(miri, ignore = "spawns a process")]
fn b6_allocation_inside_no_alloc_aborts_with_code() {
    match std::env::var(CHILD).as_deref() {
        Ok("alloc") => {
            no_alloc(|| std::hint::black_box(Box::new(1_u64)));
            unreachable!("allocation was not caught");
        }
        Ok("dealloc") => {
            let b = std::hint::black_box(Box::new(1_u64));
            no_alloc(move || drop(b));
            unreachable!("deallocation was not caught");
        }
        _ => {}
    }
    for mode in ["alloc", "dealloc"] {
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "b6_allocation_inside_no_alloc_aborts_with_code",
                "--exact",
                "--nocapture",
            ])
            .env(CHILD, mode)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{mode}: child did not abort");
        assert!(
            stderr.contains("SYRP-RT-0001"),
            "{mode}: stderr was {stderr}"
        );
    }
}

#[test]
fn b6_rt_error_code_is_catalogued() {
    let catalogue = include_str!("../../../../errors/rt.toml");
    assert!(catalogue.contains("code = \"SYRP-RT-0001\""));
}

#[test]
fn b7_every_unsafe_block_and_impl_has_a_safety_comment() {
    let sources = [
        include_str!("../src/lib.rs"),
        include_str!("../src/atomic.rs"),
        include_str!("../src/spsc.rs"),
        include_str!("../src/deferred.rs"),
        include_str!("../src/handoff.rs"),
        include_str!("../src/alloc_guard.rs"),
    ];
    for src in sources {
        let lines: Vec<&str> = src.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let code = line.split("//").next().unwrap();
            if code.contains("unsafe {") || code.contains("unsafe impl") {
                // The comment block directly above the line or its statement.
                let mut start = i;
                while start > 0 && lines[start].trim_start().starts_with('.') {
                    start -= 1; // a method chain: the comment sits above the statement
                }
                while start > 0 && lines[start - 1].trim_start().starts_with("//") {
                    start -= 1;
                }
                assert!(
                    lines[start..=i].iter().any(|l| l.contains("SAFETY:")),
                    "no SAFETY comment for line {}: {line}",
                    i + 1
                );
            }
        }
    }
}

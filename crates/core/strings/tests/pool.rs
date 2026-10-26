//! B2: StringPool.

use std::sync::Arc;
use std::thread;

use syrp_strings::StringPool;

#[test]
fn b2_equal_text_returns_the_same_allocation() {
    let pool = StringPool::new();
    let a = pool.get("frequency");
    let owned = String::from("frequency");
    let b = pool.get(&owned);
    assert!(Arc::ptr_eq(&a, &b));
    assert_ne!(owned.as_ptr(), a.as_ptr());
    let c = pool.get("Frequency");
    assert!(!Arc::ptr_eq(&a, &c));
    assert_eq!(pool.len(), 2);
}

#[test]
fn b2_garbage_collect_frees_only_unheld_entries() {
    let pool = StringPool::new();
    let held = pool.get("held");
    let weak = Arc::downgrade(&pool.get("dropped"));
    assert_eq!(pool.len(), 2);
    pool.garbage_collect();
    assert_eq!(pool.len(), 1);
    assert!(weak.upgrade().is_none(), "the unheld text must be freed");
    assert!(Arc::ptr_eq(&held, &pool.get("held")));
    drop(held);
    pool.garbage_collect();
    assert!(pool.is_empty());
}

#[test]
fn b2_pool_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<StringPool>();
}

#[test]
fn b2_threads_share_one_allocation_per_text() {
    let pool = Arc::new(StringPool::new());
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let pool = Arc::clone(&pool);
            thread::spawn(move || {
                (0..100)
                    .map(|i| pool.get(&format!("id{}", i % 10)))
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    let results: Vec<Vec<Arc<str>>> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(pool.len(), 10);
    for result in &results[1..] {
        for (x, y) in result.iter().zip(&results[0]) {
            assert!(Arc::ptr_eq(x, y));
        }
    }
}

#[test]
fn b2_global_pool_is_shared() {
    let a = StringPool::global().get("b2-global");
    assert!(Arc::ptr_eq(&a, &StringPool::global().get("b2-global")));
}

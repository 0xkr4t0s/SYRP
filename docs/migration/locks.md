---
juce: juce::CriticalSection
syrp: std::sync::Mutex
notes: "Not re-entrant: split methods into a locking public one and a private one taking &mut State. ScopedLock is the MutexGuard."
status: done
---

# Locks and thread-local values

JUCE's lock classes are not rewritten in Syrp. Most map onto the standard library. Two are
provided by `syrp-thread`: `SpinLock`, because `std` has no spin lock, and `WaitableEvent`.

| JUCE | Syrp | Notes |
|---|---|---|
| `CriticalSection` | `std::sync::Mutex<T>` | Not re-entrant: see below. The mutex owns the data it protects. |
| `ScopedLock` (`const ScopedLock sl (cs);`) | `let guard = mutex.lock().unwrap();` | The guard is the scope. Drop it early with `drop(guard)`. |
| `ScopedTryLock` | `mutex.try_lock()` | `Ok(guard)` when the lock was free. |
| `ScopedUnlock` | `drop(guard)` and then `mutex.lock()` again | Rust has no "unlock for a scope". |
| `ReadWriteLock`, `ScopedReadLock`, `ScopedWriteLock` | `std::sync::RwLock<T>`, `.read()`, `.write()` | Not re-entrant, and a reader cannot upgrade to a writer. |
| `ThreadLocalValue<T>` | `thread_local! { static X: Cell<T> / RefCell<T> = ... }` | Use `X.with(...)`, or `X.get()`/`X.set()` on a `Cell`. |
| `SpinLock`, `SpinLock::ScopedLockType` | `syrp_thread::SpinLock<T>`, `SpinLockGuard` | `lock()` spins with backoff and then yields. `try_lock()` makes no system call. |
| `WaitableEvent` | `syrp_thread::WaitableEvent` | `new(manual_reset)`, `wait(Option<Duration>) -> bool`, `signal()`, `reset()`. |

## The locks own their data

A JUCE `CriticalSection` sits next to the fields it protects, and every access has to remember
to take it. A Rust `Mutex<T>` and `SpinLock<T>` hold the `T`. The data can only be reached
through the guard, so forgetting the lock is a compile error. Several fields protected by one
lock become one struct inside one mutex: `Mutex<State>`.

`SpinLock::new(())` is a lock with no data, as in JUCE. Prefer putting the data inside.

## CriticalSection is re-entrant; Mutex is not

A JUCE thread that already holds a `CriticalSection` can lock it again. With `std::sync::Mutex`,
a second `lock()` on the same thread deadlocks or panics, and `SpinLock` spins for ever. Code
that relies on re-entry usually has a public method that locks and then calls another public
method that locks too. To restructure it:

1. Split each such method into a public method that takes the lock and a private one that
   takes the guarded data (`&mut State`) and does the work.
2. Have public methods call the private ones while holding the one guard, never each other.
3. Callbacks that might re-enter (listeners, user closures) run after the guard is dropped. Copy
   out what they need first.

```rust
use std::sync::Mutex;

struct Counter { state: Mutex<State> }
struct State { value: i64, log: Vec<i64> }

impl Counter {
    pub fn add(&self, n: i64) {
        let mut s = self.state.lock().unwrap();
        Self::add_locked(&mut s, n);
    }
    pub fn add_twice(&self, n: i64) {
        let mut s = self.state.lock().unwrap(); // one lock, no re-entry
        Self::add_locked(&mut s, n);
        Self::add_locked(&mut s, n);
    }
    fn add_locked(s: &mut State, n: i64) {
        s.value += n;
        s.log.push(n);
    }
}
```

## Poisoning

If a thread panics while holding a `std` lock, the lock is marked poisoned and `lock()` returns
`Err`. JUCE has no such state. Either propagate the panic with `.unwrap()`, or recover the data
with `.unwrap_or_else(std::sync::PoisonError::into_inner)` when it cannot be left half-updated.
`SpinLock` does not poison: its guard releases on unwind.

## WaitableEvent

The behaviour matches JUCE. An auto-reset event (`new(false)`) lets one waiter through per
signal. A manual-reset event (`new(true)`) stays signalled until `reset()`. A signal raised
before anyone waits is kept. JUCE's `wait(-1)` becomes `wait(None)` and `wait(ms)` becomes
`wait(Some(Duration::from_millis(ms)))`.

Not covered here: `InterProcessLock` (WP-1.1-08). The audio thread must not take any of these
locks; use the lock-free types in `syrp-rt`.

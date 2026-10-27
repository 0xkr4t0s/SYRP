---
juce: juce::SpinLock
syrp: syrp_thread::SpinLock
notes: "Owns its data (SpinLock<T>); lock spins with backoff then yields; try_lock makes no system call; not re-entrant."
status: done
---

# SpinLock

`syrp_thread::SpinLock<T>`. `SpinLock::ScopedLockType` becomes the `SpinLockGuard` returned by `lock()`. See [Locks and thread-local values](locks.md) for the full mapping and its pitfalls.

---
juce: juce::ReadWriteLock
syrp: std::sync::RwLock
notes: "Not re-entrant and readers cannot upgrade; ScopedReadLock/ScopedWriteLock are .read()/.write() guards."
status: done
---

# ReadWriteLock

`std::sync::RwLock<T>`. `ScopedReadLock` and `ScopedWriteLock` become the guards returned by `.read()` and `.write()`. See [Locks and thread-local values](locks.md) for the full mapping and its pitfalls.

---
juce: juce::ThreadLocalValue
syrp: std::thread_local
notes: "Use a thread_local! static holding a Cell or RefCell."
status: done
---

# ThreadLocalValue

A `thread_local!` static holding a `Cell<T>` or `RefCell<T>`, read and written with `.with(...)`, `.get()` and `.set()`. See [Locks and thread-local values](locks.md) for the full mapping and its pitfalls.

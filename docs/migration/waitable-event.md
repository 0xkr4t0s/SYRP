---
juce: juce::WaitableEvent
syrp: syrp_thread::WaitableEvent
notes: "wait(-1) is wait(None); wait(ms) is wait(Some(Duration::from_millis(ms)))."
status: done
---

# WaitableEvent

`syrp_thread::WaitableEvent`, with the same auto-reset and manual-reset behaviour. See [Locks and thread-local values](locks.md) for the full mapping and its pitfalls.

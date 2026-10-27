# syrp-thread

## Purpose
The L2 threading layer (master plan §5.2, §5.4): named threads with cooperative exit
signalling, notification and timed stop, built on `std::thread`; later increments add
priorities and realtime scheduling, `WaitableEvent`, `ThreadPool` and `SpinLock`. Covers JUCE's
Thread. Behaviour comes from the WP spec and the public JUCE class documentation only.

## Ledger rows
juce_core/Thread, juce_core/ThreadPool, juce_core/ThreadPoolJob, juce_core/ThreadPoolOptions,
juce_core/WaitableEvent, juce_core/SpinLock, juce_core/CriticalSection, juce_core/ReadWriteLock,
juce_core/ThreadLocalValue

## Invariants
- A thread is never killed. `ThreadHandle::stop` signals, waits, joins; on timeout it returns
  `SYRP-THREAD-0001` and detaches. `Drop` does the same with the builder's drop timeout
  (`DEFAULT_DROP_TIMEOUT`, 5 s) and reports nothing.
- `finished` is set by a guard on the spawned thread, so it also flips when the body panics.
- Once the exit signal is raised, `ThreadContext::wait` returns `true` immediately. Notifications
  do not queue: one flag, consumed by the next wait.
- No `unsafe` yet. When platform code arrives, every `unsafe` block gets a `// SAFETY:`
  comment (clippy `undocumented_unsafe_blocks`).

## Thread affinity
Any thread. Nothing here is RT-safe except `ThreadContext::thread_should_exit` (one atomic
load); `notify`, `wait` and `is_running` take a short mutex.

## Stability
experimental

## How to test
```sh
cargo syrp test --crate syrp-thread
```
Platform code (next increment) must also pass `cargo check --target x86_64-apple-darwin` and
`--target x86_64-pc-windows-msvc`.

## Known gaps
WP-1.1-06 increment 1 (B1 and the B6 error domain with codes 0001-0002). Still to come:
priorities, realtime options and affinity (B2), `WaitableEvent` with a loom model (B3), the
lock mapping page and `SpinLock` (B5), then `ThreadPool` (B4).
`errors/thread.toml` is not yet in the CLI's embedded catalogue.

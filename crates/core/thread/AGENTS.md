# syrp-thread

## Purpose
The L2 threading layer (master plan §5.2, §5.4): named threads with cooperative exit
signalling, notification and timed stop, priorities and realtime scheduling, `WaitableEvent` and
`SpinLock`; `ThreadPool` comes later. Covers JUCE's Thread, WaitableEvent and SpinLock; JUCE's
other locks map to `std` (`docs/migration/locks.md`). Platform code is in
`src/os/` (one module per OS: `linux.rs`, `apple.rs`, `windows.rs`, plus `other.rs`, which
refuses everything). Behaviour comes from the WP spec and the public JUCE class documentation only.

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
- Priority and realtime requests are applied on the new thread before its body runs; `spawn`
  waits for the outcome. Refusals become `SchedulingStatus::Refused`, never a spawn failure.
- The per-OS mapping table lives in the `priority` module rustdoc; keep it in step with
  `src/os/`.
- Every `unsafe` block has a `// SAFETY:` comment (clippy `undocumented_unsafe_blocks`).
- `WaitableEvent` and `SpinLock` take their atomics, mutex, condvar and cell from `src/sync.rs`,
  which swaps in `loom` under `--cfg loom`, so the models check the shipped code. `loom` is
  only a dependency under `cfg(loom)`.
- `WaitableEvent` is one `bool` behind a mutex: signals do not count, auto-reset clears it in
  the wait that returns `true`, `signal` notifies one waiter (auto) or all (manual).
- `SpinLock::try_lock` is one compare-and-swap and must stay free of system calls; `lock`
  spins with doubling backoff for 7 rounds, then yields. It is not re-entrant.

## Thread affinity
Any thread. Nothing here is RT-safe except `ThreadContext::thread_should_exit` (one atomic
load) and `SpinLock::try_lock` (one compare-and-swap, but a spin lock still blocks a realtime
thread behind a preempted holder); `notify`, `wait`, `is_running` and every `WaitableEvent`
call take a short mutex. `set_current_thread_*` act on
the calling thread only.

## Stability
experimental

## How to test
```sh
cargo syrp test --crate syrp-thread
cargo clippy -p syrp-thread --all-targets --target x86_64-apple-darwin
cargo clippy -p syrp-thread --all-targets --target x86_64-pc-windows-msvc
# loom models of WaitableEvent (B3) and SpinLock (B5):
RUSTFLAGS="--cfg loom" CARGO_TARGET_DIR=target/loom cargo test -p syrp-thread --release --test loom
```
Realtime, raised-priority and affinity tests accept "granted" or the documented refusal, since
CI runners usually lack `RLIMIT_RTPRIO` and `CAP_SYS_NICE`.

## Known gaps
WP-1.1-06 increments 1-3 (B1, B2, B3, B5 and the B6 error domain, codes 0001-0006). Still
to come: `ThreadPool` (B4). The loom models are not wired into CI yet, and the migration pages
are not linked from `docs/SUMMARY.md` (a shared file).
`errors/thread.toml` is not yet in the CLI's embedded catalogue.

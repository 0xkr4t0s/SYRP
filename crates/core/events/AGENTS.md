# syrp-events

## Purpose
The owned desktop message loops (master plan §5.5, kind 1): `EventLoop`, the loop an app runs
on its main thread. It implements the frozen `syrp_loop_api::MessageLoop` contract through the
implementer hooks (`TaskSender::new`, `CallbackGuard::new`, `MessageThread::new_unchecked`) and
adds `run`, `run_for`, `dispatch_pending`, `with_token`, `now` and `QuitHandle`. Covers JUCE's MessageManager (`callAsync`, `callFunctionOnMessageThread`,
`isThisTheMessageThread`), CallbackMessage, Message and MessageListener. Behaviour comes from
the WP spec and public JUCE documentation only. Layout: `src/event_loop.rs` is the
platform-neutral core (queue, timer schedule, dispatch pass); `src/os/` holds the backends.

## Ledger rows
juce_events/MessageManager, juce_events/MessageManagerLock (not provided, §5.5),
juce_events/CallbackMessage, juce_events/Message, juce_events/MessageListener

## Invariants
- At most one `EventLoop` per thread (thread-local flag, `SYRP-LOOP-0004`); it is `!Send`.
- Tokens are made only by the loop, on its thread, for one dispatch pass or one `with_token`
  call; every `new_unchecked` has a `// SAFETY:` comment.
- `post` only enqueues and wakes; a pass runs the tasks queued when it starts, in order.
- Timers: monotonic clock, absolute deadlines, (due, creation id) order, periods under 1 ms
  become 1 ms, first firing one period after creation. A late timer fires once and keeps its
  phase (`next_due`), unlike `ManualLoop::advance` (deliberate, tested `b3_..._syrp_choice`).
- No `RefCell` borrow or lock is held while a callback runs or is dropped (guards re-borrow).
- Dropping the loop drops queued tasks, which wakes `post_and_wait` callers with
  `SYRP-LOOP-0003`; senders and quit handles hold only weak references.
- `contracts/` and `syrp-loop-api` are never changed here.

## Thread affinity
`EventLoop` and its guards and callbacks live on the message thread. `TaskSender` and
`QuitHandle` are any-thread. Nothing here is RT-safe.

## Stability
experimental

## How to test
```sh
cargo syrp test --crate syrp-events
cargo check -p syrp-events --target x86_64-apple-darwin
cargo check -p syrp-events --target x86_64-pc-windows-msvc
```
`tests/conformance.rs` (B2) runs one suite against both `ManualLoop` and `EventLoop`; extend it
there, never in two places. Timing tests use loose bounds.

## Known gaps
WP-1.1-14 increment 1 (B1-B3, B9 codes). Every backend is interim, with no OS event sources:
a condvar on Linux, a condvar under `timeBeginPeriod(1)` on Windows, and a `kqueue` with a
`NOTE_CRITICAL` timer on macOS (Darwin coalesces ordinary timed waits). The `epoll` backend
with `add_fd` (B4), `CFRunLoop` (B5) and Win32 (B6) replace them.
`on_vblank` always uses the 60 Hz fallback timer; `VBlankDriver` (B7) and the migration page
(B8) follow. `errors/loop.toml` codes 0004-0006 are in the existing domain.

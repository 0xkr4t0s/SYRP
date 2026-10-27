# syrp-events

## Purpose
The owned desktop message loops (master plan §5.5, kind 1): `EventLoop`, the loop an app runs
on its main thread. It implements the frozen `syrp_loop_api::MessageLoop` contract through the
implementer hooks (`TaskSender::new`, `CallbackGuard::new`, `MessageThread::new_unchecked`) and
adds `run`, `run_for`, `dispatch_pending`, `with_token`, `now`, `QuitHandle` and, on Linux,
`add_fd`. Covers JUCE's MessageManager (`callAsync`, `callFunctionOnMessageThread`,
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
`tests/windows.rs` (B6) and `tests/macos_main_thread.rs` (B5) run only on their OS.
`tests/conformance.rs` (B2) runs one suite against both `ManualLoop` and `EventLoop`; extend it
there, never in two places. Timing tests use loose bounds.

## Known gaps
WP-1.1-14 increments 1-3 (B1-B6, B9 codes). Backends: `src/os/epoll.rs` (Linux: eventfd
wake, `epoll_wait` timeout rounded up to whole ms, `add_fd`), `src/os/cfrunloop.rs` (macOS:
a `CFRunLoopSource` for wakes; timer deadlines from a `NOTE_CRITICAL` kqueue timer surfaced
through a `CFFileDescriptor`, not a `CFRunLoopTimer`, because Darwin coalesces ordinary timed
waits), `src/os/win32.rs` (message-only window, `MsgWaitForMultipleObjectsEx` under
`timeBeginPeriod(1)`), `src/os/unsupported.rs` (other targets). Backends whose OS loop can be
driven by someone else call the core back through `Poller::attach`; `Poller::arm` tells them
the next deadline. Under a foreign message pump on Windows, timers run only when a wake
arrives. `add_fd` is Linux only. macOS and Windows code is compile-checked locally and run
only on CI; `tests/macos_main_thread.rs` (`harness = false`) needs the process main thread.
`on_vblank` always uses the 60 Hz fallback timer; `VBlankDriver` (B7) and the migration page
(B8) follow. `errors/loop.toml` codes 0004-0006 are in the existing domain.

# syrp-loop-api

## Purpose
The L1 message-loop contract (master plan §5.5): the `MessageThread` token, the `MessageLoop`
trait (`post`, `task_sender`, `timer`, `on_vblank`, `post_and_wait`), `Task`/`TaskSender`,
`CallbackGuard` (aliased `TimerGuard`/`VBlankGuard`), `PeerId`, `Coalescer` and `ManualLoop`,
the deterministic virtual-clock loop all tests use. Covers JUCE's MessageManager
(`callAsync`, `callSync`), Timer, MultiTimer, AsyncUpdater and VBlankAttachment.

## Ledger rows
juce_events/MessageManager, juce_events/Timer, juce_events/MultiTimer,
juce_events/AsyncUpdater, juce_gui_basics/VBlankAttachment (shared with `syrp-events`)

## Invariants
- `MessageThread` is zero-sized, `!Send`, `!Sync`; safe code gets one only from a loop.
  Loops outside this crate use `unsafe MessageThread::new_unchecked`.
- Contract types have no public fields (ADR 0003 D3); the `b6_` test scans the sources.
- `post` never runs a task inline. `post_and_wait` on the message thread fails with
  `SYRP-LOOP-0001`; a dropped loop wakes waiters with `SYRP-LOOP-0003`.
- `ManualLoop::advance` fires timers in (due time, creation) order with `now()` at the due
  time; a timer made in a callback first fires one period later; periods under 1 ms become
  1 ms. `advance` runs no tasks; `run_until_idle` does not move the clock and stops at the
  task limit (default 10,000) with `SYRP-LOOP-0002`, leaving the rest queued.
- Never hold a `RefCell` borrow or the queue lock while running user code or dropping a
  callback (callbacks own guards whose drop re-borrows).
- `Coalescer` queues at most one task; a trigger during the callback schedules one more run;
  `cancel` or drop withdraws a pending update.

## Thread affinity
`TaskSender` and `Coalescer::trigger` are any-thread. `ManualLoop` is `!Send`; its creating
thread is its message thread. Callbacks, guards and `with_token` are message-thread only.
Nothing here is RT-safe (`post` allocates and locks).

## Stability
contract (frozen after review in `contracts/syrp-loop-api.public-api.txt` by the PR manager)

## How to test
```sh
cargo syrp test --crate syrp-loop-api
```

## Known gaps
Owned and hosted loops (Win32, CFRunLoop, epoll, CLAP/VST3) and `Broadcaster<T>` belong to
`syrp-events` and the peer crates. No `Coalescer::flush` (handleUpdateNowIfNeeded) or futures
integration yet.

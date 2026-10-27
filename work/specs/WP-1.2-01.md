# WP-1.2-01: syrp-events: Timer, MultiTimer, flushable coalescers, Broadcaster<T> and ActionBroadcaster

## Purpose
Wave 1.2 item 1 and the `syrp-events` half of master plan §5.5's mapping table (Timer and MultiTimer to timer guards, AsyncUpdater to `Coalescer`, ChangeBroadcaster to `Broadcaster<T>`). The frozen `syrp-loop-api` contract (WP-0.2-01) already has `MessageLoop::timer`, `CallbackGuard` and `Coalescer`; this WP adds the owned, restartable objects ported and native code use on top of any `MessageLoop`, so everything here works the same on `ManualLoop` and on WP-1.1-14's `EventLoop`. Nothing under `contracts/` changes; a gap in the trait means a contract-change WP. It works in the crate WP-1.1-14 creates. `PropertiesFile` (WP-1.2-06) broadcasts its changes through `Broadcaster<()>`. Delivery: 2 PRs, each at most 1,200 changed lines (B1–B3, then B4–B7).

## Behaviour
- B1. `Timer`: an owned, restartable timer over any `&dyn MessageLoop`, as the `Timer` documentation describes: `Timer::new(callback)`, `start(loop, mt, interval)`, `start_hz(loop, mt, hz)`, `stop(mt)`, `is_running()` and `interval() -> Option<Duration>`. Restarting a running timer resets its phase; an interval of zero stops it (not an error); stopping it inside its own callback means no further call; dropping it stops it. `call_after_delay(loop, mt, delay, f)` is the one-shot form (JUCE's static `Timer::callAfterDelay`) and returns a guard that cancels it. `TimedCallback` is a type alias or thin wrapper of `Timer` over a closure, as JUCE's class is. Message-thread only.
- B2. `MultiTimer`: timers keyed by an `i32` id with one callback that receives the id: `start(loop, mt, id, interval)`, `stop(mt, id)`, `is_running(id)`, `interval(id)` and `stop_all`. Ids are independent; starting an id that runs restarts it. Callbacks for ids due together run in (due, start order), the order the loop contract gives.
- B3. `Updater` (the `AsyncUpdater` and `LockingAsyncUpdater` behaviour that the frozen `Coalescer` lacks): `trigger()` from any thread queues at most one call; `cancel()`; `is_pending()`; `flush(mt)` runs a pending update now on the message thread and clears it (`handleUpdateNowIfNeeded`); drop cancels. `LockingUpdater` adds the guarantee the `LockingAsyncUpdater` documentation gives: after `cancel()` returns on any thread, no callback is running or will start, except that `cancel` from inside the callback returns at once. Build both on `TaskSender`; tests drive them from several threads with `loom` or with a stress test of at least 10,000 trigger and cancel pairs.
- B4. `Broadcaster<T: Clone + Send + 'static>` (ChangeBroadcaster and ChangeListener): `subscribe(mt, FnMut(&MessageThread, &T)) -> Subscription` (dropping the guard unsubscribes), `send(value)` from any thread (asynchronous and coalesced: several sends before delivery deliver once, with the last value), `send_now(mt, value)` (synchronous, `sendSynchronousChangeMessage`) and `dispatch_pending(mt)` (`dispatchPendingMessages`). Listeners run in subscription order; one added during a delivery does not get that delivery, one removed during a delivery is not called after its removal, and a listener may drop its own subscription inside its callback. `Broadcaster<()>` is the plain ChangeBroadcaster.
- B5. `ActionBroadcaster` (ActionBroadcaster and ActionListener): `send_action_message(String)` from any thread queues every message, with no coalescing; each is delivered asynchronously, in send order, to the listeners subscribed at delivery time, with the same subscription rules as B4.
- B6. Tests: one suite generic over a small driver trait (the one WP-1.1-14 B2 introduces) runs B1–B5 on `ManualLoop` with exact counts and on `EventLoop` on every platform with loose timing bounds. Coalescing, ordering, drop-inside-callback and cross-thread `send`/`trigger` each have a test.
- B7. A `docs/migration/` page maps `Timer`, `MultiTimer`, `TimedCallback`, `Timer::callAfterDelay`, `AsyncUpdater`, `LockingAsyncUpdater`, `ChangeBroadcaster`, `ChangeListener`, `ActionBroadcaster` and `ActionListener` to this API and to the contract's `Coalescer`, and explains why a Syrp timer needs a loop and a token where JUCE's has neither. Any new error continues `errors/loop.toml` from the next free code; the frozen `LoopError` is unchanged.

## Platforms
Linux, macOS and Windows; PR CI runs B6 on all three. Nothing here is platform-specific: it uses only `syrp-loop-api` and WP-1.1-14's `EventLoop`.

## Not in this WP
`MessageManagerLock` (not provided, master plan §5.5), `DeletedAtShutdown`, `MountedVolumeListChangeDetector`, futures integration, and any change to `Coalescer` or `MessageLoop` themselves.

## Crate setup
The whole crate `crates/core/events/`, which WP-1.1-14 creates; this WP starts after WP-1.1-14 merges and may edit any file there, including `Cargo.toml` and `AGENTS.md` (add this WP's ledger rows, invariants and gaps). No new crate.
Licence: `license = "AGPL-3.0-only OR BUSL-1.1"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-1.2-01` first and `cargo syrp wp check WP-1.2-01` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

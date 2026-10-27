# syrp-analytics

## Purpose
The headless core of JUCE's `juce_analytics` module (master plan §5.2, L2 Headless extras).
`Analytics` holds a user id, user properties and an ordered list of boxed
`AnalyticsDestination`s, and turns each `log_event` call into one `AnalyticsEvent` passed to
every destination. `ThreadedAnalyticsDestination<S: BatchSink>` queues events and sends them
in batches on its own thread. Destinations that send events anywhere are Wave 5.1.

## Ledger rows
juce_analytics/Analytics, juce_analytics/AnalyticsDestination,
juce_analytics/ThreadedAnalyticsDestination.

## Invariants
- No public fields (ADR 0003 D3). `AnalyticsEvent` lends a `Copy` `AnalyticsEventView`; its
  accessors take `self` and return `&'a` data. Parameters and user properties keep the
  caller's order, duplicates included.
- One `Mutex` guards user id, user properties and destinations. `log_event` builds the event
  (timestamp from `syrp_sys::Time::millisecond_counter`), then calls every destination in
  insertion order while holding the lock, so all destinations see one global event order.
  A destination must not call back into the same `Analytics` (deadlock).
- A destination panic is caught with `catch_unwind`, logged through `syrp_sys::write_to_log`,
  and the next destination still runs. Lock poisoning is ignored (`PoisonError::into_inner`).
- `set_suspended(true)` drops events (they are not queued). The flag is an `AtomicBool`.
- `Analytics::global()` is a `static` built by the `const fn new`; it is never dropped.
- Threaded (`src/threaded/`): all scheduling is in the clock-free `core.rs::step`; the thread
  feeds it monotonic milliseconds and sleeps on the `Condvar`, tests use a manual clock.
- Queue: bounded `VecDeque` (default 10,000); a full queue drops the new event and counts it.
  Restored events go to the front and ignore the bound. A batch is cloned from the front and
  removed only after the sink returns `true`, so a failed batch is retried first, in order.
- Generations: each start gets a number; `active` = may run, `owner` = may remove delivered
  events. A stop clears `active`, calls `stop_logging_events`, waits for `finished` (timed
  `Condvar` wait), clears `owner` and saves the whole queue if non-empty. A thread outliving a
  timed-out stop cannot touch the queue, and a new start is refused until it exits.
- Back-off (Syrp choice): a failure (or sink panic) doubles the period up to 60 s (or the
  base, if longer); success resets it. `maximum_batch_size() == 0` sends one event.
- `BatchSink` methods take `&self` (not `&mut self` as the spec sketched) because
  `stop_logging_events` must run while `log_batched_events` is in progress.
- Errors: `AnalyticsError`, `errors/analytics.toml` (`SYRP-ANALYTICS-0001..0004`).

## Thread affinity
Every API is any-thread and takes `&self`. None is RT-safe (locks and allocates).
`log_batched_events` runs on the analytics thread, `restore_unlogged_events` on the starter's
thread, `stop_logging_events` and `save_unlogged_events` on the stopper's (or dropper's).

## Stability
experimental

## How to test
```sh
cargo syrp test --crate syrp-analytics
```
`tests/panics.rs` installs a process-wide logger, so it lives in its own test binary. The
batching rules are unit tests in `src/threaded/tests/` on a manual clock (`robustness.rs` has
the B4 proptest); `tests/threaded.rs` drives a real thread, waiting on a `Condvar` for what the
sink saw. Do not add tests that sleep or assert on elapsed time.

## Known gaps
- `SYRP-ANALYTICS-0001` (thread spawn failure) has no test: it cannot be provoked portably.
- Network destinations (Wave 5.1) and `ButtonTracker` (needs `syrp-ui`).

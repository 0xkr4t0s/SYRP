# syrp-analytics

## Purpose
The headless core of JUCE's `juce_analytics` module (master plan §5.2, L2 Headless extras).
`Analytics` holds a user id, user properties and an ordered list of boxed
`AnalyticsDestination`s, and turns each `log_event` call into one `AnalyticsEvent` passed to
every destination. `ThreadedAnalyticsDestination` (batching on a background thread) arrives in
the second increment of WP-1.2-11. Destinations that send events anywhere are Wave 5.1.

## Ledger rows
juce_analytics/Analytics, juce_analytics/AnalyticsDestination,
juce_analytics/ThreadedAnalyticsDestination (second increment).

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

## Thread affinity
Every API is any-thread and takes `&self`. None is RT-safe (locks and allocates).

## Stability
experimental

## How to test
```sh
cargo syrp test --crate syrp-analytics
```
`tests/panics.rs` installs a process-wide logger, so it lives in its own test binary.

## Known gaps
- `ThreadedAnalyticsDestination`, `BatchSink` and `errors/analytics.toml`: WP-1.2-11
  increment 2.
- Network destinations (Wave 5.1) and `ButtonTracker` (needs `syrp-ui`).

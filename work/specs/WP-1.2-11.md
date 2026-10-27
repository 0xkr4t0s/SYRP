# WP-1.2-11: syrp-analytics: headless Analytics, AnalyticsDestination and ThreadedAnalyticsDestination

## Purpose
Wave 1.2 item 11: the headless core of `juce_analytics` from master plan §5.2 (L2 Headless extras); the master plan's module table puts the core in phase 1 and the destinations in phase 5 (Wave 5.1 item 14). With no destination that talks to the network, the core needs neither `syrp-net` nor `syrp-events` (master plan §5.3 lists both for the finished crate), only timestamps from `syrp-sys` (WP-1.1-12) and a background thread from `std::thread`. This WP creates `syrp-analytics`. Delivery: 2 PRs, each at most 1,200 changed lines (B1–B2, then B3–B5).

## Behaviour
- B1. `AnalyticsEvent`: name, event type (`i32`), timestamp (milliseconds from `syrp_sys`' millisecond counter), parameters (ordered string pairs), user id and user properties, with accessors and no public fields. `Analytics`: `Analytics::new()` plus `Analytics::global()` for the process-wide instance JUCE's singleton provides; `Send + Sync` and internally locked. `set_user_id`, `set_user_properties`, `add_destination(Box<dyn AnalyticsDestination>)`, `destinations()`, `log_event(name, parameters, event_type)` (builds one event with the current user id and properties and passes it to every destination in the order they were added), and `set_suspended(bool)` (events logged while suspended are dropped, as the `Analytics` documentation describes).
- B2. `AnalyticsDestination`: a `Send + Sync` trait with `log_event(&self, &AnalyticsEvent)`. A destination that panics does not stop the others; the panic is caught and logged through `syrp_sys`' `Logger`.
- B3. `ThreadedAnalyticsDestination::new(thread_name, sink)`, where `sink` implements `BatchSink`: `log_batched_events(&mut self, &[AnalyticsEvent]) -> bool` (`true` when the batch was delivered), `maximum_batch_size()`, `stop_logging_events()`, `save_unlogged_events(&[AnalyticsEvent])` and `restore_unlogged_events(&mut Vec<AnalyticsEvent>)`, matching the virtual methods the class documents. `log_event` queues; `start_analytics_thread(initial_batch_period)` first restores saved events, then every period sends up to `maximum_batch_size` events; a failed batch is kept and retried later, with the period growing after failures and resetting after a success, as the documentation describes (where it is silent, double the period up to a documented cap and pin it with a `b3_..._syrp_choice` test). `stop_analytics_thread(timeout)` stops the thread, calls `stop_logging_events` and passes every unsent event to `save_unlogged_events`; drop does the same with a documented timeout.
- B4. Tests: the batching, retry and back-off rules run deterministically through a step function the thread also uses (`#[doc(hidden)]` or crate-private, with a fake clock); thread tests use loose timing bounds; a fake sink injects failures and checks that no event is lost or duplicated across a stop and restart (proptest over failure patterns).
- B5. Errors use the new `errors/analytics.toml` domain (`SYRP-ANALYTICS-0001`...) for the few fallible calls (for example a thread that cannot start). No network access and no third-party dependency.

## Not in this WP
Destinations that send events anywhere (Google Analytics and others, Wave 5.1), `ButtonTracker` (needs `syrp-ui`), and persistence of unsent events beyond the `BatchSink` hooks.

## Crate setup
Create `crates/core/analytics/` with `Cargo.toml` (inherit `version`, `edition`, `rust-version`, `repository`, `authors`, `publish` from the workspace; `[lints] workspace = true`), `[package.metadata.syrp] layer = "L2"`, an `AGENTS.md` following `docs/agents/crate-agents-template.md`, and `src/lib.rs`. Workspace members are globbed, so no root edit is needed.
Licence: `license = "AGPL-3.0-only OR BUSL-1.1"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-1.2-11` first and `cargo syrp wp check WP-1.2-11` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

# WP-1.1-07: syrp-thread: HighResolutionTimer and TimeSliceThread

## Purpose
The two timed worker types from master plan §5.2 (`syrp-thread`) and Wave 1.1 item 7. `HighResolutionTimer` drives MIDI output scheduling and device polling off the message thread; `TimeSliceThread` runs the background readers behind `BufferingAudioSource` and thumbnails. Both build on WP-1.1-06 threads and priorities. Delivery: 2 PRs (B1–B3, then B4–B6).

## Behaviour
- B1. `HighResolutionTimer::start(interval, callback: impl FnMut() + Send + 'static) -> HighResolutionTimer` with `stop()`, `is_running()`, `interval()` and `set_interval(Duration)`. Intervals below 1 ms become 1 ms. The callback runs on a dedicated high-priority thread, never the message thread, and never overlaps itself. Ticks follow absolute deadlines (start + k × interval), so error does not accumulate; ticks missed while a callback overran are skipped, not bunched (documented, with a test).
- B2. Stopping: `stop()` from another thread returns only after any running callback has returned; `stop()` or `set_interval` from inside the callback takes effect after it returns and does not deadlock, as the class documentation describes; dropping the timer stops it.
- B3. Per-platform waits, written into the rustdoc: Linux uses `clock_nanosleep(TIMER_ABSTIME)` or `timerfd`; macOS uses `mach_wait_until` on a time-constraint thread; Windows uses `CreateWaitableTimerExW` with `CREATE_WAITABLE_TIMER_HIGH_RESOLUTION`, falling back to `timeBeginPeriod(1)` on older systems. Tests use loose bounds that hold on shared CI runners (a 5 ms timer over 500 ms fires between 80 and 101 times, with mean period within 20%). A `#[ignore]`d jitter benchmark records median and 99th-percentile lateness per platform in the crate `AGENTS.md`; CI does not gate on it.
- B4. `TimeSliceClient`: a trait with `use_time_slice(&mut self) -> Slice`, where `Slice::{Again, After(Duration), Done}` replaces JUCE's integer return value; the rustdoc maps the documented integer meanings onto these variants.
- B5. `TimeSliceThread::new(name)`, `start(Priority)`, `add_client(Box<dyn TimeSliceClient + Send>, delay) -> ClientId`, `remove_client(id) -> Option<Box<...>>`, `move_to_front(id)`, `num_clients`, `client_ids` and `stop(timeout)`. After `remove_client` returns, the client is not running and will not be called again (documented guarantee, tested with a client that sleeps). The due client with the earliest time runs next, with ties in add order; the thread sleeps while nothing is due and wakes on `add_client`; a `Done` client is dropped. Test ordering through an injectable clock (`TimeSliceThread::with_clock`, test-only or `#[doc(hidden)]`).
- B6. Errors continue `errors/thread.toml`. Every `unsafe` block has a `// SAFETY:` comment.

## Platforms
Linux, macOS and Windows are required, and CI must pass on all three with the loose bounds of B3.

## Crate setup
The whole crate `crates/core/thread/`, which WP-1.1-06 creates; this WP starts after it merges and may edit any file there, including `Cargo.toml` and `AGENTS.md` (add this WP's ledger rows, invariants and gaps). No new crate.
Licence: `license = "AGPL-3.0-only OR BUSL-1.1"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-1.1-07` first and `cargo syrp wp check WP-1.1-07` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

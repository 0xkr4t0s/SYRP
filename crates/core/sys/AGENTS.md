# syrp-sys

## Purpose
The L2 system layer (master plan §5.2): wall-clock and calendar time, durations and the
monotonic counters. Covers JUCE's Time (as `Time` plus the `DateTime` calendar view) and
RelativeTime; Uuid, SystemStats and Logger arrive in later increments. Behaviour comes from the
WP spec and the public JUCE class documentation only.

## Ledger rows
juce_core/Time, juce_core/RelativeTime, juce_core/Uuid, juce_core/SystemStats, juce_core/Logger

## Invariants
- `Time` is `i64` milliseconds since the Unix epoch, UTC; `RelativeTime` is `f64` seconds.
  Fields are private (ADR 0003). `Time ± RelativeTime` rounds to whole ms and saturates.
- Months count from 0, days of the month from 1, weekdays from 0 (Sunday).
- Calendar maths is proleptic Gregorian in `src/time.rs`. All platform code is in `src/os.rs`:
  the monotonic clock and one primitive, the local UTC offset at an instant. Local fields are
  the UTC fields of `millis + offset`; local-to-UTC tries the offset twice.
- When the OS cannot convert an instant (or on targets other than Unix and Windows), local
  time is UTC.
- Every `unsafe` block has a `// SAFETY:` comment (clippy `undocumented_unsafe_blocks`).

## Thread affinity
Any thread. RT-safe (lock-free, no allocation, no blocking): `Time::millisecond_counter`,
`millisecond_counter_hi_res`, `high_resolution_ticks`, `high_resolution_ticks_per_second`, and
the `Copy` accessors of `Time`, `DateTime` and `RelativeTime` that return numbers. Not RT-safe:
`RelativeTime::description`, local-time conversion (the C library may lock), and
`wait_for_millisecond_counter` (sleeps).

## Stability
experimental

## How to test
```sh
cargo syrp test --crate syrp-sys
```
`tests/sys.rs` installs `syrp_rt::GuardedAllocator` so B3 proves the counters do not
allocate. `tests/local_time.rs` is its own binary with one test because it sets `TZ` (Unix
only; it prints a skip message elsewhere).

## Known gaps
Increment 1 covers B1, B3 and the calendar part of B2. Still to come in WP-1.1-12:
`Time::to_iso8601`/`from_iso8601` and `formatted` (B2) with `errors/sys.toml` and
`SYRP-SYS-0001` (B7), `Uuid` (B4), `SystemStats` (B5), `Logger` and `WriterLogger` (B6).

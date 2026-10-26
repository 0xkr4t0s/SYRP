# syrp-sys

## Purpose
The L2 system layer (master plan §5.2): wall-clock and calendar time, durations, monotonic
counters, UUIDs, system facts and logging. Covers JUCE's Time (as `Time` plus the `DateTime`
calendar view), RelativeTime, Uuid, SystemStats and Logger. Platform code is in `src/os.rs`
(clock, time zone, memory) and `src/os_info.rs` (OS, CPU, user and host queries). Behaviour
comes from the WP spec and the public JUCE class documentation only.

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
  time is UTC. On Unix the C library caches `TZ` after the first local-time call, so
  changing it later in the process may not take effect.
- `ticks()` is the single clock behind every counter; on fallback targets its first call
  initialises a `OnceLock` (call it once off the audio thread).
- ISO 8601 text without an offset is local time; output always has an offset.
- `Uuid` bytes are in written order; ordering and hashing are by bytes. `Uuid::random` sets
  version 4 and the RFC variant on OS entropy (`getrandom`).
- Every `unsafe` block has a `// SAFETY:` comment (clippy `undocumented_unsafe_blocks`).

## Thread affinity
Any thread. RT-safe (lock-free, no allocation, no blocking): `Time::millisecond_counter`,
`millisecond_counter_hi_res`, `high_resolution_ticks`, `high_resolution_ticks_per_second`, and
the `Copy` accessors of `Time`, `DateTime` and `RelativeTime` that return numbers. Not RT-safe:
anything returning `String`, `SystemStats`, local-time conversion (the C library may lock),
`wait_for_millisecond_counter` (sleeps), and the logger functions (they lock and do I/O).
`set_current_logger` must not be called from inside `Logger::log` (deadlock).

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
WP-1.1-12 is complete (B1-B7). Parity notes (deliberate divergences from JUCE):
- `formatted` `%Z` (and `%z`) write the numeric offset such as `-0400`, not a zone name.
- `Time::from_iso8601` truncates fractions past milliseconds and rejects leap seconds.
- `num_cpus` follows `available_parallelism`, so affinity masks and quotas lower it.
- `operating_system_name` is family plus version numbers (`Windows 10.0.22631`), not a
  marketing name; `cpu_vendor`/`cpu_model` use `cpuid` on x86-64 only, the OS elsewhere.
- `Logger` has no welcome message or file trimming; `FileLogger` belongs to `syrp-io`.

`errors/sys.toml` is not yet in the CLI's embedded catalogue.

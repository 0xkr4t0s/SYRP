# WP-1.1-12: syrp-sys: Time, RelativeTime, Uuid, high-resolution counters, SystemStats and Logger

## Purpose
The system layer from master plan §5.2 (L2 Core) and Wave 1.1 item 12. `syrp-io` and `syrp-thread` depend on it, so it unblocks the rest of Wave 1.1 and, through `syrp-events`, `syrp-state` persistence. Delivery: 3 PRs (B1–B3, then B4–B5, then B6–B7). Platform code is limited to Linux, macOS and Windows; other targets compile and return documented fallbacks.

## Behaviour
- B1. `RelativeTime`: a signed duration in seconds (`f64`) with `milliseconds`, `seconds`, `minutes`, `hours`, `days`, `weeks` constructors and accessors, arithmetic and comparison, and `description(max_units)` giving text such as `"2 hours 5 min"`; write the wording into the rustdoc, and match the format the `RelativeTime::getDescription` documentation shows where it shows one.
- B2. `Time`: milliseconds since the Unix epoch (`i64`). `now()`, `from_millis`, `from_components(year, month0, day, hours, minutes, seconds, millis, local: bool)`, component accessors in UTC and local time (`year`, `month` 0-based, `day_of_month`, `day_of_week`, `hours`, `minutes`, `seconds`, `milliseconds`), `utc_offset_seconds`, `to_iso8601`/`from_iso8601` (round trip, with and without the separators the documentation allows), `formatted(&str)` with the strftime-style specifiers the documentation lists, and arithmetic with `RelativeTime`. Local-time conversion uses the OS time-zone database; tests that depend on it set `TZ` (Unix) or skip with a message.
- B3. Counters: `Time::millisecond_counter()` (monotonic `u32`, wraps), `millisecond_counter_hi_res()` (`f64`), `high_resolution_ticks()` and `high_resolution_ticks_per_second()`, all monotonic and lock-free; `wait_for_millisecond_counter(target)`. These are `RtSafe` (documented; test with `syrp_rt::no_alloc`).
- B4. `Uuid`: random (version 4) generation from OS entropy, `nil()`, parse from and format to the 36-character hyphenated form and the 32-character undelimited form, `to_bytes`/`from_bytes`, ordering and hashing by bytes. Parsing rejects malformed text with a coded error.
- B5. `SystemStats`: `operating_system_type()` (an enum with a version where the OS reports one), `operating_system_name()`, `is_64_bit()`, `num_cpus()` (logical) and `num_physical_cpus()`, `memory_size_mb()`, `page_size()`, `cpu_vendor()`, `cpu_model()`, `cpu_speed_mhz()` (best effort; `None` when unknown), CPU feature flags (SSE family, AVX, AVX2, AVX-512F, NEON), `user_login_name()`, `full_user_name()`, `computer_name()`, `environment_variable(name, default)`, `stack_backtrace()` (text). Each platform call is behind a `// SAFETY:` comment where it is `unsafe`.
- B6. `Logger`: a trait with `log(&self, &str)`; `set_current_logger(Option<Box<dyn Logger + Send + Sync>>)` and `write_to_log(&str)`, which goes to the current logger or, with none, to standard error (and the debugger output on Windows). A `WriterLogger` over any `io::Write`. A file logger with a welcome message and size-based trimming is a later WP in `syrp-io`.
- B7. Errors use the new `errors/sys.toml` domain (`SYRP-SYS-0001`...). Dependencies are limited to small, permissively licensed crates (for example `libc`, `windows-sys`, `getrandom`); list each in `deps_added`.

## Not in this WP
`WindowsRegistry`, `RuntimePermissions`, `PlatformDefaults`, `FileLogger` (needs `syrp-io`), `PerformanceCounter`.

## Crate setup
Create `crates/core/sys/` with `Cargo.toml` (inherit `version`, `edition`, `rust-version`, `repository`, `authors`, `publish` from the workspace; `[lints] workspace = true`), `[package.metadata.syrp] layer = "L2"`, an `AGENTS.md` following `docs/agents/crate-agents-template.md`, and `src/lib.rs`. Workspace members are globbed, so no root edit is needed.
Licence: `license = "AGPL-3.0-only OR BUSL-1.1"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-1.1-12` first and `cargo syrp wp check WP-1.1-12` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

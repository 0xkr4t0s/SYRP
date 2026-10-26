# WP-0.1-04: syrp-rt: RT tokens, RtSafe, SPSC queues, deferred drop, atomics, allocation guard

## Purpose
Real-time primitives used on audio threads (master plan §5.4). Nothing here may allocate, lock or block after construction.

## Behaviour
- B1. `unsafe trait RtSafe` marker. Implemented for primitive numeric types, arrays and tuples of `RtSafe`, and the types in this crate. Document what implementing it promises.
- B2. `AtomicF32` and `AtomicF64` with `load`, `store`, `swap`, `fetch_update` and `Ordering` parameters, built on `AtomicU32`/`AtomicU64` bit casts.
- B3. A bounded SPSC queue (`spsc::channel(capacity) -> (Producer<T>, Consumer<T>)`). `try_push` fails with the value when full; `try_pop` returns `None` when empty. Wrapping `rtrb` is allowed and preferred.
- B4. `DeferredDrop`: a handle the audio thread uses to hand a value to a collector so its destructor runs on a non-RT thread. `Collector::collect()` drops everything handed over so far.
- B5. `RtHandoff<T>`: a single-slot publisher. The writer side (non-RT) publishes a new `Box<T>`; the reader side (RT) obtains the latest value without allocating or locking; the old value is reclaimed via `DeferredDrop`, never on the RT thread.
- B6. An allocation guard for tests: `#[global_allocator]`-installable `GuardedAllocator` plus `fn no_alloc<R>(f: impl FnOnce() -> R) -> R`. Any allocation inside `no_alloc` aborts (debug) with code `SYRP-RT-0001` in the message. Add `errors/rt.toml` with that code.
- B7. Every `unsafe` block has a `// SAFETY:` comment. Provide loom tests (cfg `loom`) for B3 and B5 and Miri-compatible unit tests.

## Acceptance
- Tests for B1-B7; `cargo +nightly miri test -p syrp-rt` documented in AGENTS.md (CI wiring is a separate change request).

## Crate setup
Create `crates/foundation/rt/` with `Cargo.toml` (inherit `version`, `edition`, `rust-version`, `repository`, `authors` from the workspace; `[lints] workspace = true`), `[package.metadata.syrp] layer = "L0"`, an `AGENTS.md` following `docs/agents/crate-agents-template.md`, and `src/lib.rs`. Workspace members are globbed, so no root edit is needed.
Licence: `license = "MIT OR Apache-2.0"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-0.1-04` first and `cargo syrp wp check WP-0.1-04` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

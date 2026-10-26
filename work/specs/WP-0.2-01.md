# WP-0.2-01: auricle-loop-api: MessageLoop trait, tasks, timers, VBlank, plus ManualLoop

## Purpose
The message-loop contract every UI and event crate builds on (master plan §5.5), plus `ManualLoop`, the deterministic loop all tests use.

## Behaviour
- B1. `MessageThread<'a>`: a zero-sized, `!Send`, `!Sync` token proving code runs on the message thread. Only loop implementations can create one.
- B2. `trait MessageLoop`: `post(Task)` callable from any thread; `timer(period, f) -> TimerGuard` (dropping the guard cancels); `on_vblank(peer, f) -> VBlankGuard`; `post_and_wait` that returns an error when called on the message thread itself.
- B3. `ManualLoop` implements `MessageLoop` with a virtual clock: `advance(Duration)` fires due timers in time order (a 30 ms timer fires exactly 3 times over 100 ms), `run_until_idle()` drains posted tasks, including tasks posted by tasks, and stops at a configurable limit with an error instead of looping forever.
- B4. Timer callbacks receive `&MessageThread`. Re-entrancy: a timer created inside a callback first fires on the next due time, never in the same `advance` step at the same instant.
- B5. `Coalescer` (AsyncUpdater equivalent): many `trigger()` calls before the loop runs produce one callback.
- B6. Contract freeze: after review, the public API snapshot is committed by the PR manager to `contracts/auricle-loop-api.public-api.txt`.

## Crate setup
Create `crates/contracts/loop-api/` with `Cargo.toml` (inherit `version`, `edition`, `rust-version`, `repository`, `authors` from the workspace; `[lints] workspace = true`), `[package.metadata.auricle] layer = "L1"`, an `AGENTS.md` following `docs/agents/crate-agents-template.md`, and `src/lib.rs`. Workspace members are globbed, so no root edit is needed.
Licence: `license = "MIT OR Apache-2.0"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo auricle wp show WP-0.2-01` first and `cargo auricle wp check WP-0.2-01` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

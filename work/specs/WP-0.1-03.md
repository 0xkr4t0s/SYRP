# WP-0.1-03: syrp-error: ErrorCode, Diagnostic trait, catalogue generator

## Purpose
The one error vocabulary every Syrp crate uses, so agents can map any failure to a catalogued code with a fix (`cargo syrp explain`).

## Behaviour
- B1. `ErrorCode` is a `Copy` newtype over `&'static str` holding an `SYRP-<DOMAIN>-<NNNN>` code. A `const fn new` validates the shape at compile time when used in a `const` (panic in const evaluation on a malformed code).
- B2. `trait Diagnostic: std::error::Error` exposes `code() -> ErrorCode`, `severity() -> Severity` (Error, Warning, Info), `hint() -> Option<Cow<'static, str>>` and `docs_url() -> String` (derived from the code: `https://docs.syrp.dev/errors/<code>`).
- B3. A `#[derive]`-free helper macro is NOT allowed (master plan: one sanctioned macro). Instead provide a small builder `Report` that carries code, message, optional location and hint, implements `Diagnostic`, and serialises with serde (feature `serde`, default off) to exactly the CLI envelope's diagnostic shape (`docs/agents/envelope.schema.json`, `diagnostics.items`).
- B4. A catalogue generator: `syrp_error::catalogue::parse(&str) -> Result<Vec<Entry>, ParseError>` reads the `errors/*.toml` format (see `errors/cli.toml`), rejects duplicate codes, malformed codes and unknown severities, and `render_markdown(&[Entry]) -> String` renders one page per domain for the docs site.
- B5. Errors are `Send + Sync + 'static`.
- B6. No dependencies except `serde` (optional) and `toml` (behind feature `catalogue`, default off), so the crate stays at L0.

## Acceptance
- Unit tests for B1-B5; a test that parses every real `errors/*.toml` in the repo without error.
- Rustdoc examples on every public item.

## Crate setup
Create `crates/foundation/error/` with `Cargo.toml` (inherit `version`, `edition`, `rust-version`, `repository`, `authors`, `publish` from the workspace; `[lints] workspace = true`), `[package.metadata.syrp] layer = "L0"`, an `AGENTS.md` following `docs/agents/crate-agents-template.md`, and `src/lib.rs`. Workspace members are globbed, so no root edit is needed.
Licence: `license = "MIT OR Apache-2.0"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-0.1-03` first and `cargo syrp wp check WP-0.1-03` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

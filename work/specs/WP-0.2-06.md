# WP-0.2-06: syrp-plugin-api: Processor, ParamSpec, Editor, PluginDescriptor, state traits

## Purpose
The processor and plugin contract (master plan §5.4 and §5.8). The `Processor` sketch in §5.4 is the starting point.

## Data layout
Read `docs/design/0003-data-layout.md` (ADR 0003) first. Hot-path types in this crate have no public fields (D3): expose constructors, accessors, views, handles and iterators, so storage can change without a contract change.

## Behaviour
- B1. `trait Processor` with `prepare(&ProcessConfig, &mut PrepareCtx)`, `process(&mut ProcessIo, &Params, &mut RtCtx)` and `reset`. `PrepareCtx` cannot be obtained inside `process` (prove with a trybuild compile-fail test).
- B2. Bus layout negotiation (`supports_layout`), latency and tail reporting, bypass, precision.
- B3. `ParamSpec` builder: id, name, range with skew/step (NormalisableRange semantics as JUCE documents them), default, unit, value-to-text and text-to-value, smoothing, automatable flag, group. Normalise/denormalise round-trips within 1e-6.
- B4. Each registered parameter gets a dense `ParamIndex(u32)` in registration order. Runtime values live in one contiguous array of atomics indexed by `ParamIndex`; string IDs resolve to indices at prepare time only and are never looked up in `process`. `ProcessIo` exposes one per-block change stream of `(sample_offset, ParamIndex, value)` sorted by offset, stable for equal offsets, with no allocation when filled up to the capacity fixed at prepare time.
- B5. `trait Editor` lifecycle on the message thread, size constraints; no UI types (those live in L4) - use opaque handles. Layering: this crate is L1 and must not name `syrp-ui` (L4) types. It defines `NodeHandle`, a `Copy` newtype over a 64-bit generational key (index plus generation), which `syrp-ui`'s `ComponentId` converts to and from losslessly. It also defines `trait EditorHost` (object-safe: `root(&self) -> NodeHandle`, `as_any_mut(&mut self) -> &mut dyn Any`). The signature is `fn create(&mut self, host: &mut dyn EditorHost, params: &ParamHandles) -> Result<NodeHandle, EditorError>`. The L5 `syrp-plugin` runtime implements `EditorHost` for `syrp-ui`'s `Ui` and offers the typed accessor plugins use; this crate never sees `Ui`. The returned handle is a core node handle, never a compatibility-layer type (ADR 0003, D1).
- B6. `PluginDescriptor` (name, vendor, version, unique ids per format, categories) and state save/load traits with a versioned envelope.
- B7. Event I/O carrying `syrp-midi` messages and UMP.
- B8. Contract freeze as in WP-0.2-01.

## Crate setup
Create `crates/contracts/plugin-api/` with `Cargo.toml` (inherit `version`, `edition`, `rust-version`, `repository`, `authors`, `publish` from the workspace; `[lints] workspace = true`), `[package.metadata.syrp] layer = "L1"`, an `AGENTS.md` following `docs/agents/crate-agents-template.md`, and `src/lib.rs`. Workspace members are globbed, so no root edit is needed.
Licence: `license = "MIT OR Apache-2.0"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-0.2-06` first and `cargo syrp wp check WP-0.2-06` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

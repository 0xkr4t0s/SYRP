# WP-0.2-07: syrp-gfx-api: Graphics ops, DisplayList, Brush, Path, Image and Font handles, Renderer

## Purpose
The recorded drawing model between UI code and renderers (master plan §5.7).

## Data layout
Read `docs/design/0003-data-layout.md` (ADR 0003) first. Hot-path types in this crate have no public fields (D3): expose constructors, accessors, views, handles and iterators, so storage can change without a contract change.

## Behaviour
- B1. `DisplayList`: an ordered list of drawing ops: fill/stroke path, fill rect, draw image (with RectanglePlacement and resampling quality), draw glyph run, clip (rect, path, image mask), transform push/pop, transparency layers, blend modes. It is encoded as an op-tag stream with per-kind payload arrays, and shared arenas for path verbs, points, glyph runs and gradient stops, all addressed by index. Recording reuses capacity: after `clear()`, re-recording a list of the same shape performs no allocation. Ops are read back through an iterator of borrowed op views.
- B2. `Brush`: solid colour, linear, radial and two-point-conical gradients with stops (use `peniko` types or wrap them).
- B3. `Path` with move/line/quad/cubic/close, stroke parameters (width, joins, caps, dashes), and bounds. A path is stored as a verb array and a point array; when recorded into a `DisplayList` its verbs and points are appended to the list's arenas, not stored as a separate allocation.
- B4. Opaque `ImageHandle` and `FontHandle` ids, resolved by renderers.
- B5. `trait Renderer { fn render(&mut self, list: &DisplayList, target: &mut RenderTarget) -> Result<(), RenderError> }` with explicit scale factor on `RenderTarget`.
- B6. A `DisplayList` serialises to JSON (feature `serde`) for inspection and golden debugging, and exposes a content hash, equal for lists that record the same ops, used to skip re-rendering unchanged cached layers.
- B7. Contract freeze as in WP-0.2-01.

## Crate setup
Create `crates/contracts/gfx-api/` with `Cargo.toml` (inherit `version`, `edition`, `rust-version`, `repository`, `authors`, `publish` from the workspace; `[lints] workspace = true`), `[package.metadata.syrp] layer = "L1"`, an `AGENTS.md` following `docs/agents/crate-agents-template.md`, and `src/lib.rs`. Workspace members are globbed, so no root edit is needed.
Licence: `license = "MIT OR Apache-2.0"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-0.2-07` first and `cargo syrp wp check WP-0.2-07` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

# WP-0.2-09: Renderer spike: vello_cpu deterministic mode vs tiny-skia, golden comparator

## Purpose
Decide the reference CPU renderer (master plan §11, risk R2). Output is a decision record, not a product crate.

## Behaviour
- B1. Render the same 20 scenes (paths, gradients, text-like glyph outlines, images, clips, transparency layers, at scales 1x/1.25x/1.5x/2x) with `vello_cpu` and `tiny-skia` from a shared `DisplayList` (from `syrp-gfx-api`).
- B2. Measure determinism: same output bytes across 10 runs; and across x86_64 and aarch64 if CI provides both (otherwise record as open).
- B3. Measure speed per scene.
- B4. Implement a golden comparator: per-pixel tolerance, max differing pixel ratio, and a diff PNG.
- B5. Write `docs/design/0003-renderer-spike.md` with results and a recommendation.

## Crate setup
Create `crates/spikes/render-spike/` with `Cargo.toml` (inherit `version`, `edition`, `rust-version`, `repository`, `authors`, `publish` from the workspace; `[lints] workspace = true`), `[package.metadata.syrp] layer = "L6"`, an `AGENTS.md` following `docs/agents/crate-agents-template.md`, and `src/lib.rs`. Workspace members are globbed, so no root edit is needed.
Licence: `license = "AGPL-3.0-only OR BUSL-1.1"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-0.2-09` first and `cargo syrp wp check WP-0.2-09` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

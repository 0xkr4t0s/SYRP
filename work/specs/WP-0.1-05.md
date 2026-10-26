# WP-0.1-05: auricle-geom: Point, Rect, RectList, AffineTransform, BorderSize, Line, Justification, RectanglePlacement

## Purpose
2D geometry with JUCE-equivalent semantics for UI and graphics code (JUCE classes: Point, Rectangle, RectangleList, AffineTransform, BorderSize, Line, Parallelogram, Justification, RectanglePlacement; see the public JUCE class documentation for behaviour).

## Behaviour
- B1. `Point<T>` and `Rect<T>` generic over `i32` and `f32`/`f64` (a `Coord` trait). Rect is position plus size; negative sizes normalise to empty as JUCE documents.
- B2. Rect operations: intersection, union (smallest enclosing), contains (point and rect), reduced/expanded, removeFromTop/Bottom/Left/Right equivalents (`take_top` etc.) that mutate and return the removed slice, `with_*` builders, `to_nearest_int` with documented rounding.
- B3. `RectList` that keeps a list of non-overlapping rects: add, subtract, clip, bounds, contains; results are canonical (no overlaps, merged where possible).
- B4. `AffineTransform` with identity, translation, rotation (radians, optional pivot), scale, shear, composition order matching JUCE's documented `followedBy`, inversion (returns `None` when singular), and transforming points and rects (bounding box).
- B5. `BorderSize`, `Line` (length, angle, point along, intersection), `Parallelogram`.
- B6. `Justification` flags and `RectanglePlacement` (fit, fill, only-reduce, only-increase, centred and edge combinations) with `apply_to(source, dest) -> Rect` and `transform_to_fit`.
- B7. `kurbo` interop behind feature `kurbo` (`From`/`Into`).
- B8. Property tests (proptest) for algebraic laws: intersection is commutative, union contains both, transform followed by its inverse is identity within 1e-9.

## Crate setup
Create `crates/foundation/geom/` with `Cargo.toml` (inherit `version`, `edition`, `rust-version`, `repository`, `authors` from the workspace; `[lints] workspace = true`), `[package.metadata.auricle] layer = "L0"`, an `AGENTS.md` following `docs/agents/crate-agents-template.md`, and `src/lib.rs`. Workspace members are globbed, so no root edit is needed.
Licence: `license = "MIT OR Apache-2.0"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo auricle wp show WP-0.1-05` first and `cargo auricle wp check WP-0.1-05` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

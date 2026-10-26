# syrp-gfx-api

## Purpose
The L1 drawing contract (master plan §5.7, ADR 0003) between UI code and renderers. This first
increment holds the value types that ops are recorded from: `Path` (verb array plus point
array) with `PathView` and `PathEl`, `Stroke`/`StrokeRef` (width, `Join`, `Cap`, miter limit,
dashes), `Colour`, `GradientStop`, `Brush`/`BrushRef`/`BrushKind` (solid, linear, radial,
two-point conical), and opaque `ImageHandle`/`FontHandle` ids. Covers JUCE's Path,
PathStrokeType, Colour, ColourGradient and FillType.

## Ledger rows
juce_graphics/Path, juce_graphics/PathStrokeType, juce_graphics/Colour,
juce_graphics/ColourGradient, juce_graphics/FillType (shared with `syrp-gfx`)

## Invariants
- Contract types have no public fields (ADR 0003 D3); the `b7_` test scans the sources.
  Owned types (`Path`, `Stroke`, `Brush`) lend borrowed views; a display list will hand back
  the same views over its arenas, so renderers never see the owned types.
  The view accessor is always `as_view()`.
- Colours and brushes are this crate's own types, never peniko re-exports, so the frozen
  contract pins no renderer dependency; renderer crates convert with plain functions.
- Every subpath in `Path::verbs` starts with `MoveTo` (a drawing verb with no open subpath
  inserts one at the current point). `points.len()` is the sum of `PathVerb::point_count`.
- `Path::bounds` covers control points (conservative for curves) and skips NaN points.
- Widths, miter limits, radii and dash lengths are `>= 0`; negative, NaN and `-0.0` become
  `+0.0` so equal inputs have equal bits. Gradient stop offsets are clamped to `[0, 1]` and
  stops are sorted stably by offset.
- `Colour` is straight-alpha ARGB8. Coordinates are `f32` user units, y down.

## Thread affinity
Plain data: all types are `Send + Sync`; no `MessageThread` needed. Building a `Path` or a
gradient `Brush` allocates; do it outside RT or per-frame hot loops and reuse.

## Stability
contract (frozen in `contracts/syrp-gfx-api.public-api.txt` once WP-0.2-07 is final).

## How to test
```sh
cargo syrp test --crate syrp-gfx-api
```

## Known gaps
WP-0.2-07 is not complete. Still to come in the next increment: `DisplayList` (op-tag stream,
per-kind payloads, shared arenas, allocation-free re-recording, borrowed op views),
`Glyph` runs, `BlendMode`, `ResamplingQuality`, `Renderer`/`RenderTarget`/`RenderError`
(with an `errors/gfx.toml` domain), the seedless content hash and the `serde` feature.

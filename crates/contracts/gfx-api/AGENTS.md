# syrp-gfx-api

## Purpose
The L1 drawing contract (master plan §5.7, ADR 0003) between UI code and renderers:
`DisplayList` (recorded ops read back as borrowed `Op` views, `Glyph` runs, `BlendMode`,
`ResamplingQuality`, `content_hash`, `serde` JSON), `Renderer`, `RenderTarget` and
`RenderError` (`errors/gfx.toml`), and the value types that ops are recorded from: `Path` (verb array plus point
array) with `PathView` and `PathEl`, `Stroke`/`StrokeRef` (width, `Join`, `Cap`, miter limit,
dashes), `Colour`, `GradientStop`, `Brush`/`BrushRef`/`BrushKind` (solid, linear, radial,
two-point conical), and opaque `ImageHandle`/`FontHandle` ids. Covers JUCE's Path,
PathStrokeType, Colour, ColourGradient, FillType, the recording side of Graphics and
LowLevelGraphicsContext's role.

## Ledger rows
juce_graphics/Path, juce_graphics/PathStrokeType, juce_graphics/Colour,
juce_graphics/ColourGradient, juce_graphics/FillType, juce_graphics/Graphics (shared with `syrp-gfx`)

## Invariants
- Contract types have no public fields (ADR 0003 D3); the `b7_` test scans the sources.
  Owned types (`Path`, `Stroke`, `Brush`) lend borrowed views; a display list hands back
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
- `DisplayList`: `ops` holds `(tag, index into that kind's payload array)`; paths, stops,
  glyphs and dashes are copied into shared arenas addressed by `u32` spans. `clear()` keeps
  every capacity, and re-recording the same shape must not allocate (counting-allocator test).
  `Push*` ops open scopes closed innermost-first by `Pop`; `pop()` with none open records
  nothing. Opacity is clamped to `[0, 1]`, glyph size to `>= 0`. Rects are stored as `f32`.
- `content_hash` and the JSON both walk the public `Op` views (never the Vecs): 64-bit FNV-1a
  over little-endian bytes, floats by bit pattern, slices length-prefixed. Op codes and enum
  discriminant order (`BlendMode`, `FillRule`, `PathVerb`, `Join`, `Cap`, `ResamplingQuality`)
  are part of the encoding: append variants only, never reorder. The `b6_` golden pins it.
- `RenderTarget`: physical `u32` size plus a finite positive `f64` scale; `from_logical`
  rounds each side up but snaps products within `1e-9` relative of a whole number. CPU buffers
  are RGBA8 premultiplied, tightly packed, top row first, cleared on create and resize.
- Views have by-value accessors returning `&'a` data; op iterators are `+ use<'a>`.
- `Colour` is straight-alpha ARGB8. Coordinates are `f32` user units, y down.

## Thread affinity
Plain data: all types are `Send + Sync`; no `MessageThread` needed. Building a `Path` or a
gradient `Brush` allocates; reuse them. Recording into a cleared, warmed `DisplayList` does
not allocate. `Renderer::render` runs wherever the renderer lives (usually the message thread
or a render thread); nothing here is RT-safe.

## Stability
contract (frozen in `contracts/syrp-gfx-api.public-api.txt`, committed by the PR manager).

## How to test
```sh
cargo syrp test --crate syrp-gfx-api   # tests enable the `serde` feature
```

## Known gaps
None for the contract. Renderers (`syrp-render-cpu`, `syrp-render-gpu`) and the `Graphics`
front-end (`syrp-gfx`) are separate WPs; image filters (DropShadow, Glow) arrive with them.

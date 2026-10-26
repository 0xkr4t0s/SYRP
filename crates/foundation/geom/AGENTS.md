# syrp-geom

## Purpose
2D geometry with JUCE-equivalent semantics, the L0 base for graphics and UI crates. Covers the
JUCE geometry classes Point, Rectangle (as `Rect`), RectangleList (as `RectList`),
AffineTransform, BorderSize, Line and Parallelogram, plus optional `kurbo` conversions;
Justification and RectanglePlacement follow (see Known gaps). Behaviour comes from the public
JUCE class documentation only.

## Ledger rows
juce_graphics/Point, juce_graphics/Rectangle, juce_graphics/RectangleList,
juce_graphics/AffineTransform, juce_graphics/BorderSize, juce_graphics/Line,
juce_graphics/Parallelogram

## Invariants
- `Rect` width and height are never negative: every constructor and builder clamps negative
  or NaN sizes to zero. Fields are private so the invariant cannot be bypassed.
- Rect edges: left/top inside, right/bottom outside (`contains_point`).
- `union` ignores empty rectangles; `intersection` returns `Rect::default()` when disjoint.
- `AffineTransform` is `f64`. `a.followed_by(b)` applies `a` first; builder methods
  (`translated`) append. `inverted` returns `None` when singular.
- Angles are radians; with y down, positive rotation is clockwise on screen.
- Float to integer conversions round to nearest, halves away from zero, saturating, NaN to 0;
  `transform_rect` on integer rects floors/ceils outwards. Float and integer paths round
  differently on purpose; test both (`Line` points round to nearest for `i32` only).
- `RectList` is canonical after every call: no empty rects, no overlaps, no pair sharing a
  whole edge left unmerged. Rect order is unspecified (so it has no `PartialEq`).
- `Line::angle` is clockwise from straight up (y down). `Line::intersection` treats parallel
  (including collinear) segments as not meeting.
- No default dependencies. Feature `kurbo` adds `From` impls for the `f64` types; the crate
  dev-depends on itself with that feature so plain `cargo test` covers B7.

## Thread affinity
All value types are plain `Copy`: any thread, RT-safe (no allocation, no locks). `RectList`
owns a `Vec` and allocates on `add`/`subtract`: any thread, not for the audio thread.

## Stability
experimental

## How to test
```sh
cargo syrp test --crate syrp-geom
PROPTEST_CASES=20000 cargo test -p syrp-geom --test properties   # deeper property run
```

## Known gaps
Remaining for WP-0.1-05: `Justification` and `RectanglePlacement` (B6). Smaller API left out
to keep increments reviewable: `AffineTransform` chaining builders other than `translated`
(`rotated`, `scaled`, `sheared`), pivot scale, `Point` dot product, `Rect::from_points` /
`with_position`, `to_nearest_int_edges`, `RectList` offset/transform helpers, `Line`
nearest-point queries.

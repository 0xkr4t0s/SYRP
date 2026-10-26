# syrp-geom

## Purpose
2D geometry with JUCE-equivalent semantics, the L0 base for graphics and UI crates. Covers the
JUCE geometry classes Point and Rectangle (as `Rect`) so far; AffineTransform, RectangleList,
BorderSize, Line, Parallelogram, Justification and RectanglePlacement follow (see Known gaps).
Behaviour comes from the public JUCE class documentation only.

## Ledger rows
juce_graphics/Point, juce_graphics/Rectangle

## Invariants
- `Rect` width and height are never negative: every constructor and builder clamps negative
  or NaN sizes to zero. Fields are private so the invariant cannot be bypassed.
- Rect edges: left/top inside, right/bottom outside (`contains_point`).
- `union` ignores empty rectangles; `intersection` returns `Rect::default()` when disjoint.
- Float to integer conversions round to nearest, halves away from zero, saturating, NaN to 0.
- No dependencies (kurbo interop will be an optional feature).

## Thread affinity
Plain `Copy` value types: any thread, RT-safe (no allocation, no locks).

## Stability
experimental

## How to test
```sh
cargo syrp test --crate syrp-geom
PROPTEST_CASES=20000 cargo test -p syrp-geom --test properties   # deeper property run
```

## Known gaps
Remaining for WP-0.1-05: `AffineTransform` (B4, next increment, with its B8 property test),
`RectList` (B3), `BorderSize`, `Line`, `Parallelogram` (B5),
`Justification` and `RectanglePlacement` (B6), `kurbo` interop feature (B7).
Smaller API left out to keep increments reviewable: `Point` dot product,
`Rect::from_points`/`with_position`, `to_nearest_int_edges`.

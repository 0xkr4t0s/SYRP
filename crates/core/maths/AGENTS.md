# syrp-maths

## Purpose
The L2 numeric layer (master plan §5.2): JUCE's Range (`Range`), NormalisableRange
(`NormalisableRange`); later increments add the numeric helper functions (`jmap`, `jlimit`,
`isWithin`, `approximatelyEqual`, `nextPowerOfTwo`, `isPowerOfTwo`, `roundToInt`,
`mapFromLog10`, `mapToLog10`), `MathConstants`, Random and StatisticsAccumulator.
BigInteger and Expression are a later WP. Behaviour comes from the public JUCE class
documentation only.

## Ledger rows
juce_core/Range, juce_core/NormalisableRange, juce_core/Random, juce_core/StatisticsAccumulator

## Invariants
- `Range`: `start <= end` always (`new` orders its ends); half-open, so `contains(end)` is
  false; empty means `start == end` and an empty range intersects nothing.
- `NormalisableRange`: fields private; constructors return `MathsError::InvalidRange`
  (`SYRP-MATHS-0001`) unless `start < end`, `interval >= 0`, `skew > 0`, all finite.
  `convert_to_0to1` and `convert_from_0to1` clamp and are inverse to ~1e-9 (`f64`).
  `convert_from_0to1` does not snap. Skew maps normalised `p` to `p^(1/skew)` (mirrored about
  0.5 when symmetric): this crate's own formula, not yet compared with JUCE.
- Custom mappings are `fn` pointers (`MapFn`), so the range stays `Copy` and never allocates.
- Float maths runs in `f64` internally (`Float::to_f64`/`from_f64`).

## Thread affinity
Everything is plain `Copy` data or a pure function: any thread, no allocation, no locks.
`RtSafe` impls arrive with B6 in a later increment.

## Stability
experimental

## How to test
```sh
cargo syrp test --crate syrp-maths
```

## Known gaps
WP-1.1-11 later increments: numeric helpers and `MathConstants` (B3), `Random` (B4),
`StatisticsAccumulator` (B5), `RtSafe` impls and no-allocation tests (B6). `BigInteger` and `Expression` are a later WP.

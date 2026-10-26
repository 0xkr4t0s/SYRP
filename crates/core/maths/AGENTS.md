# syrp-maths

## Purpose
The L2 numeric layer (master plan §5.2): JUCE's Range (`Range`), NormalisableRange
(`NormalisableRange`), the numeric helper functions (`jmap`, `jlimit`, `isWithin`,
`approximatelyEqual`, `nextPowerOfTwo`, `isPowerOfTwo`, `roundToInt`, `mapFromLog10`,
`mapToLog10`), `MathConstants`, Random (`Random`, `system_random`) and
StatisticsAccumulator. BigInteger and Expression are a later WP. Behaviour comes from the public JUCE class
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
  Non-capturing closures coerce; capturing ones cannot be used.
- NaN in gives NaN out for `convert_to_0to1`, `convert_from_0to1` and `snap_to_legal_value`.
- `round_to_int` rounds halves away from zero, saturates, NaN to 0.
- `Random` is SplitMix64; its output for a seed is frozen (test pins it). Never change the
  algorithm or constants: saved sequences and tests depend on it.
- `StatisticsAccumulator` uses Welford in `f64`; variance is the population variance.
- Float maths runs in `f64` internally (`Float::to_f64`/`from_f64`).

## Thread affinity
`Range`, `NormalisableRange`, `Random` and `StatisticsAccumulator` are `RtSafe`: any thread,
no allocation, no locks (custom `MapFn`s must be RT-safe themselves). `Random::with_entropy`
and `system_random` (a `Mutex`) are not for the audio thread.

## Stability
experimental

## How to test
```sh
cargo syrp test --crate syrp-maths
```
`tests/no_alloc.rs` installs `syrp_rt::GuardedAllocator`; keep it its own test binary.

## Known gaps
All WP-1.1-11 behaviours (B1-B6) are covered. `BigInteger` and `Expression` are a later WP.

## Open questions
- Are capturing closures ever needed for custom `NormalisableRange` mappings? JUCE accepts
  capturing lambdas; `MapFn` does not, so the differential harness has no direct equivalent
  for such a JUCE range. If a real need appears, add a separate non-`RtSafe` type rather than
  changing `NormalisableRange`. `BigInteger` and `Expression` are a later WP.

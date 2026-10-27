# syrp-maths

## Purpose
The L2 numeric layer (master plan §5.2): JUCE's Range (`Range`), NormalisableRange
(`NormalisableRange`), the numeric helper functions (`jmap`, `jlimit`, `isWithin`,
`approximatelyEqual`, `nextPowerOfTwo`, `isPowerOfTwo`, `roundToInt`, `mapFromLog10`,
`mapToLog10`), `MathConstants`, Random (`Random`, `system_random`) and
StatisticsAccumulator, and BigInteger (`BigInteger`, WP-1.1-16). Expression is a later WP.
Behaviour comes from the public JUCE class documentation only.

## Ledger rows
juce_core/Range, juce_core/NormalisableRange, juce_core/Random, juce_core/StatisticsAccumulator,
juce_core/BigInteger

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
- `BigInteger`: sign and magnitude, private fields; the magnitude is little-endian `u64`
  limbs with no zero limb on top, and zero is never negative (`from_parts`/`renormalise`
  restore both after every edit). Our own limbs, no bignum dependency, so no third-party type
  can reach the public API. Division truncates toward zero; the remainder has the dividend's
  sign. Bit operations, shifts and `&`/`|`/`^` act on the magnitude; `>>` keeps the sign.
  Bit indices are `u32`.
- `BigInteger` text: bases 2, 8, 10 and 16 only; anything else, empty text or a non-digit is
  `MathsError::InvalidBaseOrDigit` (`SYRP-MATHS-0002`). `to_le_bytes` drops high zero bytes
  and the sign; `from_le_bytes` accepts any bytes and is never negative.
- `BigInteger` number theory: results are in `0..|modulus|`; modulus signs are ignored. Not
  constant-time (fine for `Primes`/`RSAKey` key generation, not for secret-dependent timing).
  `montgomeryMultiplication` is left out: its documented contract is not specified enough.
- `parity/scenarios/maths/big_integer/` (owned with this crate) holds inputs only;
  `tests/big_integer_scenarios.rs` checks they cover every op in `scenario.schema.json`.

## Thread affinity
`Range`, `NormalisableRange`, `Random` and `StatisticsAccumulator` are `RtSafe`: any thread,
no allocation, no locks (custom `MapFn`s must be RT-safe themselves). `BigInteger` allocates
and is not `RtSafe`; it is any-thread (`Send + Sync`). `Random::with_entropy`
and `system_random` (a `Mutex`) are not for the audio thread.

## Stability
experimental

## How to test
```sh
cargo syrp test --crate syrp-maths
```
`tests/big_integer*.rs` check against `i128`/`u128` for small
values and algebraic identities for large ones. `tests/no_alloc.rs` installs
`syrp_rt::GuardedAllocator`; keep it its own test binary.

## Known gaps
All WP-1.1-11 behaviours (B1-B6) are covered. All WP-1.1-16 behaviours (B1-B6) are covered;
the scenarios have no reference outputs until the harness runs them. `Expression` is a later
WP. Multiplication is schoolbook (no Karatsuba); division is long division.

## Open questions
- Are capturing closures ever needed for custom `NormalisableRange` mappings? JUCE accepts
  capturing lambdas; `MapFn` does not, so the differential harness has no direct equivalent
  for such a JUCE range. If a real need appears, add a separate non-`RtSafe` type rather than
  changing `NormalisableRange`.
- `BigInteger` division by zero: `/`, `%` and `div_rem` return zero (`checked_div_rem` returns
  `None`). Syrp's choice, pinned by `b2_division_by_zero_returns_zero_syrp_choice`; the JUCE
  documentation does not define it.
- `BigInteger` `&`, `|`, `^`: magnitudes combined, result keeps the left operand's sign.
  Syrp's choice, pinned by `b3_bitwise_keeps_left_sign_syrp_choice`; compare with the harness.
- `bit_range_as_u32` and `set_bit_range_as_u32` treat a count above 32 as 32 (Syrp's choice).
- `BigInteger::to_string_radix`: hex is lowercase, the `-` goes before the zero padding and is
  not counted in `minimum_digits`, and an unsupported base is an error (Syrp's choices; the
  JUCE documentation does not say). `to_le_bytes` of zero is empty.
- `BigInteger::exponent_modulo`: a zero modulus gives zero and a negative exponent raises the
  modular inverse (zero when there is none); `inverse_modulo` with modulus 1 is `Some(0)`.
  Syrp's choices, pinned by `b5_exponent_modulo_syrp_choices` and
  `b5_inverse_and_gcd_edge_cases`; the scenarios use positive moduli only.

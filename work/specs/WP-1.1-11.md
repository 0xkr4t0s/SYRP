# WP-1.1-11: syrp-maths: Range, NormalisableRange, Random, StatisticsAccumulator and numeric helpers

## Purpose
The numeric layer from master plan §5.2 (L2 Core) and Wave 1.1 item 11. `NormalisableRange` is what parameters (`ParamSpec`, §5.8) and sliders map through, and `syrp-audio-basics` may use these types later. Delivery: 2 PRs (B1–B3, then B4–B6). `BigInteger` and `Expression` are a later WP in this crate.

## Behaviour
- B1. `Range<T>` for integer and float `T`: `new` (orders its ends), `start`, `end`, `length`, `is_empty`, `contains`, `clip_value`, `intersects`, `get_intersection_with`, `get_union_with`, `expanded`, `moved_to_start_at`, `constrain_range`, as the `Range` documentation describes (an empty range is `start == end`; `contains` is half-open).
- B2. `NormalisableRange<T>` (`f32` and `f64`): start, end, interval, skew, symmetric-skew flag. `convert_to_0to1` and `convert_from_0to1` are inverse within tolerance; `snap_to_legal_value` rounds to the interval and clamps. `with_centre(start, end, centre)` sets the skew so that 0.5 maps to `centre`. Custom mapping functions (from-0-to-1, to-0-to-1, snap) replace the built-in ones when given, as the class documentation describes. Write the skew mapping from the documented behaviour (skew 1 is linear, below 1 spends more of the 0–1 range at the low end); it is not required to be bit-identical to JUCE until the harness has compared it.
- B3. Helpers: `map_range`, `map_to_0to1`, `map_from_log10`, `map_to_log10`, `limit`, `is_within`, `approximately_equal` (relative and absolute tolerance), `next_power_of_two`, `is_power_of_two`, `round_to_int`, `MathConstants`-style `pi`, `two_pi`, `half_pi`, `euler`, `sqrt2` for `f32` and `f64`. Each has a doc example.
- B4. `Random`: a seedable generator with `new(seed)`, `with_entropy()` (seeded from the OS), `next_i32`, `next_i32_below(n)`, `next_i64`, `next_bool`, `next_f32`/`next_f64` in [0, 1), `fill_bytes`, and a `system_random()` shared instance behind a lock. The algorithm is your own choice, documented, and stable across platforms and releases (a test pins the first values for a fixed seed). Same seed, same sequence; sequences are not expected to match JUCE's.
- B5. `StatisticsAccumulator<T>`: `add`, `average`, `variance`, `standard_deviation`, `min`, `max`, `count`, using a numerically stable update (Welford or equivalent); tested on sequences where a naive sum loses precision.
- B6. No allocation in any B1–B3 or B5 call and in `Random` methods other than `with_entropy`; types used on the audio thread (`Range`, `NormalisableRange` without custom functions, `Random`) implement `syrp_rt::RtSafe` (this adds `syrp-rt`, an L0 crate, to the dependencies; master plan §5.3 lists it).

## Crate setup
Create `crates/core/maths/` with `Cargo.toml` (inherit `version`, `edition`, `rust-version`, `repository`, `authors`, `publish` from the workspace; `[lints] workspace = true`), `[package.metadata.syrp] layer = "L2"`, an `AGENTS.md` following `docs/agents/crate-agents-template.md`, and `src/lib.rs`. Workspace members are globbed, so no root edit is needed.
Licence: `license = "AGPL-3.0-only OR BUSL-1.1"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-1.1-11` first and `cargo syrp wp check WP-1.1-11` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

# WP-1.1-16: syrp-maths: BigInteger, arbitrary-precision signed integers with bit, string and modular operations

## Purpose
`BigInteger` from master plan §5.2 (`syrp-maths`: "BigInteger, Expression, ..."), which WP-1.1-11 left to a later WP. Phase 1's exit criteria include a BigInteger differential suite against the harness, and `syrp-crypto` (WP-1.2-09) builds `Primes` and `RSAKey` on it, so its public API never exposes a third-party big-number type. Numbered after the Wave 1.1 list because the list has no item of its own for it. Delivery: 2 PRs, each at most 1,200 changed lines (B1–B3, then B4–B6).

## Behaviour
- B1. `BigInteger`: an arbitrary-size signed integer (sign and magnitude, as the `BigInteger` documentation describes; zero is never negative). `Default` is zero; `From<i32>`, `From<u32>`, `From<i64>` and `From<u64>`; `to_i64() -> Option<i64>` and `to_u64() -> Option<u64>` (checked, unlike JUCE's truncating `toInt64`, documented); `is_zero`, `is_one`, `is_negative`, `set_negative`, `negate`, `abs`. `Clone`, `Eq`, `Ord` (numeric order), `Hash` and `Debug`; no public fields. The representation is private: you may build it on `num-bigint` 0.5 (MIT OR Apache-2.0) or on your own limbs, but no third-party type appears in the public API.
- B2. Arithmetic: `Add`, `Sub`, `Mul`, `Div`, `Rem` and their `*Assign` forms, by value and by reference, `Neg`, and `div_rem(&divisor) -> (quotient, remainder)`. Division truncates toward zero and the remainder takes the dividend's sign; if the documentation says otherwise, follow it. Division by zero is not a panic: the operators return zero and `checked_div_rem` returns `None`; document the operator choice, pin it with a `b2_..._syrp_choice` test and list it under "Open questions" in the crate `AGENTS.md`.
- B3. Bits, on the magnitude as the documentation describes: `bit(n)`, `set_bit(n, bool)`, `clear_bit`, `flip_bit`, `set_range(start, count, bool)`, `insert_bit`, `bit_range(start, count) -> BigInteger`, `bit_range_as_u32(start, count)` (at most 32 bits) and `set_bit_range_as_u32`, `highest_bit() -> Option<u32>`, `find_next_set_bit(from)`, `find_next_clear_bit(from)`, `count_set_bits`, `Shl`/`Shr` by `u32`, and `BitAnd`, `BitOr`, `BitXor`.
- B4. Text and bytes: `to_string_radix(base, minimum_digits)` and `parse(text, base) -> Result<BigInteger, MathsError>` for bases 2, 8, 10 and 16 (a leading `-` for negative values; other bases or invalid digits are the new coded error `SYRP-MATHS-0002`); `Display` is base 10 and `LowerHex`/`UpperHex` are base 16. `to_le_bytes()` and `from_le_bytes(&[u8])` give the magnitude as little-endian bytes, the form the `toMemoryBlock`/`loadFromMemoryBlock` documentation describes; the sign is not stored, as documented.
- B5. Number theory for `syrp-crypto`: `exponent_modulo(&exponent, &modulus)`, `inverse_modulo(&modulus) -> Option<BigInteger>` (`None` when there is no inverse), `find_greatest_common_divisor(&other)`, and `montgomery_multiplication` only if the documentation describes its contract; otherwise leave it out and say so in the rustdoc. Results are always in `0..modulus` for a positive modulus.
- B6. Tests and the differential suite: proptests check B2–B5 against `i128` arithmetic for values that fit and algebraic identities for large ones (`(a / b) * b + a % b == a`, `(a << n) >> n == a`, parse of `to_string_radix` is the identity in every base, `from_le_bytes(to_le_bytes(x)) == abs(x)`, `a * inverse_modulo(a, m) % m == 1`). Add scenario files under `parity/scenarios/maths/big_integer/` (the WP-0.1-11 format) covering every operation, with zero, negative operands and values above 2^1024. Scenario files hold inputs only; expected outputs come solely from harness runs. No allocation guarantees are claimed (arithmetic allocates); `BigInteger` is not `RtSafe`.

## Not in this WP
`Expression` (a later WP in this crate), constant-time arithmetic, and floating-point conversion beyond what the documentation describes.

## Crate setup
The whole crate `crates/core/maths/`, which WP-1.1-11 creates; this WP starts after WP-1.1-11 merges and may edit any file there, including `Cargo.toml` and `AGENTS.md` (add this WP's ledger rows, invariants and gaps). No new crate.
Licence: `license = "AGPL-3.0-only OR BUSL-1.1"`.
This WP also owns `parity/scenarios/maths/big_integer/` for B6.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-1.1-16` first and `cargo syrp wp check WP-1.1-16` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.

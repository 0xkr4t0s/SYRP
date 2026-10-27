//! B1-B3: `BigInteger` construction, arithmetic and bit operations.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use proptest::prelude::*;
use syrp_maths::BigInteger;

fn big(v: i128) -> BigInteger {
    BigInteger::from(v)
}

fn hash_of(v: &BigInteger) -> u64 {
    let mut h = DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}

/// `2^n`, built with multiplication only.
fn pow2(n: u32) -> BigInteger {
    let mut out = BigInteger::from(1_u128 << (n % 64));
    for _ in 0..n / 64 {
        out *= BigInteger::from(1_u128 << 64);
    }
    out
}

/// Builds a non-negative value from little-endian limbs with multiplication and addition.
fn from_limbs(limbs: &[u64]) -> BigInteger {
    let mut out = BigInteger::default();
    for (i, &l) in limbs.iter().enumerate() {
        out += BigInteger::from(l) * pow2(64 * i as u32);
    }
    out
}

/// Limbs biased toward the edge patterns that stress carries and division corrections.
fn limb() -> impl Strategy<Value = u64> {
    prop_oneof![
        Just(0_u64),
        Just(1),
        Just(u64::MAX),
        Just(1 << 63),
        Just((1 << 63) - 1),
        Just(u64::MAX - 1),
        any::<u64>(),
    ]
}

fn large() -> impl Strategy<Value = BigInteger> {
    (prop::collection::vec(limb(), 0..20), any::<bool>()).prop_map(|(l, neg)| {
        let mut v = from_limbs(&l);
        v.set_negative(neg);
        v
    })
}

#[test]
fn b1_default_is_zero_and_zero_is_never_negative() {
    let z = BigInteger::default();
    assert!(z.is_zero() && !z.is_negative() && !z.is_one());
    let mut n = BigInteger::from(0);
    n.set_negative(true);
    n.negate();
    assert!(!n.is_negative());
    assert_eq!(-z.clone(), z);
    assert_eq!(&BigInteger::from(5) - &BigInteger::from(5), z);
    assert_eq!(hash_of(&(big(-3) + big(3))), hash_of(&z));
}

#[test]
fn b1_conversions_round_trip_and_are_checked() {
    assert_eq!(
        BigInteger::from(i32::MIN).to_i64(),
        Some(i64::from(i32::MIN))
    );
    assert_eq!(
        BigInteger::from(u32::MAX).to_u64(),
        Some(u64::from(u32::MAX))
    );
    assert_eq!(BigInteger::from(i64::MIN).to_i64(), Some(i64::MIN));
    assert_eq!(BigInteger::from(u64::MAX).to_u64(), Some(u64::MAX));
    assert_eq!(BigInteger::from(u64::MAX).to_i64(), None);
    assert_eq!(BigInteger::from(-1).to_u64(), None);
    assert_eq!(pow2(64).to_u64(), None);
    assert_eq!(BigInteger::from(i128::MIN).to_i128(), Some(i128::MIN));
    assert_eq!(pow2(128).to_u128(), None);
    assert_eq!((-pow2(127) - big(1)).to_i128(), None);
}

#[test]
fn b1_sign_helpers() {
    let mut v = big(12);
    v.set_negative(true);
    assert!(v.is_negative());
    assert_eq!(v.abs(), big(12));
    v.negate();
    assert_eq!(v, big(12));
    assert!(big(1).is_one() && !big(-1).is_one() && !big(2).is_one());
}

#[test]
fn b1_order_is_numeric() {
    let mut v = vec![big(5), -pow2(200), big(0), big(-3), pow2(200), big(-4)];
    v.sort();
    let expect = vec![-pow2(200), big(-4), big(-3), big(0), big(5), pow2(200)];
    assert_eq!(v, expect);
}

#[test]
fn b1_debug_shows_sign_and_hex_magnitude() {
    assert_eq!(format!("{:?}", big(-255)), "BigInteger(-0xff)");
    assert_eq!(format!("{:?}", big(0)), "BigInteger(0x0)");
    assert_eq!(format!("{:?}", pow2(64)), "BigInteger(0x10000000000000000)");
}

#[test]
fn b2_division_truncates_and_remainder_takes_dividend_sign() {
    for (a, b) in [
        (7, 2),
        (-7, 2),
        (7, -2),
        (-7, -2),
        (6, 3),
        (-6, 3),
        (1, 5),
        (-1, 5),
    ] {
        let (q, r) = big(a).div_rem(&big(b));
        assert_eq!((q, r), (big(a / b), big(a % b)), "{a} / {b}");
    }
}

#[test]
fn b2_division_by_zero_returns_zero_syrp_choice() {
    let zero = BigInteger::default();
    let a = big(-42) * pow2(300);
    assert_eq!(&a / &zero, zero);
    assert_eq!(&a % &zero, zero);
    assert_eq!(a.div_rem(&zero), (zero.clone(), zero.clone()));
    assert_eq!(a.checked_div_rem(&zero), None);
    let mut b = a.clone();
    b /= &zero;
    assert_eq!(b, zero);
}

#[test]
fn b2_operator_forms_agree() {
    let (a, b) = (big(-1234) * pow2(100), big(987_654_321));
    let r = &a + &b;
    assert_eq!(a.clone() + b.clone(), r);
    assert_eq!(a.clone() + &b, r);
    assert_eq!(&a + b.clone(), r);
    let mut c = a.clone();
    c += &b;
    assert_eq!(c, r);
    let mut c = a.clone();
    c -= b.clone();
    assert_eq!(c, &a - &b);
    let mut c = a.clone();
    c *= &b;
    assert_eq!(c, &a * &b);
    let mut c = a.clone();
    c %= &b;
    assert_eq!(c, &a % &b);
    assert_eq!(-&a, -(a.clone()));
}

#[test]
fn b2_large_known_values() {
    // (2^128 - 1)^2 = 2^256 - 2^129 + 1
    let m = BigInteger::from(u128::MAX);
    let square = &m * &m;
    let expect = pow2(256) - pow2(129) + big(1);
    assert_eq!(square, expect);
    assert_eq!(&square / &m, m);
    assert!((&square % &m).is_zero());
    assert_eq!((&square + big(5)) % &m, big(5));
}

#[test]
fn b2_division_quotient_estimate_correction() {
    // An input whose first quotient-digit estimate is one too large even after the two-limb
    // refinement, so the division has to add the divisor back (found by search).
    let a = from_limbs(&[
        0,
        0x12a3_c54b_daeb_22a5,
        u64::MAX - 1,
        0x7fff_ffff_ffff_ffff,
    ]);
    let b = from_limbs(&[0x7fff_ffff_ffff_ffff, 0, u64::MAX - 1]);
    let (q, r) = a.div_rem(&b);
    assert_eq!(q, BigInteger::from(1_u64 << 63));
    assert_eq!(
        r,
        from_limbs(&[0x8000_0000_0000_0000, 0xd2a3_c54b_daeb_22a5, u64::MAX - 2])
    );
}

proptest! {
    #[test]
    fn b2_arithmetic_matches_i128(a in any::<i64>(), b in any::<i64>(), c in any::<i128>(), d in any::<i128>()) {
        let (a, b) = (i128::from(a), i128::from(b));
        prop_assert_eq!(big(a) + big(b), big(a + b));
        prop_assert_eq!(big(a) - big(b), big(a - b));
        prop_assert_eq!(big(a) * big(b), big(a * b));
        prop_assert_eq!((big(c) - big(d)).to_i128(), c.checked_sub(d));
        prop_assert_eq!((big(c) + big(d)).to_i128(), c.checked_add(d));
        if let (Some(q), Some(r)) = (c.checked_div(d), c.checked_rem(d)) {
            prop_assert_eq!(big(c).div_rem(&big(d)), (big(q), big(r)));
        }
        prop_assert_eq!(big(c).cmp(&big(d)), c.cmp(&d));
        prop_assert_eq!(big(c) == big(d), c == d);
    }

    #[test]
    fn b2_division_identity_holds_for_large_values(a in large(), b in large()) {
        if let Some((q, r)) = a.checked_div_rem(&b) {
            prop_assert_eq!(&(&q * &b) + &r, a.clone());
            prop_assert!(r.abs() < b.abs());
            prop_assert!(r.is_zero() || r.is_negative() == a.is_negative());
        } else {
            prop_assert!(b.is_zero());
        }
    }

    #[test]
    fn b2_ring_identities_hold_for_large_values(a in large(), b in large(), c in large()) {
        prop_assert_eq!(&(&a + &b) - &b, a.clone());
        prop_assert_eq!(&a * &(&b + &c), &(&a * &b) + &(&a * &c));
        prop_assert_eq!(&(&a + &b) * &(&a - &b), &(&a * &a) - &(&b * &b));
        if !b.is_zero() {
            prop_assert_eq!(&(&a * &b) / &b, a.clone());
        }
    }
}

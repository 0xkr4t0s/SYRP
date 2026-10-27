//! B5: `BigInteger` modular exponentiation, modular inverse and greatest common divisor.

use proptest::prelude::*;
use syrp_maths::BigInteger;

fn big(v: i128) -> BigInteger {
    BigInteger::from(v)
}

/// The Mersenne prime 2^1279 - 1, above 2^1024.
fn m1279() -> BigInteger {
    (big(1) << 1279) - big(1)
}

fn large() -> impl Strategy<Value = BigInteger> {
    (prop::collection::vec(any::<u8>(), 1..170), any::<bool>()).prop_map(|(b, neg)| {
        let mut v = BigInteger::from_le_bytes(&b);
        v.set_negative(neg);
        v
    })
}

fn pow_mod_u128(mut b: u128, mut e: u128, m: u128) -> u128 {
    let mut r = 1 % m;
    b %= m;
    while e > 0 {
        if e & 1 == 1 {
            r = r * b % m;
        }
        b = b * b % m;
        e >>= 1;
    }
    r
}

fn gcd_u128(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

#[test]
fn b5_exponent_modulo_edge_cases() {
    let m = big(1000);
    assert_eq!(big(7).exponent_modulo(&big(0), &m), big(1));
    assert_eq!(big(0).exponent_modulo(&big(0), &m), big(1));
    assert_eq!(big(0).exponent_modulo(&big(5), &m), big(0));
    assert_eq!(big(7).exponent_modulo(&big(5), &big(1)), big(0));
    assert_eq!(big(-3).exponent_modulo(&big(3), &big(10)), big(3)); // -27 mod 10
    assert_eq!(big(3).exponent_modulo(&big(2), &big(-10)), big(9)); // modulus sign ignored
}

#[test]
fn b5_exponent_modulo_syrp_choices() {
    // Zero modulus gives zero; a negative exponent raises the inverse, or gives zero.
    assert_eq!(big(3).exponent_modulo(&big(4), &big(0)), big(0));
    assert_eq!(big(3).exponent_modulo(&big(-1), &big(11)), big(4));
    assert_eq!(big(3).exponent_modulo(&big(-2), &big(11)), big(5)); // 4^2 mod 11
    assert_eq!(big(2).exponent_modulo(&big(-1), &big(4)), big(0));
}

#[test]
fn b5_fermat_holds_above_2_pow_1024() {
    let p = m1279();
    let e = &p - big(1);
    for a in [2, 3, -5, 12345] {
        assert!(big(a).exponent_modulo(&e, &p).is_one(), "a = {a}");
    }
    let a = big(3) << 1100;
    let inv = a.inverse_modulo(&p).unwrap();
    assert!((&a * &inv % &p).is_one());
}

#[test]
fn b5_inverse_and_gcd_edge_cases() {
    assert_eq!(big(3).inverse_modulo(&big(0)), None);
    assert_eq!(big(0).inverse_modulo(&big(7)), None);
    assert_eq!(big(5).inverse_modulo(&big(1)), Some(big(0)));
    assert_eq!(big(-3).inverse_modulo(&big(11)), Some(big(7))); // -3 * 7 = -21 = 1 mod 11
    assert_eq!(big(3).inverse_modulo(&big(-11)), Some(big(4)));
    assert_eq!(big(0).find_greatest_common_divisor(&big(0)), big(0));
    assert_eq!(big(0).find_greatest_common_divisor(&big(-9)), big(9));
    assert_eq!(big(-12).find_greatest_common_divisor(&big(-18)), big(6));
}

proptest! {
    #[test]
    fn b5_matches_u128(b in any::<u64>(), e in any::<u64>(), m in 1_u64.., c in any::<u128>(), d in any::<u128>()) {
        let r = BigInteger::from(b).exponent_modulo(&BigInteger::from(e), &BigInteger::from(m));
        prop_assert_eq!(r, BigInteger::from(pow_mod_u128(u128::from(b), u128::from(e), u128::from(m))));
        let g = BigInteger::from(c).find_greatest_common_divisor(&BigInteger::from(d));
        prop_assert_eq!(g, BigInteger::from(gcd_u128(c, d)));
        let inv = BigInteger::from(b).inverse_modulo(&BigInteger::from(m));
        prop_assert_eq!(inv.is_some(), gcd_u128(u128::from(b), u128::from(m)) == 1);
    }

    #[test]
    fn b5_identities_hold_for_large_values(a in large(), m in large(), e in 0_u32..200, f in 0_u32..200) {
        let m = m.abs() + big(2);
        if let Some(inv) = a.inverse_modulo(&m) {
            prop_assert!(!inv.is_negative() && inv < m);
            // `%` keeps the dividend's sign, so a negative `a` gives 1 - m.
            let p = &a * &inv % &m;
            prop_assert!(p.is_one() || (p + &m).is_one());
            prop_assert!(a.find_greatest_common_divisor(&m).is_one());
        } else {
            prop_assert!(!a.find_greatest_common_divisor(&m).is_one());
        }
        let x = a.exponent_modulo(&BigInteger::from(e + f), &m);
        prop_assert!(!x.is_negative() && x < m);
        let y = a.exponent_modulo(&BigInteger::from(e), &m) * a.exponent_modulo(&BigInteger::from(f), &m) % &m;
        prop_assert_eq!(x, y);
        let g = a.find_greatest_common_divisor(&m);
        prop_assert!((&a % &g).is_zero() && (&m % &g).is_zero());
    }
}

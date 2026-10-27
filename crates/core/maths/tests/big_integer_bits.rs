//! B3: `BigInteger` bit access, shifts and bitwise operators.

use proptest::prelude::*;
use syrp_maths::BigInteger;

fn big(v: i128) -> BigInteger {
    BigInteger::from(v)
}

/// Builds a non-negative value from little-endian limbs using only bit operations.
fn from_limbs(limbs: &[u64]) -> BigInteger {
    let mut out = BigInteger::default();
    for (i, &l) in limbs.iter().enumerate() {
        out |= BigInteger::from(l) << (64 * i as u32);
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
fn b3_bits_act_on_the_magnitude() {
    let mut v = big(-0b1010);
    assert!(v.bit(1) && v.bit(3) && !v.bit(0));
    v.set_bit(0, true);
    assert_eq!(v, big(-0b1011));
    v.clear_bit(3);
    v.flip_bit(2);
    assert_eq!(v, big(-0b0111));
    assert_eq!(v.count_set_bits(), 3);
    assert_eq!(v.highest_bit(), Some(2));
    v.set_range(0, 3, false);
    assert!(v.is_zero() && !v.is_negative());
}

#[test]
fn b3_shifts_keep_sign_and_drop_low_bits() {
    assert_eq!(big(-5) >> 1, big(-2));
    assert_eq!(big(-1) >> 1, big(0));
    assert!(!(big(-1) >> 1).is_negative());
    assert_eq!(big(-3) << 70, big(-3 * (1 << 70)));
    let mut v = big(1);
    v <<= 1000;
    assert_eq!(v.highest_bit(), Some(1000));
    v >>= 999;
    assert_eq!(v, big(2));
}

#[test]
fn b3_bitwise_keeps_left_sign_syrp_choice() {
    assert_eq!(big(-0b1100) & big(0b1010), big(-0b1000));
    assert_eq!(big(0b1100) | big(-0b1010), big(0b1110));
    assert_eq!(big(-0b1100) ^ big(-0b1100), big(0));
    assert!(!(big(-0b1100) ^ big(-0b1100)).is_negative());
}

#[test]
fn b3_ranges_inserts_and_searches() {
    let mut v = BigInteger::default();
    v.set_range(60, 10, true);
    assert_eq!(v, big(0x3FF << 60));
    assert_eq!(v.bit_range(62, 4), big(0xF));
    assert_eq!(v.bit_range_as_u32(58, 40), 0xFFC);
    assert_eq!(v.find_next_set_bit(0), Some(60));
    assert_eq!(v.find_next_clear_bit(60), Some(70));
    assert_eq!(v.find_next_set_bit(70), None);
    v.insert_bit(64, false);
    assert_eq!(v, big(0x7EF << 60));
    v.set_bit_range_as_u32(100, 40, u32::MAX);
    assert_eq!(v.highest_bit(), Some(131));
    assert_eq!(BigInteger::default().highest_bit(), None);
    assert_eq!(BigInteger::default().find_next_clear_bit(7), Some(7));
    let mut top = BigInteger::default();
    top.set_bit(u32::MAX, true);
    assert_eq!(top.highest_bit(), Some(u32::MAX));
    assert_eq!(top.find_next_set_bit(5), Some(u32::MAX));
    assert_eq!(top.find_next_clear_bit(u32::MAX), None);
}

proptest! {
    #[test]
    fn b3_bits_match_u128(a in any::<u64>(), b in any::<u128>(), n in 0_u32..64, k in 0_u32..40, v in any::<bool>()) {
        let (x, y) = (u128::from(a), b);
        let (bx, by) = (BigInteger::from(x), BigInteger::from(y));
        prop_assert_eq!(&bx & &by, BigInteger::from(x & y));
        prop_assert_eq!(&bx | &by, BigInteger::from(x | y));
        prop_assert_eq!(&bx ^ &by, BigInteger::from(x ^ y));
        prop_assert_eq!(&bx << n, BigInteger::from(x << n));
        prop_assert_eq!(&by >> (n * 2), BigInteger::from(y >> (n * 2)));
        prop_assert_eq!(by.bit(n * 2), (y >> (n * 2)) & 1 == 1);
        let mask = (1_u128 << k) - 1;
        prop_assert_eq!(by.bit_range(n, k), BigInteger::from((y >> n) & mask));
        prop_assert_eq!(u128::from(by.bit_range_as_u32(n, k)), (y >> n) & mask & 0xFFFF_FFFF);
        let mut s = by.clone();
        s.set_range(n, k, v);
        let expect = if v { y | (mask << n) } else { y & !(mask << n) };
        prop_assert_eq!(s, BigInteger::from(expect));
        let mut ins = bx.clone();
        ins.insert_bit(n, v);
        let low = x & ((1_u128 << n) - 1);
        prop_assert_eq!(ins, BigInteger::from(((x >> n) << (n + 1)) | (u128::from(v) << n) | low));
        let mut w = bx.clone();
        w.set_bit_range_as_u32(n, k, a as u32);
        let m32 = mask & 0xFFFF_FFFF;
        prop_assert_eq!(w, BigInteger::from((x & !(m32 << n)) | ((u128::from(a as u32) & m32) << n)));
        prop_assert_eq!(by.count_set_bits(), u64::from(y.count_ones()));
        prop_assert_eq!(by.highest_bit(), (y != 0).then(|| 127 - y.leading_zeros()));
        let above = y & !((1_u128 << n) - 1);
        prop_assert_eq!(by.find_next_set_bit(n), (above != 0).then(|| above.trailing_zeros()));
        prop_assert_eq!(by.find_next_clear_bit(n), Some((!y & !((1_u128 << n) - 1)).trailing_zeros().min(128).max(n)));
    }

    #[test]
    fn b3_shift_round_trip_for_large_values(a in large(), n in 0_u32..3000) {
        prop_assert_eq!(&(&a << n) >> n, a.clone());
        prop_assert_eq!(&a << n, &a * &(BigInteger::from(1) << n));
        prop_assert_eq!(&a >> n, (&a.abs() / &(BigInteger::from(1) << n)) * &(if a.is_negative() { BigInteger::from(-1) } else { BigInteger::from(1) }));
    }
}

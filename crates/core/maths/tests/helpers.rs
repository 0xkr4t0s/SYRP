//! B3: numeric helpers and constants.

use syrp_maths::{
    MathConstants, Tolerance, approximately_equal, approximately_equal_with, is_power_of_two,
    is_within, limit, map_from_log10, map_range, map_to_0to1, map_to_log10, next_power_of_two,
    round_to_int,
};

#[test]
fn b3_map_range_and_map_to_0to1_are_linear() {
    assert_eq!(map_range(0.0, -1.0, 1.0, 0.0, 100.0), 50.0);
    assert_eq!(map_range(3.0_f32, 2.0, 4.0, 10.0, 0.0), 5.0);
    assert_eq!(map_range(-1.0, 0.0, 1.0, 0.0, 10.0), -10.0);
    assert_eq!(map_to_0to1(30.0_f32, 20.0, 40.0), 0.5);
    assert_eq!(map_to_0to1(50.0, 20.0, 40.0), 1.5);
}

#[test]
fn b3_log10_mappings_are_inverse() {
    assert!((map_from_log10(0.0_f64, 20.0, 20_000.0) - 20.0).abs() < 1e-9);
    assert!((map_from_log10(1.0_f64, 20.0, 20_000.0) - 20_000.0).abs() < 1e-6);
    assert!((map_from_log10(1.0_f64 / 3.0, 20.0, 20_000.0) - 200.0).abs() < 1e-9);
    for i in 0..=100 {
        let p = f64::from(i) / 100.0;
        let back = map_to_log10(map_from_log10(p, 20.0, 20_000.0), 20.0, 20_000.0);
        assert!((back - p).abs() < 1e-12, "{p} -> {back}");
    }
    assert!((map_to_log10(1_000.0_f32, 10.0, 100_000.0) - 0.5).abs() < 1e-6);
}

#[test]
fn b3_limit_and_is_within() {
    assert_eq!(
        [limit(-1, 1, -5), limit(-1, 1, 0), limit(-1, 1, 5)],
        [-1, 0, 1]
    );
    assert_eq!(limit(0.0_f32, 1.0, 1.5), 1.0);
    assert!(is_within(0.3, 0.1 + 0.2, 1e-12));
    assert!(is_within(5.0_f32, 4.0, 1.0));
    assert!(!is_within(5.0_f32, 3.9, 1.0));
}

#[test]
fn b3_approximately_equal_uses_relative_and_absolute_tolerance() {
    assert!(approximately_equal(0.1 + 0.2, 0.3));
    assert!(approximately_equal(1e20_f32 * (1.0 + f32::EPSILON), 1e20));
    assert!(!approximately_equal(1.0_f32, 1.0 + 4.0 * f32::EPSILON));
    // Near zero only the absolute tolerance helps.
    assert!(!approximately_equal(0.0, 1e-300));
    assert!(approximately_equal_with(
        0.0,
        1e-300,
        Tolerance::default().with_absolute(1e-299)
    ));
    let rel = Tolerance::default().with_relative(1e-3);
    assert!(approximately_equal_with(1_000_000.0, 1_000_900.0, rel));
    assert!(!approximately_equal_with(1_000_000.0, 1_002_000.0, rel));
    assert!(approximately_equal(f64::INFINITY, f64::INFINITY));
    assert!(!approximately_equal(f64::INFINITY, f64::NEG_INFINITY));
    assert!(!approximately_equal(f64::NAN, f64::NAN));
}

#[test]
fn b3_powers_of_two() {
    assert_eq!(next_power_of_two(0_u32), Some(1));
    assert_eq!(next_power_of_two(1_u32), Some(1));
    assert_eq!(next_power_of_two(3_u32), Some(4));
    assert_eq!(next_power_of_two(1025_usize), Some(2048));
    assert_eq!(next_power_of_two(-7_i32), Some(1));
    assert_eq!(next_power_of_two(1 << 30_i32), Some(1 << 30));
    assert_eq!(next_power_of_two((1 << 30) + 1_i32), None);
    assert_eq!(next_power_of_two(u64::MAX), None);
    assert!(is_power_of_two(1_u32) && is_power_of_two(1_i64 << 62));
    assert!(!is_power_of_two(0_i32) && !is_power_of_two(i32::MIN) && !is_power_of_two(6_u64));
}

#[test]
fn b3_round_to_int_rounds_to_nearest() {
    assert_eq!(
        [
            round_to_int(0.4_f32),
            round_to_int(0.6_f32),
            round_to_int(-0.6_f32)
        ],
        [0, 1, -1]
    );
    assert_eq!([round_to_int(1.5_f64), round_to_int(-1.5_f64)], [2, -2]);
    assert_eq!(round_to_int(-1e30_f64), i32::MIN);
    assert_eq!(round_to_int(f32::NAN), 0);
}

#[test]
fn b3_math_constants_for_f32_and_f64() {
    assert_eq!(<f64 as MathConstants>::PI, std::f64::consts::PI);
    assert_eq!(<f64 as MathConstants>::TWO_PI, 2.0 * std::f64::consts::PI);
    assert_eq!(<f64 as MathConstants>::HALF_PI, std::f64::consts::PI / 2.0);
    assert!((<f64 as MathConstants>::EULER.ln() - 1.0).abs() < 1e-15);
    assert!((<f64 as MathConstants>::SQRT2 - 2.0_f64.sqrt()).abs() < 1e-15);
    assert_eq!(<f32 as MathConstants>::PI, std::f32::consts::PI);
    assert_eq!(<f32 as MathConstants>::TWO_PI, 2.0 * std::f32::consts::PI);
    assert_eq!(<f32 as MathConstants>::HALF_PI, std::f32::consts::PI / 2.0);
    assert_eq!(<f32 as MathConstants>::EULER, std::f32::consts::E);
    assert_eq!(<f32 as MathConstants>::SQRT2, 2.0_f32.sqrt());
}

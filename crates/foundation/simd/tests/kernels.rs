// SPDX-License-Identifier: MIT OR Apache-2.0
//! B5: every SIMD level agrees with the scalar reference, on unaligned lengths and offsets.
//!
//! Tolerance: all six kernels are exact (one rounding per element, or a selection of an
//! input), so the bound is 1 ulp and the observed difference is 0.

use proptest::prelude::*;
use syrp_simd::Level;
use syrp_simd::kernels::{self, Element};

/// Distance in units in the last place; NaN equals NaN.
trait Ulps: Element {
    fn ulps(self, other: Self) -> u64;
}
impl Ulps for f32 {
    fn ulps(self, other: Self) -> u64 {
        if self.is_nan() && other.is_nan() || self == other {
            return 0;
        }
        let key = |x: f32| {
            let b = x.to_bits() as i64;
            if b & (1 << 31) != 0 {
                -(b & 0x7fff_ffff)
            } else {
                b
            }
        };
        key(self).abs_diff(key(other))
    }
}
impl Ulps for f64 {
    fn ulps(self, other: Self) -> u64 {
        if self.is_nan() && other.is_nan() || self == other {
            return 0;
        }
        let key = |x: f64| {
            let b = x.to_bits() as i128;
            if b & (1 << 63) != 0 {
                -(b & 0x7fff_ffff_ffff_ffff)
            } else {
                b
            }
        };
        key(self).abs_diff(key(other)) as u64
    }
}

fn assert_close<E: Ulps + core::fmt::Debug>(got: &[E], want: &[E], what: &str, level: Level) {
    for (i, (g, w)) in got.iter().zip(want).enumerate() {
        assert!(
            g.ulps(*w) <= 1,
            "{what} at {level:?}, index {i}: {g:?} vs {w:?}"
        );
    }
}

/// Runs every kernel at every level on `data[off..]` slices and compares with `Level::Scalar`.
fn check_all<E: Ulps + core::fmt::Debug>(data: &[E], off: usize) {
    let n = (data.len() - off) / 4;
    let [a, b, c, d] = [0, 1, 2, 3].map(|k| &data[off + k * n..off + (k + 1) * n]);
    let run = |level: Level| {
        let mut outs = vec![d.to_vec(); 7];
        kernels::add(level, &mut outs[0], a);
        kernels::add_into(level, &mut outs[1], a, b);
        kernels::mul(level, &mut outs[2], a);
        kernels::mul_into(level, &mut outs[3], a, b);
        kernels::mul_add(level, &mut outs[4], a, b);
        kernels::mul_add_into(level, &mut outs[5], a, b, c);
        kernels::copy_with_gain(
            level,
            &mut outs[6],
            a,
            b.first().copied().unwrap_or(E::one()),
        );
        (
            outs,
            kernels::find_min_max(level, c),
            kernels::abs_max(level, c),
        )
    };
    let (want, want_mm, want_abs) = run(Level::Scalar);
    for level in Level::supported() {
        let (got, mm, abs) = run(level);
        for (k, (g, w)) in got.iter().zip(&want).enumerate() {
            assert_close(g, w, &format!("kernel {k}"), level);
        }
        assert_eq!(mm.is_some(), want_mm.is_some());
        if let (Some((lo, hi)), Some((wlo, whi))) = (mm, want_mm) {
            assert_close(&[lo, hi], &[wlo, whi], "find_min_max", level);
        }
        assert_close(&[abs], &[want_abs], "abs_max", level);
    }
}

fn finite_f32() -> impl Strategy<Value = f32> {
    prop::num::f32::NORMAL | prop::num::f32::SUBNORMAL | prop::num::f32::ZERO
}

fn finite_f64() -> impl Strategy<Value = f64> {
    prop::num::f64::NORMAL | prop::num::f64::SUBNORMAL | prop::num::f64::ZERO
}

proptest! {
    // Under Miri: few cases and no failure file (Miri isolation forbids `getcwd`).
    #![proptest_config(if cfg!(miri) {
        ProptestConfig { cases: 2, failure_persistence: None, ..ProptestConfig::default() }
    } else {
        ProptestConfig::with_cases(256)
    })]

    #[test]
    fn b5_f32_kernels_match_the_scalar_reference(
        data in prop::collection::vec(finite_f32(), 0..300), off in 0usize..16,
    ) {
        check_all(&data, off.min(data.len()));
    }

    #[test]
    fn b5_f64_kernels_match_the_scalar_reference(
        data in prop::collection::vec(finite_f64(), 0..300), off in 0usize..16,
    ) {
        check_all(&data, off.min(data.len()));
    }

    #[test]
    fn b5_small_range_f32_kernels_match(
        data in prop::collection::vec(-4.0f32..4.0, 0..100), off in 0usize..8,
    ) {
        check_all(&data, off.min(data.len()));
    }
}

#[test]
fn b5_mul_add_kernels_are_fused_on_every_level() {
    let a = vec![1.0 + f32::EPSILON; 37];
    let c = vec![-(1.0 + 2.0 * f32::EPSILON); 37];
    let a64 = vec![1.0 + f64::EPSILON; 19];
    let c64 = vec![-(1.0 + 2.0 * f64::EPSILON); 19];
    for level in Level::supported() {
        let mut out = c.clone();
        kernels::mul_add(level, &mut out, &a, &a);
        assert!(
            out.iter().all(|&x| x == f32::EPSILON * f32::EPSILON),
            "{level:?}"
        );
        let mut out = vec![0.0; 37];
        kernels::mul_add_into(level, &mut out, &a, &a, &c);
        assert!(
            out.iter().all(|&x| x == f32::EPSILON * f32::EPSILON),
            "{level:?}"
        );
        let mut out = c64.clone();
        kernels::mul_add(level, &mut out, &a64, &a64);
        assert!(
            out.iter().all(|&x| x == f64::EPSILON * f64::EPSILON),
            "{level:?}"
        );
    }
}

#[test]
fn b5_reductions_handle_empty_input_and_kernels_reject_mismatched_lengths() {
    let level = Level::detect();
    assert_eq!(kernels::find_min_max::<f32>(level, &[]), None);
    assert_eq!(kernels::abs_max::<f64>(level, &[]), 0.0);
    let r = std::panic::catch_unwind(|| kernels::add(level, &mut [0.0_f32; 3], &[1.0; 2]));
    assert!(r.is_err());
}

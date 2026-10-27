// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Tests for work/specs/WP-2.1-10.md B6 (first part): `FastMathApproximations`, `Phase` and
//! `LogRampedValue`.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use syrp_dsp_core::{FastMathApproximations as Fast, LogRampedValue, Phase};
use syrp_rt::no_alloc;
use syrp_simd::{F32x8, F64x4};
use syrp_testkit::{Tolerance, assert_close};

#[global_allocator]
static ALLOC: syrp_rt::GuardedAllocator = syrp_rt::GuardedAllocator;

/// How a documented error bound is measured.
#[derive(Clone, Copy)]
enum Err {
    Abs(f64),
    Rel(f64),
}

type Case = (&'static str, fn(f64) -> f64, fn(f64) -> f64, f64, f64, Err);

/// Every approximation, its reference, the documented range and the bound in its rustdoc.
const CASES: [Case; 8] = [
    ("sin", Fast::sin, f64::sin, -PI, PI, Err::Abs(1.2e-5)),
    ("cos", Fast::cos, f64::cos, -PI, PI, Err::Abs(7.4e-5)),
    (
        "tan",
        Fast::tan,
        f64::tan,
        -0.999 * FRAC_PI_2,
        0.999 * FRAC_PI_2,
        Err::Rel(1.7e-6),
    ),
    ("sinh", Fast::sinh, f64::sinh, -5.0, 5.0, Err::Rel(3.1e-6)),
    ("cosh", Fast::cosh, f64::cosh, -5.0, 5.0, Err::Rel(2.0e-5)),
    ("tanh", Fast::tanh, f64::tanh, -5.0, 5.0, Err::Abs(1.1e-4)),
    ("exp", Fast::exp, f64::exp, -6.0, 4.0, Err::Rel(4.6e-3)),
    (
        "log_nplusone",
        Fast::log_nplusone,
        f64::ln_1p,
        -0.8,
        5.0,
        Err::Abs(7.6e-5),
    ),
];

/// 20,001 evenly spaced points across `lo..=hi` (the measurement the rustdoc describes).
fn grid(lo: f64, hi: f64) -> impl Iterator<Item = f64> {
    (0..=20_000).map(move |i| lo + (hi - lo) * f64::from(i) / 20_000.0)
}

#[test]
fn b6_fast_math_errors_are_within_the_documented_bounds() {
    for (name, fast, exact, lo, hi, bound) in CASES {
        let mut worst = 0.0_f64;
        for x in grid(lo, hi) {
            let (a, e) = (fast(x), exact(x));
            let err = match bound {
                Err::Abs(_) => (a - e).abs(),
                Err::Rel(_) if e.abs() < 1e-300 => (a - e).abs(),
                Err::Rel(_) => ((a - e) / e).abs(),
            };
            worst = worst.max(err);
        }
        let (Err::Abs(limit) | Err::Rel(limit)) = bound;
        assert!(
            worst <= limit,
            "{name}: measured {worst:e} above documented {limit:e}"
        );
        assert!(
            worst >= limit / 10.0,
            "{name}: documented {limit:e} is far above {worst:e}"
        );
    }
}

#[test]
fn b6_fast_math_exp_is_tight_near_zero() {
    for x in grid(-1.5, 1.5) {
        assert!((Fast::exp(x) / x.exp() - 1.0).abs() <= 1e-6, "x {x}");
    }
}

#[test]
fn b6_fast_math_is_lane_generic_and_allocation_free() {
    let xs = [-3.0_f32, -1.0, -0.25, 0.0, 0.1, 0.7, 1.9, 3.0];
    let packed = no_alloc(|| {
        let v = F32x8::from_array(xs);
        [Fast::sin(v), Fast::cos(v), Fast::tanh(v), Fast::sinh(v)]
    });
    let scalar_fns: [fn(f32) -> f32; 4] = [Fast::sin, Fast::cos, Fast::tanh, Fast::sinh];
    for (p, f) in packed.iter().zip(scalar_fns) {
        let expected: Vec<f32> = xs.iter().map(|&x| f(x)).collect();
        assert_close(&p.to_array(), &expected, Tolerance::exact());
    }
    // f32 agrees with f64 to about f32 precision.
    for x in grid(-PI, PI).step_by(97) {
        let d = f64::from(Fast::sin(x as f32)) - Fast::sin(x);
        assert!(d.abs() <= 1e-6, "x {x}: {d}");
    }
}

#[test]
fn b6_phase_wraps_past_two_pi() {
    let mut p = Phase::<f64>::new();
    let inc = 1.0;
    let mut expected = 0.0;
    for _ in 0..100 {
        let before = p.advance(inc);
        assert!((before - expected).abs() <= 1e-9);
        expected = (expected + inc) % TAU;
        assert!((0.0..TAU).contains(&p.value()));
    }
    p.reset();
    assert_eq!(p.value(), 0.0);
}

#[test]
fn b6_phase_lanes_advance_independently() {
    let incs = [0.1, 1.0, 3.0, 6.2];
    let mut packed = Phase::<F64x4>::new();
    let mut scalars = [Phase::<f64>::new(); 4];
    for _ in 0..50 {
        let before = packed.advance(F64x4::from_array(incs));
        for (lane, (s, inc)) in scalars.iter_mut().zip(incs).enumerate() {
            assert_eq!(before.to_array()[lane], s.advance(inc));
        }
    }
}

/// `start + (end - start) g(t)` with the curve documented on `LogRampedValue`.
fn curve(start: f64, end: f64, t: f64, mid_db: f64, rising: bool) -> f64 {
    let m = 10.0_f64.powf(mid_db / 20.0);
    let k = 2.0 * (1.0 / m - 1.0).ln();
    let g = |t: f64| ((k * t).exp() - 1.0) / (k.exp() - 1.0);
    let shape = if rising { g(t) } else { 1.0 - g(1.0 - t) };
    start + (end - start) * shape
}

#[test]
fn b6_log_ramped_value_follows_the_documented_curve() {
    for (mid_db, rising) in [(-12.0, true), (-12.0, false), (-3.0, true), (-40.0, false)] {
        let mut v = LogRampedValue::<f64>::new(0.25);
        v.set_log_parameters(mid_db, rising);
        v.reset(100);
        v.set_target_value(-0.75);
        for j in 1..=100 {
            let got = v.next_value();
            let want = curve(0.25, -0.75, f64::from(j) / 100.0, mid_db, rising);
            assert!(
                (got - want).abs() <= 1e-12,
                "{mid_db} dB {rising}: step {j}: {got} vs {want}"
            );
        }
        assert!(!v.is_smoothing());
        assert_eq!(v.current_value(), -0.75);
    }
}

#[test]
fn b6_log_ramped_value_midpoint_and_direction() {
    let mut up = LogRampedValue::<f32>::new(0.0);
    up.set_log_parameters(-12.0, true);
    up.reset_seconds(1000.0, 0.1);
    up.set_target_value(1.0);
    up.skip(50);
    let m = 10.0_f32.powf(-12.0 / 20.0);
    assert!(
        (up.current_value() - m).abs() <= 1e-6,
        "rising midpoint is m"
    );

    let mut down = LogRampedValue::<f32>::new(0.0);
    down.set_log_parameters(-12.0, false);
    down.reset(100);
    down.set_target_value(1.0);
    down.skip(50);
    assert!(
        (down.current_value() - (1.0 - m)).abs() <= 1e-6,
        "falling midpoint is 1 - m"
    );
}

#[test]
fn b6_log_ramped_value_retargets_from_where_it_is_without_allocating() {
    let mut v = LogRampedValue::<f32>::new(0.0);
    v.reset(64);
    v.set_target_value(1.0);
    let (mid, end) = no_alloc(|| {
        v.skip(32);
        let mid = v.current_value();
        v.set_target_value(0.0);
        assert!(v.is_smoothing());
        v.skip(64);
        (mid, v.next_value())
    });
    assert!(mid > 0.0 && mid < 1.0);
    assert_eq!(end, 0.0);
    v.set_current_and_target_value(0.5);
    assert_eq!((v.current_value(), v.target_value()), (0.5, 0.5));
}

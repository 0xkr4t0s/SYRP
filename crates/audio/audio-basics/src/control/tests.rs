// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! WP-2.1-03 increment 1: B1 (decibels), B2 (`SmoothedValue`) and B3 (control rate).
//! Packs are checked lane by lane against scalar runs at the `f32-arith` tolerance.

use crate::control::decibels::{
    DecibelFloat, MINUS_INFINITY_TEXT, decibels_to_gain, gain_to_decibels, to_string,
};
use crate::control::{Linear, Mapping, Multiplicative, SmoothedValue};
use syrp_simd::{F32x4, F32x8, F64x4, Mask, Sample};

/// The `f32-arith` profile: 4 ulp, or 1e-7 absolute near zero.
fn arith(got: f32, want: f32) -> bool {
    (got - want).abs() <= 1e-7_f32.max(want.abs() * 4.0 * f32::EPSILON)
}

/// The `nonlinear` static-curve bound, 1e-5, relative to values above 1.
fn nonlinear(got: f32, want: f32) -> bool {
    (got - want).abs() <= 1e-5 * want.abs().max(1.0)
}

fn ramp<S: crate::control::ControlSample, M: Mapping>(
    v: &mut SmoothedValue<S, M>,
    n: usize,
) -> Vec<S> {
    (0..n).map(|_| v.next_value()).collect()
}

#[test]
fn b1_gain_and_decibels_convert_both_ways() {
    for (gain, db) in [
        (1.0_f64, 0.0),
        (10.0, 20.0),
        (0.1, -20.0),
        (0.5, -6.020_599_913),
    ] {
        assert!((gain_to_decibels(gain, -100.0) - db).abs() < 1e-9, "{gain}");
        assert!((decibels_to_gain(db, -100.0) - gain).abs() < 1e-9, "{db}");
        let g32 = gain as f32;
        let back = decibels_to_gain(gain_to_decibels(g32, -100.0), -100.0);
        assert!(arith(back, g32), "{g32} -> {back}");
    }
    // Packs convert lane by lane.
    let gains = F32x4::from_array([1.0, 0.1, 0.0, 10.0]);
    let db = gains.to_array().map(|g| gain_to_decibels(g, -100.0));
    assert_eq!(db, [0.0, -20.0, -100.0, 20.0]);
}

#[test]
fn b1_floor_maps_to_floor_and_back_to_zero() {
    assert_eq!(f32::DEFAULT_MINUS_INFINITY_DB, -100.0);
    assert_eq!(f64::DEFAULT_MINUS_INFINITY_DB, -100.0);
    for gain in [0.0_f32, -0.5, 1e-5, 1e-7, f32::NAN] {
        assert_eq!(gain_to_decibels(gain, -100.0), -100.0, "{gain}");
    }
    assert_eq!(gain_to_decibels(0.001_f64, -60.0), -60.0);
    assert_eq!(gain_to_decibels(0.002_f64, -60.0), 20.0 * 0.002_f64.log10());
    for db in [-100.0_f64, -100.5, -1000.0, f64::NEG_INFINITY] {
        assert_eq!(decibels_to_gain(db, -100.0), 0.0, "{db}");
    }
    assert!(decibels_to_gain(-99.0_f32, -100.0) > 0.0);
    assert_eq!(decibels_to_gain(-60.0_f32, -60.0), 0.0);
    assert_eq!(
        decibels_to_gain(gain_to_decibels(0.0_f32, -100.0), -100.0),
        0.0
    );
}

#[test]
fn b1_to_string_formats_with_suffix_and_minus_infinity_text() {
    assert_eq!(to_string(-6.020_6_f64, 2, MINUS_INFINITY_TEXT), "-6.02 dB");
    assert_eq!(to_string(12.345_f32, 1, MINUS_INFINITY_TEXT), "12.3 dB");
    assert_eq!(to_string(0.0_f32, 0, MINUS_INFINITY_TEXT), "0 dB");
    assert_eq!(to_string(-100.0_f32, 2, MINUS_INFINITY_TEXT), "-INF");
    assert_eq!(to_string(-150.0_f64, 2, "off"), "off");
    assert_eq!(to_string(-99.5_f64, 1, "off"), "-99.5 dB");
}

#[test]
fn b2_linear_ramp_reaches_target_exactly_after_ramp_length() {
    let mut v = SmoothedValue::<f32>::new(0.3);
    v.reset(44_100.0, 0.01); // 441 samples
    assert_eq!(v.ramp_steps(), 441);
    v.set_target(0.7);
    let out = ramp(&mut v, 441);
    assert!(out.windows(2).all(|w| w[0] < w[1]));
    assert!(out[..440].iter().all(|&x| x < 0.7));
    assert_eq!(out[440], 0.7);
    assert!(!v.is_smoothing());
    assert_eq!((v.next_value(), v.current(), v.target()), (0.7, 0.7, 0.7));
    for (i, &x) in out.iter().enumerate() {
        let want = 0.3 + 0.4 * (i + 1) as f32 / 441.0;
        assert!((x - want).abs() < 1e-5, "{i}: {x} vs {want}");
    }
}

#[test]
fn b2_new_target_mid_ramp_restarts_from_current() {
    let mut v = SmoothedValue::<f64>::new(0.0);
    v.reset_steps(10);
    v.set_target(10.0);
    assert_eq!(v.skip(4), 4.0);
    v.set_target(10.0); // same target: the ramp continues
    assert_eq!(v.next_value(), 5.0);
    v.set_target(0.0); // ten steps from 5.0 down to 0.0
    let out = ramp(&mut v, 10);
    assert!((out[0] - 4.5).abs() < 1e-12 && out[9] == 0.0);
    // Jumps: set_current_and_target, reset, and a ramp length of zero.
    v.set_target(1.0);
    v.set_current_and_target(2.0);
    assert!(!v.is_smoothing() && v.next_value() == 2.0);
    v.set_target(3.0);
    v.reset_steps(10);
    assert_eq!((v.current(), v.is_smoothing()), (3.0, false));
    v.reset_steps(0);
    v.set_target(-1.0);
    assert_eq!(v.current(), -1.0);
}

#[test]
fn b2_multiplicative_ramp_is_geometric() {
    let mut v = SmoothedValue::<f64, Multiplicative>::new(0.001);
    v.reset_steps(30);
    v.set_target(1.0);
    let out = ramp(&mut v, 30);
    for (i, &x) in out.iter().enumerate() {
        let want = 0.001 * 1000.0_f64.powf((i + 1) as f64 / 30.0);
        assert!((x - want).abs() <= 1e-12 * want.max(1e-3), "{i}");
    }
    assert_eq!(out[29], 1.0);
    // Negative ranges work too, as long as they do not cross zero.
    let mut n = SmoothedValue::<f32, Multiplicative>::new(-1.0);
    n.reset_steps(2);
    n.set_target(-4.0);
    assert_eq!(n.next_value(), -2.0);
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "Multiplicative smoothing")]
fn b2_multiplicative_rejects_zero() {
    let _ = SmoothedValue::<f32, Multiplicative>::new(0.0);
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "Multiplicative smoothing")]
fn b2_multiplicative_rejects_sign_crossing() {
    let mut v = SmoothedValue::<F32x4, Multiplicative>::new(F32x4::one());
    v.reset_steps(4);
    v.set_target(F32x4::from_array([2.0, 2.0, -1.0, 2.0]));
}

fn check_skip<M: Mapping>(from: f32, to: f32) {
    for n in [0_u32, 1, 7, 63, 64, 65, 500] {
        let mut a = SmoothedValue::<f32, M>::new(from);
        a.reset_steps(64);
        a.set_target(to);
        let mut b = a;
        a.next_value();
        b.next_value();
        let skipped = a.skip(n);
        let stepped = (0..n).fold(b.current(), |_, _| b.next_value());
        assert!(nonlinear(skipped, stepped), "n={n}: {skipped} vs {stepped}");
        assert_eq!(a.is_smoothing(), b.is_smoothing(), "n={n}");
        assert!(nonlinear(a.next_value(), b.next_value()), "after n={n}");
    }
}

#[test]
fn b2_skip_matches_next_value_n_times() {
    check_skip::<Linear>(-1.0, 3.0);
    check_skip::<Multiplicative>(0.01, 2.0);
}

#[test]
fn b2_apply_gain_ramps_a_block() {
    let mut v = SmoothedValue::<F32x4>::new(F32x4::one());
    v.reset_steps(4);
    v.set_target(F32x4::from_array([0.0, 2.0, 1.0, 3.0]));
    let mut block = [F32x4::splat(2.0); 6];
    v.apply_gain(&mut block);
    let lane = |l: usize| block.map(|p| p.lane(l));
    assert_eq!(lane(0), [1.5, 1.0, 0.5, 0.0, 0.0, 0.0]);
    assert_eq!(lane(1), [2.5, 3.0, 3.5, 4.0, 4.0, 4.0]);
    assert_eq!(lane(2), [2.0; 6]);
    assert_eq!(lane(3), [3.0, 4.0, 5.0, 6.0, 6.0, 6.0]);
    v.apply_gain(&mut block); // not smoothing: multiplies by the target
    assert_eq!(block[5].to_array(), [0.0, 8.0, 2.0, 18.0]);
}

/// Per-lane scripts: (initial, first target, second target, sample at which lane changes).
const SCRIPT: [(f32, f32, f32, usize); 8] = [
    (1.0, 2.0, 0.5, 10),
    (0.5, 0.5, 4.0, 3),
    (2.0, 0.25, 0.25, 50),
    (0.1, 3.0, 1.0, 37),
    (1.0, 1.5, 1.5, 0),
    (4.0, 0.2, 0.9, 64),
    (0.3, 0.6, 0.1, 21),
    (0.75, 0.75, 0.75, 5),
];

/// Runs the script with `M` on a pack (one lane per entry) and on scalar values, and checks
/// each lane against its scalar run at `f32-arith`, the smoothing masks included.
fn lanes_match<P: crate::control::ControlSample, M: Mapping, const N: usize>(
    pack: fn([f32; N]) -> P,
    lanes: fn(P) -> [f32; N],
    smoothing: fn(P::Mask) -> [bool; N],
    control: u32,
) {
    let col = |f: fn(&(f32, f32, f32, usize)) -> f32| pack(core::array::from_fn(|l| f(&SCRIPT[l])));
    let mut p = SmoothedValue::<P, M>::new(col(|s| s.0)).with_control_rate(control);
    let mut s: [SmoothedValue<f32, M>; N] =
        core::array::from_fn(|l| SmoothedValue::new(SCRIPT[l].0).with_control_rate(control));
    p.reset_steps(40);
    s.iter_mut().for_each(|v| v.reset_steps(40));
    p.set_target(col(|s| s.1));
    s.iter_mut()
        .zip(&SCRIPT)
        .for_each(|(v, l)| v.set_target(l.1));
    let mut targets = lanes(col(|s| s.1));
    for t in 0..120 {
        // Each lane gets its second target at its own sample; the others keep theirs.
        for (l, script) in SCRIPT.iter().take(N).enumerate() {
            if script.3 == t {
                targets[l] = script.2;
                s[l].set_target(script.2);
            }
        }
        p.set_target(pack(targets));
        let got = lanes(if t % 17 == 5 {
            p.skip(3)
        } else {
            p.next_value()
        });
        let mask = smoothing(p.is_smoothing());
        for l in 0..N {
            let want = if t % 17 == 5 {
                s[l].skip(3)
            } else {
                s[l].next_value()
            };
            assert!(
                arith(got[l], want),
                "N={N} c={control} t={t} lane {l}: {} vs {want}",
                got[l]
            );
            assert_eq!(mask[l], s[l].is_smoothing(), "t={t} lane {l}");
        }
    }
}

#[test]
fn b2_pack_lanes_match_scalar_runs() {
    for control in [1, 16] {
        lanes_match::<f32, Linear, 1>(|a| a[0], |p| [p], |m| [m], control);
        lanes_match::<F32x4, Linear, 4>(
            F32x4::from_array,
            F32x4::to_array,
            |m| m.to_array(),
            control,
        );
        lanes_match::<F32x8, Linear, 8>(
            F32x8::from_array,
            F32x8::to_array,
            |m| m.to_array(),
            control,
        );
        lanes_match::<F32x4, Multiplicative, 4>(
            F32x4::from_array,
            F32x4::to_array,
            |m| m.to_array(),
            control,
        );
        lanes_match::<F32x8, Multiplicative, 8>(
            F32x8::from_array,
            F32x8::to_array,
            |m| m.to_array(),
            control,
        );
    }
    // f64 packs against f64 scalars.
    let mut p =
        SmoothedValue::<F64x4, Multiplicative>::new(F64x4::from_array([1.0, 2.0, 0.5, 8.0]));
    p.reset_steps(16);
    p.set_target(F64x4::from_array([2.0, 2.0, 0.25, 1.0]));
    let out = ramp(&mut p, 16);
    for (l, (a, b)) in [(1.0, 2.0), (2.0, 2.0), (0.5, 0.25), (8.0, 1.0)]
        .into_iter()
        .enumerate()
    {
        let mut s = SmoothedValue::<f64, Multiplicative>::new(a);
        s.reset_steps(16);
        s.set_target(b);
        for x in &out {
            let want = s.next_value();
            assert!((x.lane(l) - want).abs() <= 4.0 * f64::EPSILON * want.abs());
        }
    }
}

/// A ramp with control rate `n` against the per-sample ramp, within `tol` relative to values
/// above 1, including a restart a third of the way in.
fn control_tracks<S, M>(from: S, to: S, steps: u32, n: u32, tol: f64)
where
    S: crate::control::ControlSample + Into<f64>,
    M: Mapping,
{
    let mut fine = SmoothedValue::<S, M>::new(from);
    fine.reset_steps(steps);
    fine.set_target(to);
    let mut coarse = SmoothedValue::<S, M>::new(from).with_control_rate(n);
    coarse.reset_steps(steps);
    coarse.set_target(to);
    for i in 0..steps + steps / 3 + 1 {
        if i == steps / 3 {
            // A new target mid-ramp restarts both from their current values.
            fine.set_target(from);
            coarse.set_target(from);
        }
        let (f, c): (f64, f64) = (fine.next_value().into(), coarse.next_value().into());
        assert!(
            (c - f).abs() <= tol * f.abs().max(1.0),
            "n={n} i={i}: {c} vs {f}"
        );
        assert_eq!(fine.is_smoothing(), coarse.is_smoothing(), "n={n} i={i}");
    }
    assert_eq!(coarse.current(), from);
}

#[test]
fn b3_control_rate_stays_within_nonlinear_tolerance() {
    for n in [2, 16, 32] {
        control_tracks::<f32, Linear>(0.0, 1.0, 4800, n, 1e-5);
        control_tracks::<f32, Linear>(-3.0, 5.0, 100, n, 1e-5);
        // 40 dB in half a second and 12 dB in 0.2 s at 48 kHz.
        control_tracks::<f64, Multiplicative>(0.01, 1.0, 24_000, n, 1e-5);
        control_tracks::<f64, Multiplicative>(2.0, 0.5, 9_600, n, 1e-5);
        // In `f32` a multiplicative ramp's per-sample ratio is rounded, and both paths drift
        // from the exact curve by up to about steps * 2^-24 relative; the bound covers that.
        control_tracks::<f32, Multiplicative>(2.0, 0.5, 9_600, n, 1e-4);
    }
}

#[test]
fn b3_per_sample_is_the_default_and_control_rate_ends_on_target() {
    let v = SmoothedValue::<F32x8>::new(F32x8::zero());
    assert_eq!(v.control_rate(), 1);
    assert_eq!(v.with_control_rate(0).control_rate(), 1);
    let mut c = SmoothedValue::<F32x8>::new(F32x8::zero()).with_control_rate(16);
    c.reset_steps(37);
    c.set_target(F32x8::from_array([1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]));
    let out = ramp(&mut c, 37);
    assert!(c.is_smoothing().none());
    assert_eq!(out[36].to_array(), [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
    assert!(out[35].simd_lt(out[36]).all());
}

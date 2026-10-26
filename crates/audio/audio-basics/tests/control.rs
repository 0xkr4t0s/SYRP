// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! WP-2.1-03: B1 (decibels), B2 (`SmoothedValue`), B3 (control rate), B4 (`Adsr`), B5 (every
//! lane-generic type with `f32`, `f64`, `F32x4` and `F32x8`, each pack lane against a scalar
//! run at the `f32-arith` tolerance) and B6 (allocation-free, `RtSafe`).

use syrp_audio_basics::control::decibels::{
    DecibelFloat, MINUS_INFINITY_TEXT, decibels_to_gain, gain_to_decibels, to_string,
};
use syrp_audio_basics::control::{
    Adsr, AdsrParams, ControlSample, Linear, Mapping, Multiplicative, SmoothedValue,
};
use syrp_rt::{RtSafe, no_alloc};
use syrp_simd::{F32x4, F32x8, Mask, Sample};

#[global_allocator]
static ALLOC: syrp_rt::GuardedAllocator = syrp_rt::GuardedAllocator;

/// The `f32-arith` profile: 4 ulp, or 1e-7 absolute near zero.
fn arith(got: f32, want: f32) -> bool {
    (got - want).abs() <= 1e-7_f32.max(want.abs() * 4.0 * f32::EPSILON)
}

/// The `nonlinear` static-curve bound, 1e-5, relative to values above 1.
fn nonlinear(got: f32, want: f32) -> bool {
    (got - want).abs() <= 1e-5 * want.abs().max(1.0)
}

fn ramp<S: ControlSample, M: Mapping>(v: &mut SmoothedValue<S, M>, n: usize) -> Vec<S> {
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
    // NaN reads as silence, as `gain_to_decibels` maps a NaN gain to the floor.
    assert_eq!(to_string(f32::NAN, 2, MINUS_INFINITY_TEXT), "-INF");
    assert_eq!(to_string(f64::NAN, 1, "off"), "off");
    // The cutoff is the default floor whatever floor the value was converted with.
    assert_eq!(
        to_string(gain_to_decibels(0.0_f32, -60.0), 0, "off"),
        "-60 dB"
    );
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

/// A ramp with control rate `n` against the per-sample ramp, within `tol` relative to values
/// above 1, including a restart a third of the way in.
fn control_tracks<S, M>(from: S, to: S, steps: u32, n: u32, tol: f64)
where
    S: ControlSample + Into<f64>,
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

/// Runs `env` for `n` samples.
fn run<S: ControlSample>(env: &mut Adsr<S>, n: usize) -> Vec<S> {
    (0..n).map(|_| env.next_sample()).collect()
}

fn approx(got: &[f32], want: &[f32]) {
    assert_eq!(got.len(), want.len());
    for (i, (g, w)) in got.iter().zip(want).enumerate() {
        assert!((g - w).abs() <= 1e-6, "{i}: {got:?} vs {want:?}");
    }
}

#[test]
fn b4_segments_are_linear_and_last_their_rounded_length() {
    // At 1 kHz: attack 10.4 samples (rounds to 10 equal steps), decay 5, release 8.
    let mut env = Adsr::new(1000.0, AdsrParams::new(0.0104_f32, 0.005, 0.4, 0.008));
    env.note_on(true);
    let out = run(&mut env, 20);
    let attack: Vec<f32> = (1..10).map(|k| k as f32 / 10.0).collect();
    approx(&out[..9], &attack);
    assert_eq!(out[9], 1.0);
    approx(&out[10..15], &[0.88, 0.76, 0.64, 0.52, 0.4]);
    assert_eq!(out[14..], [0.4; 6]);
    env.note_off(true);
    let release = run(&mut env, 9);
    let want: Vec<f32> = (1..=8)
        .map(|k| 0.4 - 0.05 * k as f32)
        .chain([0.0])
        .collect();
    approx(&release, &want);
    assert_eq!(release[7], 0.0);
    assert!(!env.is_active());
}

#[test]
fn b4_zero_length_segments_jump() {
    let mut env = Adsr::new(48_000.0, AdsrParams::new(0.0_f64, 0.0, 0.5, 0.0));
    env.note_on(true);
    assert_eq!(run(&mut env, 3), [1.0, 0.5, 0.5]);
    env.note_off(true);
    assert_eq!(run(&mut env, 2), [0.0, 0.0]);
    assert!(!env.is_active());
}

#[test]
fn b4_release_starts_from_current_level() {
    let mut env = Adsr::new(1000.0, AdsrParams::new(0.01_f32, 0.0, 1.0, 0.004));
    env.note_on(true);
    approx(&run(&mut env, 4), &[0.1, 0.2, 0.3, 0.4]);
    env.note_off(true); // mid-attack: four samples from 0.4 down to 0
    approx(&run(&mut env, 4), &[0.3, 0.2, 0.1, 0.0]);
    assert_eq!(env.level(), 0.0);
    assert!(!env.is_active());
}

#[test]
fn b4_note_on_during_release_restarts_attack_from_current_level() {
    let mut env = Adsr::new(1000.0, AdsrParams::new(0.01_f32, 0.0, 1.0, 0.01));
    env.note_on(true);
    assert_eq!(run(&mut env, 11)[9..], [1.0, 1.0]);
    env.note_off(true);
    approx(&run(&mut env, 2), &[0.9, 0.8]);
    env.note_on(true); // the attack slope, from 0.8
    approx(&run(&mut env, 3), &[0.9, 1.0, 1.0]);
    assert!(env.is_active());
}

#[test]
fn b4_settings_sample_rate_reset_and_idle_lanes() {
    let mut env = Adsr::new(1000.0, AdsrParams::new(0.004_f32, 0.0, 1.0, 0.0));
    env.note_off(true); // idle: stays idle
    assert!(!env.is_active());
    let mut block = [3.0_f32; 4];
    env.apply_to(&mut block);
    assert_eq!(block, [0.0; 4]);
    env.set_sample_rate(500.0); // the attack is now two samples
    assert_eq!(env.sample_rate(), 500.0);
    env.note_on(true);
    let mut block = [2.0_f32; 3];
    env.apply_to(&mut block);
    assert_eq!(block, [1.0, 2.0, 2.0]);
    env.set_params(AdsrParams::new(0.0, 0.0, 0.25, 0.0)); // sustain follows the new level
    assert_eq!(run(&mut env, 2), [0.25, 0.25]);
    assert_eq!(env.params().sustain(), 0.25);
    env.reset();
    assert_eq!(
        (env.is_active(), env.level(), env.next_sample()),
        (false, 0.0, 0.0)
    );
    // Packs: one lane per gate.
    let p = AdsrParams::new(
        F32x4::splat(0.002),
        F32x4::zero(),
        F32x4::one(),
        F32x4::zero(),
    );
    let mut env = Adsr::new(1000.0, p);
    env.note_on(F32x4::from_array([1.0, 0.0, 1.0, 0.0]).simd_gt(F32x4::zero()));
    assert_eq!(env.next_sample().to_array(), [0.5, 0.0, 0.5, 0.0]);
    assert_eq!(env.is_active().to_array(), [true, false, true, false]);
}

#[test]
fn b4_sustain_change_mid_decay_still_finishes_the_stage() {
    // Decay from 1.0 toward 0.5 over 10 samples; after 3 samples the level is 0.85.
    for (sustain, len) in [(1.0_f32, 10), (0.9, 5)] {
        let mut env = Adsr::new(1000.0, AdsrParams::new(0.0_f32, 0.01, 0.5, 0.0));
        env.note_on(true);
        run(&mut env, 4);
        assert!((env.level() - 0.85).abs() < 1e-6);
        // Sustain 1 has a zero decay step: the stage takes its full length (10 samples).
        // Sustain 0.9 is above the level, 0.05 away at the new decay step of 0.01: 5 samples.
        env.set_params(AdsrParams::new(0.0, 0.01, sustain, 0.0));
        let out = run(&mut env, len + 2);
        assert!(out.windows(2).all(|w| w[0] <= w[1]), "{sustain}: {out:?}");
        assert_eq!(out[len - 1..], [sustain; 3], "{sustain}: {out:?}");
        assert!(env.is_active());
        env.note_off(true);
        run(&mut env, 1);
        assert!(!env.is_active());
    }
}

/// The B5 matrix: `f32`, `f64`, `F32x4` and `F32x8`, each compared lane by lane with runs of
/// its scalar element type.
trait Lanes: ControlSample {
    /// The element type, which is its own one-lane `Lanes`.
    type E: Lanes;
    /// The `f32-arith` bound in ulp of the element type.
    const EPS: f64;
    fn build(f: impl Fn(usize) -> f64) -> Self;
    fn get(self, lane: usize) -> f64;
    fn mask(f: impl Fn(usize) -> bool) -> Self::Mask {
        Self::build(|l| if f(l) { 1.0 } else { 0.0 }).simd_gt(Self::zero())
    }
    fn bit(mask: Self::Mask, lane: usize) -> bool {
        Self::select(mask, Self::one(), Self::zero()).get(lane) == 1.0
    }
}

impl Lanes for f32 {
    type E = f32;
    const EPS: f64 = f32::EPSILON as f64;
    fn build(f: impl Fn(usize) -> f64) -> Self {
        f(0) as f32
    }
    fn get(self, _: usize) -> f64 {
        self.into()
    }
}

impl Lanes for f64 {
    type E = f64;
    const EPS: f64 = f64::EPSILON;
    fn build(f: impl Fn(usize) -> f64) -> Self {
        f(0)
    }
    fn get(self, _: usize) -> f64 {
        self
    }
}

impl Lanes for F32x4 {
    type E = f32;
    const EPS: f64 = f32::EPSILON as f64;
    fn build(f: impl Fn(usize) -> f64) -> Self {
        Self::from_array(core::array::from_fn(|l| f(l) as f32))
    }
    fn get(self, lane: usize) -> f64 {
        self.lane(lane).into()
    }
}

impl Lanes for F32x8 {
    type E = f32;
    const EPS: f64 = f32::EPSILON as f64;
    fn build(f: impl Fn(usize) -> f64) -> Self {
        Self::from_array(core::array::from_fn(|l| f(l) as f32))
    }
    fn get(self, lane: usize) -> f64 {
        self.lane(lane).into()
    }
}

fn same<S: Lanes>(got: f64, want: f64, what: &str) {
    let tol = 1e-7_f64.max(want.abs() * 4.0 * S::EPS);
    assert!((got - want).abs() <= tol, "{what}: {got} vs {want}");
}

/// Per-lane ADSR scripts at 1 kHz: attack, decay, sustain, release (seconds), then the
/// samples of note on, note off and a second note on (during the release).
const ADSR: [[f64; 7]; 8] = [
    [0.010, 0.020, 0.5, 0.030, 0.0, 40.0, 55.0],
    [0.0, 0.0, 0.7, 0.0, 3.0, 9.0, 12.0],
    [0.025, 0.0104, 0.3, 0.0157, 7.0, 20.0, 30.0],
    [0.001, 0.050, 0.0, 0.010, 1.0, 70.0, 75.0],
    [0.040, 0.010, 1.0, 0.020, 15.0, 30.0, 45.0],
    [0.005, 0.005, 0.2, 0.060, 0.0, 12.0, 40.0],
    [0.0333, 0.0, 0.9, 0.0071, 50.0, 60.0, 64.0],
    [0.002, 0.012, 0.6, 0.0, 99.0, 99.0, 99.0],
];

fn adsr_matches_scalar<S: Lanes>() {
    let param = |s: usize| S::build(|l| ADSR[l][s]);
    let params = AdsrParams::new(param(0), param(1), param(2), param(3));
    let mut pack = Adsr::new(1000.0, params);
    let mut scalar: Vec<Adsr<S::E>> = (0..S::LANES)
        .map(|l| {
            let p = |s: usize| S::E::build(|_| ADSR[l][s]);
            Adsr::new(1000.0, AdsrParams::new(p(0), p(1), p(2), p(3)))
        })
        .collect();
    let mut block = [S::one(); 1];
    for t in 0..120 {
        let at = |step: usize| move |l: usize| ADSR[l][step] == t as f64;
        let (on, off) = (|l| at(4)(l) || at(6)(l), at(5));
        pack.note_on(S::mask(on));
        pack.note_off(S::mask(off));
        // Every 16th sample goes through `apply_to` on a block of ones instead.
        let got = if t % 16 == 7 {
            block[0] = S::one();
            pack.apply_to(&mut block[..1]);
            block[0]
        } else {
            pack.next_sample()
        };
        for (l, env) in scalar.iter_mut().enumerate() {
            env.note_on(S::E::mask(|_| on(l)));
            env.note_off(S::E::mask(|_| off(l)));
            let want = env.next_sample().get(0);
            same::<S>(got.get(l), want, &format!("t={t} lane {l}"));
            assert_eq!(S::bit(pack.is_active(), l), S::E::bit(env.is_active(), 0));
        }
    }
}

/// Per-lane smoothing scripts: initial value, first and second target, and the sample of the
/// second target.
const SMOOTH: [[f64; 4]; 8] = [
    [1.0, 2.0, 0.5, 10.0],
    [0.5, 0.5, 4.0, 3.0],
    [2.0, 0.25, 0.25, 50.0],
    [0.1, 3.0, 1.0, 37.0],
    [1.0, 1.5, 1.5, 0.0],
    [4.0, 0.2, 0.9, 64.0],
    [0.3, 0.6, 0.1, 21.0],
    [0.75, 0.75, 0.75, 5.0],
];

fn smoothed_matches_scalar<S: Lanes, M: Mapping>(control: u32) {
    let col = |s: usize| S::build(|l| SMOOTH[l][s]);
    let mut pack = SmoothedValue::<S, M>::new(col(0)).with_control_rate(control);
    let mut scalar: Vec<SmoothedValue<S::E, M>> = (0..S::LANES)
        .map(|l| SmoothedValue::new(S::E::build(|_| SMOOTH[l][0])).with_control_rate(control))
        .collect();
    pack.reset_steps(40);
    pack.set_target(col(1));
    for (l, v) in scalar.iter_mut().enumerate() {
        v.reset_steps(40);
        v.set_target(S::E::build(|_| SMOOTH[l][1]));
    }
    for t in 0..120 {
        let late = |l: usize| t as f64 >= SMOOTH[l][3];
        pack.set_target(S::build(|l| SMOOTH[l][if late(l) { 2 } else { 1 }]));
        let skip = t % 17 == 5;
        let got = if skip {
            pack.skip(3)
        } else {
            pack.next_value()
        };
        for (l, v) in scalar.iter_mut().enumerate() {
            v.set_target(S::E::build(|_| SMOOTH[l][if late(l) { 2 } else { 1 }]));
            let want = if skip { v.skip(3) } else { v.next_value() };
            same::<S>(
                got.get(l),
                want.get(0),
                &format!("c={control} t={t} lane {l}"),
            );
            assert_eq!(
                S::bit(pack.is_smoothing(), l),
                S::E::bit(v.is_smoothing(), 0)
            );
        }
    }
}

fn matrix<S: Lanes>() {
    adsr_matches_scalar::<S>();
    for control in [1, 16] {
        smoothed_matches_scalar::<S, Linear>(control);
        smoothed_matches_scalar::<S, Multiplicative>(control);
    }
}

#[test]
fn b5_every_type_matches_scalar_lanes_with_per_lane_targets_gates_and_timings() {
    matrix::<f32>();
    matrix::<f64>();
    matrix::<F32x4>();
    matrix::<F32x8>();
}

#[test]
fn b5_f64_and_f32_runs_agree() {
    let (mut a, mut b) = (adsr_of::<f32>(), adsr_of::<f64>());
    for t in 0..200 {
        if t == 120 {
            a.note_off(true);
            b.note_off(true);
        }
        let (x, y) = (a.next_sample(), b.next_sample());
        assert!((f64::from(x) - y).abs() < 1e-5, "t={t}: {x} vs {y}");
    }
}

fn adsr_of<S: Lanes>() -> Adsr<S> {
    let p = |x: f64| S::build(|_| x);
    let mut env = Adsr::new(1000.0, AdsrParams::new(p(0.03), p(0.05), p(0.4), p(0.06)));
    env.note_on(S::mask(|_| true));
    env
}

fn rt_safe<T: RtSafe>() {}

fn exercise<S: Lanes>() {
    let mut env = adsr_of::<S>();
    let mut smooth = SmoothedValue::<S, Multiplicative>::new(S::one()).with_control_rate(16);
    let mut block = [S::one(); 64];
    no_alloc(|| {
        env.set_params(AdsrParams::new(S::zero(), S::one(), S::one(), S::one()));
        env.set_sample_rate(48_000.0);
        env.apply_to(&mut block);
        env.note_off(S::mask(|l| l % 2 == 0));
        env.note_on(S::mask(|l| l % 3 == 0));
        let _ = (
            env.next_sample(),
            env.is_active(),
            env.level(),
            env.params(),
        );
        env.reset();
        smooth.reset(48_000.0, 0.01);
        smooth.set_target(S::build(|l| 2.0 + l as f64));
        smooth.apply_gain(&mut block);
        let _ = (smooth.skip(100), smooth.next_value(), smooth.is_smoothing());
        smooth.set_current_and_target(S::one());
        let _ = gain_to_decibels(0.5_f32, -100.0) + decibels_to_gain(-6.0_f32, -100.0);
    });
    rt_safe::<Adsr<S>>();
    rt_safe::<SmoothedValue<S, Linear>>();
}

#[test]
fn b6_control_types_are_allocation_free_and_rt_safe() {
    exercise::<f32>();
    exercise::<f64>();
    exercise::<F32x4>();
    exercise::<F32x8>();
}

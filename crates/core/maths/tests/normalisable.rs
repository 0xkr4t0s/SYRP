//! B2: `NormalisableRange<T>` for `f32` and `f64`.

use syrp_error::Diagnostic;
use syrp_maths::{MathsError, NormalisableRange};

fn steps() -> impl Iterator<Item = f64> {
    (0..=200).map(|i| f64::from(i) / 200.0)
}

#[test]
fn b2_linear_round_trip_and_clamping() {
    let r = NormalisableRange::new(-12.0_f64, 24.0).unwrap();
    assert_eq!(
        (r.start(), r.end(), r.interval(), r.skew()),
        (-12.0, 24.0, 0.0, 1.0)
    );
    assert_eq!(r.convert_from_0to1(0.25), -3.0);
    assert_eq!(r.convert_to_0to1(-3.0), 0.25);
    assert_eq!(
        (r.convert_to_0to1(-50.0), r.convert_to_0to1(50.0)),
        (0.0, 1.0)
    );
    assert_eq!(
        (r.convert_from_0to1(-0.5), r.convert_from_0to1(1.5)),
        (-12.0, 24.0)
    );
}

#[test]
fn b2_skewed_conversions_are_inverse_within_tolerance() {
    for skew in [0.2, 0.5, 1.0, 2.0, 3.7] {
        for symmetric in [false, true] {
            let r = NormalisableRange::new(20.0_f64, 20_000.0)
                .unwrap()
                .with_skew(skew)
                .unwrap()
                .with_symmetric_skew(symmetric);
            // Strong skews squeeze many positions into a few ulps near the start, so the
            // round trip is exact only to about 1e-10 there.
            for p in steps() {
                let v = r.convert_from_0to1(p);
                assert!(
                    (r.convert_to_0to1(v) - p).abs() < 1e-9,
                    "skew {skew} sym {symmetric} p {p}"
                );
            }
            assert_eq!(r.convert_from_0to1(0.0), 20.0);
            assert!((r.convert_from_0to1(1.0) - 20_000.0).abs() < 1e-9);
        }
    }
    let r = NormalisableRange::new(0.0_f32, 1.0)
        .unwrap()
        .with_skew(0.3)
        .unwrap();
    for p in steps() {
        let p = p as f32;
        assert!((r.convert_to_0to1(r.convert_from_0to1(p)) - p).abs() < 1e-5);
    }
}

#[test]
fn b2_skew_below_one_spends_more_range_on_the_low_end() {
    let r = NormalisableRange::new(0.0_f64, 1.0)
        .unwrap()
        .with_skew(0.5)
        .unwrap();
    // The lower half of the knob covers only the lowest quarter of the values.
    assert!((r.convert_from_0to1(0.5) - 0.25).abs() < 1e-12);
    assert!(r.convert_to_0to1(0.1) > 0.1);
    let high = r.with_skew(2.0).unwrap();
    assert!(high.convert_from_0to1(0.5) > 0.5);
    // Values increase monotonically with the normalised position.
    let mut last = f64::NEG_INFINITY;
    for p in steps() {
        let v = r.convert_from_0to1(p);
        assert!(v >= last);
        last = v;
    }
}

#[test]
fn b2_symmetric_skew_mirrors_about_the_middle() {
    let r = NormalisableRange::new(-1.0_f64, 1.0)
        .unwrap()
        .with_skew(0.5)
        .unwrap()
        .with_symmetric_skew(true);
    assert!(r.convert_from_0to1(0.5).abs() < 1e-12);
    for p in steps() {
        let a = r.convert_from_0to1(p);
        let b = r.convert_from_0to1(1.0 - p);
        assert!((a + b).abs() < 1e-12, "{p}: {a} vs {b}");
    }
    // Skew below 1 spends more of the knob near the centre.
    assert!(r.convert_from_0to1(0.75) < 0.5);
}

#[test]
fn b2_with_centre_maps_half_to_centre() {
    for (s, e, c) in [
        (20.0_f64, 20_000.0, 1_000.0),
        (0.0, 10.0, 9.0),
        (-5.0, 5.0, 0.0),
    ] {
        let r = NormalisableRange::with_centre(s, e, c).unwrap();
        assert!((r.convert_from_0to1(0.5) - c).abs() < 1e-9, "{s} {e} {c}");
        assert!((r.convert_to_0to1(c) - 0.5).abs() < 1e-12);
    }
    let f = NormalisableRange::with_centre(20.0_f32, 20_000.0, 500.0).unwrap();
    assert!((f.convert_from_0to1(0.5) - 500.0).abs() < 0.01);
    assert!(NormalisableRange::with_centre(0.0, 1.0, 1.0).is_err());
    assert!(NormalisableRange::with_centre(0.0, 1.0, -0.1).is_err());
}

#[test]
fn b2_snap_rounds_to_interval_and_clamps() {
    let r = NormalisableRange::new(1.0_f64, 10.0)
        .unwrap()
        .with_interval(2.0)
        .unwrap();
    assert_eq!(r.snap_to_legal_value(3.9), 3.0);
    assert_eq!(r.snap_to_legal_value(4.1), 5.0);
    assert_eq!(r.snap_to_legal_value(-100.0), 1.0);
    // The nearest step above 9 would be 11, which is clamped to the end.
    assert_eq!(r.snap_to_legal_value(10.0), 10.0);
    let continuous = NormalisableRange::new(0.0_f32, 1.0).unwrap();
    assert_eq!(continuous.snap_to_legal_value(0.123), 0.123);
    assert_eq!(continuous.snap_to_legal_value(1.5), 1.0);
}

#[test]
fn b2_custom_functions_replace_builtin_mappings() {
    fn from(s: f64, e: f64, p: f64) -> f64 {
        s * (e / s).powf(p)
    }
    fn to(s: f64, e: f64, v: f64) -> f64 {
        (v / s).ln() / (e / s).ln()
    }
    let r = NormalisableRange::new(1.0_f64, 1_000.0)
        .unwrap()
        .with_skew(0.1)
        .unwrap()
        .with_custom_mapping(from, to);
    assert!((r.convert_from_0to1(1.0 / 3.0) - 10.0).abs() < 1e-9);
    assert!((r.convert_to_0to1(100.0) - 2.0 / 3.0).abs() < 1e-12);
    // The interval snap still applies until a custom snap is set.
    let r = r.with_interval(1.0).unwrap();
    assert_eq!(r.snap_to_legal_value(9.6), 10.0);
    let r = r.with_custom_snap(|_, _, v| if v < 500.0 { 1.0 } else { 1_000.0 });
    assert_eq!(
        (r.snap_to_legal_value(9.6), r.snap_to_legal_value(600.0)),
        (1.0, 1_000.0)
    );
}

#[test]
fn b2_invalid_parameters_are_rejected() {
    let err = NormalisableRange::new(5.0_f32, 5.0).unwrap_err();
    assert_eq!(err, MathsError::InvalidRange);
    assert_eq!(err.code().as_str(), "SYRP-MATHS-0001");
    assert!(NormalisableRange::new(f64::NAN, 1.0).is_err());
    assert!(NormalisableRange::new(0.0, f64::INFINITY).is_err());
    let r = NormalisableRange::new(0.0_f64, 1.0).unwrap();
    assert!(r.with_interval(-0.1).is_err());
    assert!(r.with_skew(-1.0).is_err() && r.with_skew(f64::NAN).is_err());
}

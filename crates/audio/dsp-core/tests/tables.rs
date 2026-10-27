// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Tests for work/specs/WP-2.1-10.md B5: `LookupTable` and `LookupTableTransform`.

use syrp_dsp_core::{DspError, LookupTable, LookupTableTransform};
use syrp_error::Diagnostic;
use syrp_rt::no_alloc;
use syrp_simd::{F32x4, F32x8, F64x2, Sample};
use syrp_testkit::{Tolerance, assert_close};

#[global_allocator]
static ALLOC: syrp_rt::GuardedAllocator = syrp_rt::GuardedAllocator;

/// The linear-interpolation reference: `f` at the neighbouring integers, blended.
fn lerp_reference(f: impl Fn(f64) -> f64, index: f64) -> f64 {
    let i = index.floor();
    f(i) + (f(i + 1.0) - f(i)) * (index - i)
}

#[test]
fn b5_lookup_table_interpolates_linearly_between_points() {
    let f = |i: f64| (i * 0.1).sin() * 3.0;
    let table = LookupTable::<f64>::new(f, 64).unwrap();
    assert_eq!(table.num_points(), 64);
    for k in 0..630 {
        let index = f64::from(k) * 0.1;
        let expected = lerp_reference(f, index);
        assert!(
            (table.get_unchecked(index) - expected).abs() <= 1e-12,
            "index {index}"
        );
    }
    for i in 0..64 {
        assert_eq!(table.get(i as f64), f(i as f64), "exact at the points");
    }
}

#[test]
fn b5_lookup_table_get_clamps_to_the_first_and_last_points() {
    let table = LookupTable::<f32>::new(|i| i * i, 10).unwrap();
    assert_eq!(table.get(-0.5_f32), 0.0);
    assert_eq!(table.get(9.0_f32), 81.0);
    assert_eq!(table.get(9.75_f32), 81.0);
    assert_eq!(table.get(f32::INFINITY), 81.0);
    assert!((table.get_unchecked(8.999_f32) - (64.0 + 17.0 * 0.999)).abs() <= 1e-5);
}

#[test]
fn b5_lookup_reads_are_lane_generic() {
    let table = LookupTable::<f32>::new(|i| (i * 0.3).cos(), 32).unwrap();
    let idx = [0.0_f32, 1.25, 7.5, 30.9, -2.0, 31.0, 15.5, 100.0];
    let packed = table.get(F32x8::from_array(idx)).to_array();
    let scalar: Vec<f32> = idx.iter().map(|&i| table.get(i)).collect();
    assert_close(&packed, &scalar, Tolerance::exact());

    let t64 = LookupTableTransform::<f64>::new(f64::exp, -1.0, 1.0, 256).unwrap();
    let x = F64x2::from_array([-0.3, 0.9]);
    assert_eq!(
        t64.process_sample(x).to_array(),
        [t64.process_sample(-0.3), t64.process_sample(0.9)]
    );
}

#[test]
fn b5_transform_maps_the_input_range_onto_the_table() {
    let t = LookupTableTransform::<f64>::new(f64::tanh, -4.0, 4.0, 801).unwrap();
    // Grid points are exact: x = -4 + i * 0.01.
    for i in [0, 1, 400, 799, 800] {
        let x = -4.0 + f64::from(i) * 0.01;
        assert!((t.process_sample(x) - x.tanh()).abs() <= 1e-12, "x {x}");
    }
    // Between points the error is bounded by the linear-interpolation error h²/8 · max|f''|.
    let bound = 0.01_f64.powi(2) / 8.0 * 0.77;
    for k in 0..8000 {
        let x = -4.0 + f64::from(k) * 0.001 + 0.0003;
        assert!((t.process_sample(x) - x.tanh()).abs() <= bound, "x {x}");
    }
    assert_eq!(t.process_sample(-9.0), t.process_sample(-4.0));
    assert_eq!(t.process_sample(9.0), t.process_sample(4.0));
}

#[test]
fn b5_transform_process_over_slices_matches_process_sample() {
    let t = LookupTableTransform::<f32>::new(|x| x * x * x, -2.0, 2.0, 128).unwrap();
    let input: Vec<F32x4> = (0..16)
        .map(|i| F32x4::from_array([i as f32 * 0.3 - 2.5, 0.0, 1.0, i as f32 * 0.1]))
        .collect();
    let mut out = vec![F32x4::default(); 16];
    let mut out_unchecked = vec![F32x4::default(); 16];
    let clipped: Vec<F32x4> = input
        .iter()
        .map(|v| F32x4::from_array(v.to_array().map(|x| x.clamp(-2.0, 2.0))))
        .collect();
    no_alloc(|| {
        t.process(&input, &mut out);
        t.process_unchecked(&clipped, &mut out_unchecked);
    });
    for ((o, u), i) in out.iter().zip(&out_unchecked).zip(&input) {
        assert_eq!(o, u);
        assert_eq!(o.to_array()[0], t.process_sample(i.to_array()[0]));
    }
}

#[test]
fn b5_max_relative_error_shrinks_with_more_points() {
    let errors: Vec<f64> = [16, 64, 256]
        .iter()
        .map(|&n| {
            LookupTableTransform::<f64>::calculate_max_relative_error(f64::exp, -2.0, 2.0, n, 0)
                .unwrap()
        })
        .collect();
    assert!(errors[0] > errors[1] && errors[1] > errors[2], "{errors:?}");
    // Linear interpolation of exp: relative error about h²/8 with h = 4 / (n - 1).
    let h = 4.0 / 255.0;
    assert!(
        errors[2] <= h * h / 8.0 * 1.01 && errors[2] >= h * h / 8.0 * 0.9,
        "{errors:?}"
    );
    let fixed =
        LookupTableTransform::<f32>::calculate_max_relative_error(|x| x, 0.0, 1.0, 4, 50).unwrap();
    assert!(fixed <= 1e-6, "a straight line is exact: {fixed}");
}

#[test]
fn b5_invalid_parameters_are_coded_errors() {
    let err = LookupTable::<f32>::new(|i| i, 1).unwrap_err();
    assert!(matches!(err, DspError::InvalidTable { num_points: 1, .. }));
    assert_eq!(err.code().as_str(), "SYRP-DSP-0003");
    for (lo, hi) in [
        (1.0, 1.0),
        (2.0, 1.0),
        (f32::NAN, 1.0),
        (0.0, f32::INFINITY),
    ] {
        let err = LookupTableTransform::<f32>::new(|x| x, lo, hi, 16).unwrap_err();
        assert_eq!(err.code().as_str(), "SYRP-DSP-0003", "{lo}..{hi}");
    }
    let mut t = LookupTable::<f64>::new(|i| i, 4).unwrap();
    assert!(t.initialise(|i| -i, 0).is_err());
    assert_eq!(t.get(3.0), 3.0, "a failed initialise changes nothing");
}

#[test]
fn b5_reads_never_allocate() {
    let table = LookupTable::<f32>::new(|i| i, 1024).unwrap();
    let t = LookupTableTransform::<f32>::new(f32::sqrt, 0.0, 4.0, 1024).unwrap();
    let sum = no_alloc(|| {
        (0..1000)
            .map(|k| {
                table.get(k as f32 * 1.5)
                    + t.process_sample(F32x4::splat(k as f32 * 0.01)).to_array()[0]
            })
            .sum::<f32>()
    });
    assert!(sum.is_finite());
}

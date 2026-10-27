// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Tests for work/specs/WP-2.1-10.md B3: `Fft` against a naive DFT, at the `fft` tolerance
//! profile (master plan §9.3: relative RMS error at most 1e-6 for `f32`).

use std::f64::consts::TAU;

use syrp_dsp_core::{Complex, DspError, Fft};
use syrp_error::Diagnostic;
use syrp_rt::no_alloc;
use syrp_testkit::NoiseRng;

#[global_allocator]
static ALLOC: syrp_rt::GuardedAllocator = syrp_rt::GuardedAllocator;

/// The `fft` profile limit for `f32`.
const FFT_F32: f64 = 1e-6;
/// Orders whose full spectrum is checked against the O(N²) DFT; above this, a spread of 97
/// bins is checked so the debug-build test stays fast.
const FULL_DFT_MAX_ORDER: u32 = 10;

type C64 = Complex<f64>;

/// `Σ x[n] e^(∓2πikn/N)` for each `k` in `bins` (no scaling), in `f64`.
fn naive_dft(x: &[C64], bins: &[usize], inverse: bool) -> Vec<C64> {
    let n = x.len();
    let sign = if inverse { 1.0 } else { -1.0 };
    let twiddle: Vec<C64> = (0..n)
        .map(|j| C64::from_polar(1.0, sign * TAU * j as f64 / n as f64))
        .collect();
    bins.iter()
        .map(|&k| {
            x.iter()
                .enumerate()
                .map(|(i, &v)| v * twiddle[(k * i) % n])
                .sum()
        })
        .collect()
}

/// The bins compared at `order`: all of them, or a spread including 0, N/2 and N-1.
fn bins(order: u32) -> Vec<usize> {
    let n = 1_usize << order;
    if order <= FULL_DFT_MAX_ORDER {
        return (0..n).collect();
    }
    let mut b: Vec<usize> = (0..96).map(|i| i * n / 96).collect();
    b.extend([n / 2, n - 1]);
    b
}

/// Relative RMS error of `actual` against `expected`; plain RMS when `expected` is silent.
fn rel_rms(actual: &[C64], expected: &[C64]) -> f64 {
    assert_eq!(actual.len(), expected.len());
    let err: f64 = actual
        .iter()
        .zip(expected)
        .map(|(a, e)| (a - e).norm_sqr())
        .sum();
    let reference: f64 = expected.iter().map(C64::norm_sqr).sum();
    if reference == 0.0 {
        err.sqrt()
    } else {
        (err / reference).sqrt()
    }
}

#[track_caller]
fn assert_fft_profile(actual: &[C64], expected: &[C64], what: &str) {
    let e = rel_rms(actual, expected);
    assert!(
        e <= FFT_F32,
        "{what}: relative RMS error {e:e} above {FFT_F32:e}"
    );
}

/// Test signals of `n` points: an impulse, a sine on a bin, a sine between bins, noise.
fn signals(n: usize, seed: u64) -> Vec<(&'static str, Vec<f32>)> {
    let mut impulse = vec![0.0_f32; n];
    impulse[n / 3] = 1.0;
    let sine = |cycles: f64| -> Vec<f32> {
        (0..n)
            .map(|i| (TAU * cycles * i as f64 / n as f64).sin() as f32)
            .collect()
    };
    let mut noise = vec![0.0_f32; n];
    NoiseRng::new(seed).fill_noise(&mut noise, 1.0);
    vec![
        ("impulse", impulse),
        ("bin sine", sine((n / 4) as f64)),
        ("off-bin sine", sine(n as f64 / 7.3)),
        ("noise", noise),
    ]
}

fn widen(x: &[Complex<f32>]) -> Vec<C64> {
    x.iter()
        .map(|c| C64::new(c.re.into(), c.im.into()))
        .collect()
}

#[test]
fn b3_order_above_the_limit_is_a_coded_error() {
    let err = Fft::new(Fft::MAX_ORDER + 1).unwrap_err();
    assert_eq!(
        err,
        Fft::new(Fft::MAX_ORDER + 1).unwrap_err(),
        "the error is deterministic"
    );
    assert!(matches!(
        err,
        DspError::OrderTooLarge {
            order: 21,
            max: 20,
            ..
        }
    ));
    assert_eq!(err.code().as_str(), "SYRP-DSP-0002");
    assert_eq!(Fft::new(14).unwrap().size(), 16_384);
}

#[test]
fn b3_complex_perform_matches_naive_dft_for_every_order() {
    for order in 0..=14 {
        let mut fft = Fft::new(order).unwrap();
        let n = fft.size();
        let b = bins(order);
        for (name, re) in signals(n, u64::from(order)) {
            // A complex input: the signal plus a reversed copy as the imaginary part.
            let x: Vec<Complex<f32>> = re
                .iter()
                .zip(re.iter().rev())
                .map(|(&r, &i)| Complex::new(r, 0.5 * i))
                .collect();
            let mut out = vec![Complex::default(); n];
            fft.perform(&x, &mut out, false);
            let got: Vec<C64> = b.iter().map(|&k| widen(&out[k..=k])[0]).collect();
            assert_fft_profile(
                &got,
                &naive_dft(&widen(&x), &b, false),
                &format!("forward {name}, order {order}"),
            );

            let mut back = vec![Complex::default(); n];
            fft.perform(&x, &mut back, true);
            let got: Vec<C64> = b.iter().map(|&k| widen(&back[k..=k])[0]).collect();
            let expected: Vec<C64> = naive_dft(&widen(&x), &b, true)
                .into_iter()
                .map(|c| c / n as f64)
                .collect();
            assert_fft_profile(&got, &expected, &format!("inverse {name}, order {order}"));
        }
    }
}

#[test]
fn b3_real_forward_matches_naive_dft_with_interleaved_layout_for_every_order() {
    for order in 0..=14 {
        let mut fft = Fft::new(order).unwrap();
        let n = fft.size();
        let b = bins(order);
        for (name, x) in signals(n, 100 + u64::from(order)) {
            for hint in [false, true] {
                let mut data = vec![0.0_f32; 2 * n];
                data[..n].copy_from_slice(&x);
                fft.perform_real_only_forward_transform(&mut data, hint);
                // Interleaved re, im per bin; the negative bins are present either way.
                let got: Vec<C64> = b
                    .iter()
                    .map(|&k| C64::new(data[2 * k].into(), data[2 * k + 1].into()))
                    .collect();
                let reference: Vec<C64> = x.iter().map(|&v| C64::new(v.into(), 0.0)).collect();
                assert_fft_profile(
                    &got,
                    &naive_dft(&reference, &b, false),
                    &format!("real {name}, order {order}"),
                );
            }
        }
    }
}

#[test]
fn b3_round_trips_return_the_input_for_every_order() {
    for order in 0..=14 {
        let mut fft = Fft::new(order).unwrap();
        let n = fft.size();
        for (name, x) in signals(n, 200 + u64::from(order)) {
            let mut data = vec![0.0_f32; 2 * n];
            data[..n].copy_from_slice(&x);
            fft.perform_real_only_forward_transform(&mut data, true);
            fft.perform_real_only_inverse_transform(&mut data);
            let to_c =
                |v: &[f32]| -> Vec<C64> { v.iter().map(|&s| C64::new(s.into(), 0.0)).collect() };
            assert_fft_profile(
                &to_c(&data[..n]),
                &to_c(&x),
                &format!("real round trip {name}, order {order}"),
            );

            let cx: Vec<Complex<f32>> = x.iter().map(|&s| Complex::new(s, -s)).collect();
            let (mut spec, mut back) = (vec![Complex::default(); n], vec![Complex::default(); n]);
            fft.perform(&cx, &mut spec, false);
            fft.perform(&spec, &mut back, true);
            assert_fft_profile(
                &widen(&back),
                &widen(&cx),
                &format!("complex round trip {name}, order {order}"),
            );
        }
    }
}

#[test]
fn b3_inverse_reads_only_the_non_negative_bins() {
    let mut fft = Fft::new(4).unwrap();
    let n = fft.size();
    let (_, x) = signals(n, 7).pop().unwrap();
    let mut data = vec![0.0_f32; 2 * n];
    data[..n].copy_from_slice(&x);
    fft.perform_real_only_forward_transform(&mut data, false);
    // Garbage in the negative bins and in the DC and Nyquist imaginary parts is ignored.
    data[2 * (n / 2 + 1)..].fill(123.0);
    data[1] = 9.0;
    data[n + 1] = -9.0;
    fft.perform_real_only_inverse_transform(&mut data);
    for (a, e) in data[..n].iter().zip(&x) {
        assert!((a - e).abs() <= 1e-6, "{a} vs {e}");
    }
}

#[test]
fn b3_frequency_only_transform_gives_bin_magnitudes() {
    let mut fft = Fft::new(6).unwrap();
    let n = fft.size();
    let (_, x) = signals(n, 9).pop().unwrap();
    let reference: Vec<C64> = x.iter().map(|&v| C64::new(v.into(), 0.0)).collect();
    let all: Vec<usize> = (0..n).collect();
    let expected: Vec<C64> = naive_dft(&reference, &all, false)
        .iter()
        .map(|c| C64::new(c.norm(), 0.0))
        .collect();
    for ignore_negative in [false, true] {
        let mut data = vec![5.0_f32; 2 * n];
        data[..n].copy_from_slice(&x);
        fft.perform_frequency_only_forward_transform(&mut data, ignore_negative);
        let keep = if ignore_negative { n / 2 + 1 } else { n };
        let got: Vec<C64> = data[..keep]
            .iter()
            .map(|&m| C64::new(m.into(), 0.0))
            .collect();
        assert_fft_profile(&got, &expected[..keep], "magnitudes");
        assert!(data[keep..].iter().all(|&v| v == 0.0), "the rest is zeroed");
    }
}

#[test]
fn b3_scaling_forward_unscaled_inverse_one_over_n_syrp_choice() {
    let mut fft = Fft::new(3).unwrap();
    let mut data = [1.0_f32; 16];
    fft.perform_real_only_forward_transform(&mut data, false);
    assert_eq!(
        data[0], 8.0,
        "forward DC of eight ones is N, not 1 or sqrt(N)"
    );
    let mut data = [0.0_f32; 16];
    data[0] = 8.0;
    fft.perform_real_only_inverse_transform(&mut data);
    assert_eq!(data[..8], [1.0; 8], "inverse is scaled by 1/N");
    let input = [Complex::new(8.0_f32, 0.0); 8];
    let mut out = [Complex::default(); 8];
    fft.perform(&input, &mut out, true);
    assert_eq!(
        out[0],
        Complex::new(8.0, 0.0),
        "complex inverse of a flat spectrum is 8 at 0"
    );
    assert!(out[1..].iter().all(|c| c.norm() < 1e-6));
}

#[test]
fn b3_perform_calls_never_allocate() {
    let mut fft = Fft::new(10).unwrap();
    let n = fft.size();
    let mut data = vec![0.25_f32; 2 * n];
    let input = vec![Complex::new(0.5_f32, 0.0); n];
    let mut output = vec![Complex::default(); n];
    no_alloc(|| {
        fft.perform(&input, &mut output, false);
        fft.perform(&input, &mut output, true);
        fft.perform_real_only_forward_transform(&mut data, false);
        fft.perform_real_only_inverse_transform(&mut data);
        fft.perform_frequency_only_forward_transform(&mut data, true);
    });
    assert!(data.iter().all(|v| v.is_finite()));
}

#[test]
#[should_panic(expected = "needs 16 values")]
fn b3_real_transforms_need_two_times_size_values() {
    let mut fft = Fft::new(3).unwrap();
    fft.perform_real_only_forward_transform(&mut [0.0; 15], false);
}

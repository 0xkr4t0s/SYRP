// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Tests for work/specs/WP-2.1-10.md B4: `WindowingFunction` against the closed forms.

use std::f64::consts::{PI, TAU};

use syrp_dsp_core::{WindowingFunction, WindowingMethod};
use syrp_rt::no_alloc;
use syrp_testkit::{Tolerance, assert_close};

#[global_allocator]
static ALLOC: syrp_rt::GuardedAllocator = syrp_rt::GuardedAllocator;

/// `I0(x) = (1/π) ∫₀^π e^(x cos t) dt` by the trapezoid rule, which converges geometrically
/// for this periodic integrand; an independent check on the series in `special`.
fn i0_by_integral(x: f64) -> f64 {
    let steps = 4000;
    let h = PI / f64::from(steps);
    let f = |t: f64| (x * t.cos()).exp();
    let inner: f64 = (1..steps).map(|k| f(f64::from(k) * h)).sum();
    (inner + 0.5 * (f(0.0) + f(PI))) * h / PI
}

/// The closed form of point `i` of an `n`-point window, written out here independently.
fn closed_form(method: WindowingMethod, i: usize, n: usize, beta: f64) -> f64 {
    if n == 1 {
        return 1.0;
    }
    let m = (n - 1) as f64;
    let i = i as f64;
    let cos = |k: f64| (TAU * k * i / m).cos();
    match method {
        WindowingMethod::Rectangular => 1.0,
        WindowingMethod::Triangular => 1.0 - ((i - m / 2.0) / (m / 2.0)).abs(),
        WindowingMethod::Hann => 0.5 * (1.0 - cos(1.0)),
        WindowingMethod::Hamming => 0.54 - 0.46 * cos(1.0),
        WindowingMethod::Blackman => 0.42 - 0.5 * cos(1.0) + 0.08 * cos(2.0),
        WindowingMethod::BlackmanHarris => {
            0.35875 - 0.48829 * cos(1.0) + 0.14128 * cos(2.0) - 0.01168 * cos(3.0)
        }
        WindowingMethod::FlatTop => {
            0.21557895 - 0.41663158 * cos(1.0) + 0.277263158 * cos(2.0) - 0.083578947 * cos(3.0)
                + 0.006947368 * cos(4.0)
        }
        WindowingMethod::Kaiser => {
            let r = 2.0 * i / m - 1.0;
            i0_by_integral(beta * (1.0 - r * r).sqrt()) / i0_by_integral(beta)
        }
        _ => unreachable!("every method is listed"),
    }
}

const METHODS: [WindowingMethod; 8] = [
    WindowingMethod::Rectangular,
    WindowingMethod::Triangular,
    WindowingMethod::Hann,
    WindowingMethod::Hamming,
    WindowingMethod::Blackman,
    WindowingMethod::BlackmanHarris,
    WindowingMethod::FlatTop,
    WindowingMethod::Kaiser,
];

#[test]
fn b4_every_window_matches_its_closed_form() {
    for method in METHODS {
        for beta in [0.0, 4.0, 8.6] {
            for n in [1, 2, 3, 64, 1023] {
                let expected: Vec<f64> = (0..n).map(|i| closed_form(method, i, n, beta)).collect();
                let w64 = WindowingFunction::<f64>::new(n, method, false, beta).unwrap();
                for (i, (a, e)) in w64.table().iter().zip(&expected).enumerate() {
                    assert!(
                        (a - e).abs() <= 1e-12,
                        "{method:?} n={n} beta={beta} i={i}: {a} vs {e}"
                    );
                }
                let w32 = WindowingFunction::<f32>::new(n, method, false, beta as f32).unwrap();
                let expected32: Vec<f32> = expected.iter().map(|&v| v as f32).collect();
                assert_close(w32.table(), &expected32, Tolerance::max_abs(1e-7));
            }
        }
    }
}

#[test]
fn b4_every_window_is_symmetric() {
    for method in METHODS {
        for n in [2, 7, 256, 1001] {
            let w = WindowingFunction::<f64>::new(n, method, true, 6.0).unwrap();
            let t = w.table();
            for i in 0..n / 2 {
                assert!(
                    (t[i] - t[n - 1 - i]).abs() <= 1e-15,
                    "{method:?} n={n} i={i}"
                );
            }
        }
    }
}

#[test]
fn b4_normalise_gives_a_dc_response_of_one() {
    for method in METHODS {
        let w = WindowingFunction::<f64>::new(128, method, true, 5.0).unwrap();
        let sum: f64 = w.table().iter().sum();
        assert!((sum - 1.0).abs() <= 1e-12, "{method:?}: sum {sum}");
        let raw = WindowingFunction::<f64>::new(128, method, false, 5.0).unwrap();
        let ratio = raw.table()[64] / w.table()[64];
        assert!(
            (ratio - raw.table().iter().sum::<f64>()).abs() <= 1e-9,
            "only a scale"
        );
    }
}

#[test]
fn b4_kaiser_with_zero_beta_is_rectangular() {
    let k = WindowingFunction::<f32>::new(16, WindowingMethod::Kaiser, false, 0.0).unwrap();
    assert_close(k.table(), &[1.0_f32; 16], Tolerance::exact());
}

#[test]
fn b4_multiply_and_fill_into_do_not_allocate() {
    let mut w =
        WindowingFunction::<f32>::new(512, WindowingMethod::BlackmanHarris, false, 0.0).unwrap();
    let mut samples = vec![2.0_f32; 600];
    let mut table = vec![0.0_f32; 512];
    no_alloc(|| {
        w.multiply_with_windowing_table(&mut samples);
        WindowingFunction::fill_windowing_tables_into(
            &mut table,
            WindowingMethod::BlackmanHarris,
            false,
            0.0,
        );
        // Refilling at the same or a smaller size reuses the table.
        w.fill_windowing_tables(256, WindowingMethod::Hann, false, 0.0)
            .unwrap();
    });
    let expected =
        WindowingFunction::<f32>::new(512, WindowingMethod::BlackmanHarris, false, 0.0).unwrap();
    let doubled: Vec<f32> = expected.table().iter().map(|v| v * 2.0).collect();
    assert_close(&samples[..512], &doubled, Tolerance::exact());
    assert_eq!(
        samples[512..],
        [2.0; 88],
        "beyond the table nothing changes"
    );
    assert_close(&table, expected.table(), Tolerance::exact());
    assert_eq!(w.size(), 256);
}

#[test]
fn b4_empty_window_is_allowed() {
    let w = WindowingFunction::<f64>::new(0, WindowingMethod::Hann, true, 0.0).unwrap();
    assert!(w.table().is_empty());
}

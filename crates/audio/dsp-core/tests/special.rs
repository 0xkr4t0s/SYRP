// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Tests for work/specs/WP-2.1-10.md B6 (special functions): `special` against independent
//! numerical references (quadrature and ODE integration).

use std::f64::consts::FRAC_PI_2;

use syrp_dsp_core::Complex;
use syrp_dsp_core::special::{asne, bessel_i0, cde, elliptic_integral_k, sne};
use syrp_rt::no_alloc;

#[global_allocator]
static ALLOC: syrp_rt::GuardedAllocator = syrp_rt::GuardedAllocator;

const MODULI: [f64; 5] = [0.0, 0.3, 0.7, 0.95, 0.999];

/// `K(k) = ∫₀^{π/2} dθ / √(1 - k² sin²θ)` by composite Simpson's rule.
fn k_by_quadrature(k: f64) -> f64 {
    let n = 20_000;
    let h = FRAC_PI_2 / f64::from(n);
    let f = |t: f64| 1.0 / (1.0 - k * k * t.sin().powi(2)).sqrt();
    let inner: f64 = (1..n)
        .map(|i| f(f64::from(i) * h) * if i % 2 == 1 { 4.0 } else { 2.0 })
        .sum();
    (f(0.0) + inner + f(FRAC_PI_2)) * h / 3.0
}

/// `(sn, cn, dn)(x, k)` by RK4 on `sn' = cn dn`, `cn' = -sn dn`, `dn' = -k² sn cn`.
fn jacobi_by_ode(x: f64, k: f64) -> (f64, f64, f64) {
    let steps = 40_000;
    let h = x / f64::from(steps);
    let f = |[s, c, d]: [f64; 3]| [c * d, -s * d, -k * k * s * c];
    let mut y = [0.0, 1.0, 1.0];
    let add =
        |y: [f64; 3], k: [f64; 3], a: f64| [y[0] + a * k[0], y[1] + a * k[1], y[2] + a * k[2]];
    for _ in 0..steps {
        let k1 = f(y);
        let k2 = f(add(y, k1, h / 2.0));
        let k3 = f(add(y, k2, h / 2.0));
        let k4 = f(add(y, k3, h));
        y = [0, 1, 2].map(|i| y[i] + h / 6.0 * (k1[i] + 2.0 * k2[i] + 2.0 * k3[i] + k4[i]));
    }
    (y[0], y[1], y[2])
}

fn real(x: f64) -> Complex<f64> {
    Complex::new(x, 0.0)
}

#[test]
fn b6_bessel_i0_matches_published_values() {
    // Values from the power series evaluated to 20 digits (Abramowitz and Stegun table 9.8).
    for (x, expected) in [
        (0.5, 1.063_483_370_741_323_5),
        (2.0, 2.279_585_302_336_067_3),
        (5.0, 27.239_871_823_604_442),
        (10.0, 2_815.716_628_466_254),
    ] {
        assert!((bessel_i0(x) / expected - 1.0).abs() <= 1e-14, "I0({x})");
    }
}

#[test]
fn b6_elliptic_k_matches_quadrature() {
    for k in MODULI {
        let (big_k, big_kp) = elliptic_integral_k(k);
        assert!(
            (big_k - k_by_quadrature(k)).abs() <= 1e-10 * big_k,
            "K({k})"
        );
        if k > 0.0 {
            let kc = (1.0 - k * k).sqrt();
            assert!(
                (big_kp - k_by_quadrature(kc)).abs() <= 1e-10 * big_kp,
                "K'({k})"
            );
        }
    }
    assert!(elliptic_integral_k(1.0).0.is_infinite());
}

#[test]
fn b6_sne_and_cde_match_the_jacobi_ode_on_the_real_axis() {
    for k in MODULI {
        let (big_k, _) = elliptic_integral_k(k);
        for u in [0.0, 0.1, 0.37, 0.5, 0.8, 1.0, 1.6, 2.5] {
            let (s, c, d) = jacobi_by_ode(u * big_k, k);
            let sn = sne(real(u), k);
            let cd = cde(real(u), k);
            assert!(
                (sn.re - s).abs() <= 1e-9 && sn.im.abs() <= 1e-12,
                "sn({u}K, {k}): {sn} vs {s}"
            );
            assert!(
                (cd.re - c / d).abs() <= 1e-9,
                "cd({u}K, {k}): {cd} vs {}",
                c / d
            );
        }
    }
    for u in [0.0, 0.3, 1.0] {
        assert!(
            (cde(real(u), 0.0).re - (u * FRAC_PI_2).cos()).abs() <= 1e-15,
            "k = 0 is cos"
        );
    }
}

#[test]
fn b6_sne_on_the_imaginary_axis_follows_jacobis_imaginary_transform() {
    // sn(i y, k) = i sc(y, k') with k' = √(1 - k²).
    for k in [0.3, 0.7, 0.95] {
        let (big_k, _) = elliptic_integral_k(k);
        let kc = (1.0 - k * k).sqrt();
        for t in [0.1, 0.4, 0.7] {
            let (s, c, _) = jacobi_by_ode(t * big_k, kc);
            let sn = sne(Complex::new(0.0, t), k);
            assert!(
                sn.re.abs() <= 1e-9 && (sn.im - s / c).abs() <= 1e-8,
                "sn(i{t}K, {k}): {sn}"
            );
        }
    }
}

#[test]
fn b6_asne_inverts_sne() {
    // Inside the fundamental region: real part in -1..=1, imaginary part within K'/K (sn has
    // the imaginary period 2iK', so further out asne returns an equivalent point).
    for k in MODULI {
        let (big_k, big_kp) = elliptic_integral_k(k);
        let ratio = if big_kp.is_finite() {
            big_kp / big_k
        } else {
            1.0
        };
        for u in [
            real(0.0),
            real(0.2),
            real(0.75),
            Complex::new(0.3, 0.3 * ratio),
            Complex::new(0.6, -0.5 * ratio),
            Complex::new(-0.1, 0.9 * ratio),
        ] {
            let back = asne(sne(u, k), k);
            assert!((back - u).norm() <= 1e-10, "asne(sne({u}, {k})) = {back}");
        }
    }
    assert!(
        (asne(real(1.0), 0.6) - real(1.0)).norm() <= 1e-7,
        "sn(K) = 1"
    );
}

#[test]
fn b6_special_functions_never_allocate() {
    let v = no_alloc(|| {
        let (k, kp) = elliptic_integral_k(0.9);
        let w = sne(Complex::new(0.4, 0.1), 0.9) + cde(real(0.2), 0.5);
        k + kp + w.re + asne(w, 0.9).re + bessel_i0(3.0)
    });
    assert!(v.is_finite());
}

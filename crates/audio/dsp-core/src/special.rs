// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Special functions (JUCE's `dsp::SpecialFunctions`): the Bessel `I0` of the Kaiser window
//! and the elliptic helpers that elliptic filter design needs.
//!
//! The elliptic functions use quarter-period units: `u = 1` is the quarter period `K(k)`, so
//! `sne(u, k)` is `sn(u K(k), k)`. They are computed with the descending Landen
//! transformation, which converges to a few ULP of `f64` in at most a handful of steps for
//! moduli `k` in `0..1`. None of them allocates.

use std::f64::consts::{FRAC_2_PI, FRAC_PI_2};

use rustfft::num_complex::Complex;

/// The most Landen steps taken; moduli below `1 - 1e-12` need far fewer.
const MAX_LANDEN: usize = 16;

/// The descending Landen moduli `k1, k2, ..` of `k`, each `(k / (1 + √(1 - k²)))²` of the
/// one before, until they underflow below `1e-17`. Returns the array and how many are used.
fn landen(k: f64) -> ([f64; MAX_LANDEN], usize) {
    let mut v = [0.0; MAX_LANDEN];
    let mut n = 0;
    let mut kn = k;
    while n < MAX_LANDEN && kn > 1e-17 {
        kn = (kn / (1.0 + (1.0 - kn * kn).sqrt())).powi(2);
        v[n] = kn;
        n += 1;
    }
    (v, n)
}

/// The modified Bessel function of the first kind and order zero, `I0(x)`, which the Kaiser
/// window is built on (JUCE's `SpecialFunctions::besselI0`).
///
/// Summed from its power series `Σ ((x/2)^k / k!)²` until a term no longer changes the sum,
/// in `f64`. `I0` is even, `I0(0) = 1`, and it overflows to infinity above about `|x| = 713`.
///
/// ```
/// use syrp_dsp_core::special::bessel_i0;
/// assert_eq!(bessel_i0(0.0), 1.0);
/// assert!((bessel_i0(1.0) - 1.266_065_877_752_008_4).abs() < 1e-15);
/// assert_eq!(bessel_i0(-2.5), bessel_i0(2.5));
/// ```
#[doc(alias = "besselI0")]
#[must_use]
pub fn bessel_i0(x: f64) -> f64 {
    let half_sq = 0.25 * x * x;
    let mut term = 1.0;
    let mut sum = 1.0;
    let mut k = 1.0;
    loop {
        term *= half_sq / (k * k);
        let next = sum + term;
        if next == sum || !next.is_finite() {
            return next;
        }
        sum = next;
        k += 1.0;
    }
}

/// The complete elliptic integrals of the first kind `(K(k), K'(k))`, where
/// `K'(k) = K(√(1 - k²))` is the integral for the complementary modulus (JUCE's
/// `SpecialFunctions::ellipticIntegralK`). Computed from the arithmetic-geometric mean,
/// `K(k) = π / (2 agm(1, √(1 - k²)))`. `k` must be in `0..=1`; `K(1)` is infinite.
///
/// ```
/// use syrp_dsp_core::special::elliptic_integral_k;
/// let (k, kp) = elliptic_integral_k(0.0);
/// assert!((k - std::f64::consts::FRAC_PI_2).abs() < 1e-15);
/// assert!(kp.is_infinite());
/// ```
#[doc(alias = "ellipticIntegralK")]
#[must_use]
pub fn elliptic_integral_k(k: f64) -> (f64, f64) {
    let kc = (1.0 - k * k).max(0.0).sqrt();
    (quarter_period(kc), quarter_period(k))
}

/// `K` of the modulus whose complement is `kc`: `π / (2 agm(1, kc))`.
fn quarter_period(kc: f64) -> f64 {
    if kc <= 0.0 {
        return f64::INFINITY;
    }
    let (mut a, mut b) = (1.0_f64, kc);
    for _ in 0..64 {
        if (a - b).abs() <= 1e-16 * a {
            break;
        }
        (a, b) = (0.5 * (a + b), (a * b).sqrt());
    }
    FRAC_PI_2 / a
}

/// The Jacobian elliptic function `cd(u K(k), k)` for complex `u` in quarter-period units
/// (JUCE's `SpecialFunctions::cde`). `cd(0) = 1`, `cd(K) = 0`, and for `k = 0` it is
/// `cos(u π/2)`.
///
/// ```
/// use syrp_dsp_core::Complex;
/// use syrp_dsp_core::special::cde;
/// assert!((cde(Complex::new(0.0, 0.0), 0.8).re - 1.0).abs() < 1e-15);
/// assert!(cde(Complex::new(1.0, 0.0), 0.8).norm() < 1e-15);
/// ```
#[must_use]
pub fn cde(u: Complex<f64>, k: f64) -> Complex<f64> {
    let (v, n) = landen(k);
    let mut w = (u * FRAC_PI_2).cos();
    for &kn in v[..n].iter().rev() {
        w = w * (1.0 + kn) / (w * w * kn + 1.0);
    }
    w
}

/// The Jacobian elliptic function `sn(u K(k), k)` for complex `u` in quarter-period units
/// (JUCE's `SpecialFunctions::sne`); `sn(x) = cd(K - x)`.
///
/// ```
/// use syrp_dsp_core::Complex;
/// use syrp_dsp_core::special::sne;
/// assert!((sne(Complex::new(1.0, 0.0), 0.5).re - 1.0).abs() < 1e-15);
/// ```
#[must_use]
pub fn sne(u: Complex<f64>, k: f64) -> Complex<f64> {
    cde(Complex::new(1.0, 0.0) - u, k)
}

/// The inverse of [`sne`]: the `u`, in quarter-period units, with `sn(u K(k), k) = w`
/// (JUCE's `SpecialFunctions::asne`), by the ascending Landen transformation and the
/// principal complex `asin`. `sn` has periods `4K` and `2iK'`, so this returns the
/// principal value: real part in `-1..=1` and imaginary part within `K'/K`.
///
/// ```
/// use syrp_dsp_core::Complex;
/// use syrp_dsp_core::special::{asne, sne};
/// let u = Complex::new(0.3, 0.2);
/// assert!((asne(sne(u, 0.9), 0.9) - u).norm() < 1e-12);
/// ```
#[must_use]
pub fn asne(w: Complex<f64>, k: f64) -> Complex<f64> {
    let (v, n) = landen(k);
    let mut w = w;
    let mut prev = k;
    for &kn in &v[..n] {
        let root = (Complex::new(1.0, 0.0) - w * w * (prev * prev)).sqrt();
        w = w * 2.0 / ((root + 1.0) * (1.0 + kn));
        prev = kn;
    }
    w.asin() * FRAC_2_PI
}

// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Special functions (JUCE's `dsp::SpecialFunctions`).

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

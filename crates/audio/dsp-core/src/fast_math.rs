// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`FastMathApproximations`]: rational approximations of elementary functions.

use crate::{DspFloat, DspSample};

/// `value` in every lane.
#[inline(always)]
fn c<S: DspSample<Element: DspFloat>>(value: f64) -> S {
    S::splat(S::Element::from_f64(value))
}

/// `Σ coeffs[i] * y^i` by Horner's rule.
#[inline(always)]
fn poly<S: DspSample<Element: DspFloat>>(y: S, coeffs: &[f64]) -> S {
    coeffs
        .iter()
        .rev()
        .fold(S::zero(), |acc, &k| acc * y + c(k))
}

/// Fast approximations of `sin`, `cos`, `tan`, `sinh`, `cosh`, `tanh`, `exp` and
/// `ln(1 + x)` (JUCE's `dsp::FastMathApproximations`).
///
/// Each is a Padé approximant: a ratio of two polynomials, built only from `+`, `-`, `*` and
/// `/`, so it is lane-generic and branch-free. The coefficients are Syrp's own, derived from
/// the Taylor series (tan and tanh from Lambert's continued fraction). Each function is
/// accurate only over the input range the JUCE documentation recommends, given on each
/// function with the maximum errors measured over 20,001 evenly spaced inputs in that range
/// (in `f64`; `f32` adds its own rounding of about `1e-7` relative). Outside the range the
/// error grows quickly. Nothing here allocates.
///
/// ```
/// use syrp_dsp_core::FastMathApproximations as Fast;
/// use syrp_simd::F32x4;
/// assert!((Fast::sin(1.0_f64) - 1.0_f64.sin()).abs() < 2e-5);
/// let y = Fast::tanh(F32x4::from_array([-5.0, -1.0, 0.0, 2.0]));
/// assert!((y.to_array()[1] - (-1.0_f32).tanh()).abs() < 1e-4);
/// ```
#[doc(alias = "dsp::FastMathApproximations")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FastMathApproximations;

impl FastMathApproximations {
    /// `sin(x)`, Padé `[7/6]`, for `x` in `-π..=π`: max error `1.2e-5` absolute.
    ///
    /// ```
    /// use syrp_dsp_core::FastMathApproximations as Fast;
    /// assert_eq!(Fast::sin(0.0_f32), 0.0);
    /// ```
    #[inline(always)]
    #[must_use]
    pub fn sin<S: DspSample<Element: DspFloat>>(x: S) -> S {
        let y = x * x;
        let num = poly(
            y,
            &[
                1.0,
                -0.142_523_454_506_925_58,
                0.004_585_515_911_586_535_5,
                -4.163_277_313_164_616e-5,
            ],
        );
        let den = poly(
            y,
            &[
                1.0,
                0.024_143_212_159_741_086,
                0.000_276_051_271_543_382_75,
                1.595_035_873_773_664_8e-6,
            ],
        );
        x * num / den
    }

    /// `cos(x)`, Padé `[6/6]`, for `x` in `-π..=π`: max error `7.4e-5` absolute.
    ///
    /// ```
    /// use syrp_dsp_core::FastMathApproximations as Fast;
    /// assert_eq!(Fast::cos(0.0_f64), 1.0);
    /// ```
    #[inline(always)]
    #[must_use]
    pub fn cos<S: DspSample<Element: DspFloat>>(x: S) -> S {
        let y = x * x;
        let num = poly(
            y,
            &[
                1.0,
                -0.470_595_788_392_398_6,
                0.027_388_289_676_425_27,
                -0.000_372_342_268_528_709_2,
            ],
        );
        let den = poly(
            y,
            &[
                1.0,
                0.029_404_211_607_601_437,
                0.000_423_728_813_559_322,
                3.235_543_489_780_778e-6,
            ],
        );
        num / den
    }

    /// `tan(x)`, Lambert `[7/6]`, for `x` in `-π/2..π/2`: max error `1.7e-6` relative,
    /// measured over `|x| <= 0.999 π/2` (tan itself has poles at the ends).
    ///
    /// ```
    /// use syrp_dsp_core::FastMathApproximations as Fast;
    /// assert!((Fast::tan(0.5_f64) - 0.5_f64.tan()).abs() < 1e-9);
    /// ```
    #[inline(always)]
    #[must_use]
    pub fn tan<S: DspSample<Element: DspFloat>>(x: S) -> S {
        let y = x * x;
        let num = poly(y, &[135_135.0, -17_325.0, 378.0, -1.0]);
        let den = poly(y, &[135_135.0, -62_370.0, 3_150.0, -28.0]);
        x * num / den
    }

    /// `sinh(x)`, Padé `[9/8]`, for `x` in `-5..=5`: max error `3.1e-6` relative.
    ///
    /// ```
    /// use syrp_dsp_core::FastMathApproximations as Fast;
    /// assert!((Fast::sinh(2.0_f64) / 2.0_f64.sinh() - 1.0).abs() < 1e-5);
    /// ```
    #[inline(always)]
    #[must_use]
    pub fn sinh<S: DspSample<Element: DspFloat>>(x: S) -> S {
        let y = x * x;
        let num = poly(
            y,
            &[
                1.0,
                0.147_623_113_115_217_8,
                0.005_336_955_632_703_259,
                6.831_227_380_145_156e-5,
                2.938_721_919_175_267e-7,
            ],
        );
        let den = poly(
            y,
            &[
                1.0,
                -0.019_043_553_551_448_86,
                0.000_177_547_891_278_069_05,
                -9.954_602_288_511_775e-7,
                2.967_394_519_919_980_5e-9,
            ],
        );
        x * num / den
    }

    /// `cosh(x)`, Padé `[8/8]`, for `x` in `-5..=5`: max error `2.0e-5` relative.
    ///
    /// ```
    /// use syrp_dsp_core::FastMathApproximations as Fast;
    /// assert_eq!(Fast::cosh(0.0_f32), 1.0);
    /// ```
    #[inline(always)]
    #[must_use]
    pub fn cosh<S: DspSample<Element: DspFloat>>(x: S) -> S {
        let y = x * x;
        let num = poly(
            y,
            &[
                1.0,
                0.477_862_206_485_004_44,
                0.030_842_435_054_370_66,
                0.000_587_146_544_419_631_5,
                3.421_843_486_182_193e-6,
            ],
        );
        let den = poly(
            y,
            &[
                1.0,
                -0.022_137_793_514_995_566,
                0.000_244_665_145_201_774_57,
                -1.666_853_945_329_390_1e-6,
                6.237_544_679_486_808e-9,
            ],
        );
        num / den
    }

    /// `tanh(x)`, Lambert `[7/6]`, for `x` in `-5..=5`: max error `1.1e-4` absolute.
    ///
    /// ```
    /// use syrp_dsp_core::FastMathApproximations as Fast;
    /// assert!((Fast::tanh(1.0_f64) - 1.0_f64.tanh()).abs() < 1e-6);
    /// ```
    #[inline(always)]
    #[must_use]
    pub fn tanh<S: DspSample<Element: DspFloat>>(x: S) -> S {
        let y = x * x;
        let num = poly(y, &[135_135.0, 17_325.0, 378.0, 1.0]);
        let den = poly(y, &[135_135.0, 62_370.0, 3_150.0, 28.0]);
        x * num / den
    }

    /// `exp(x)`, Padé `[6/6]`, for `x` in `-6..=4`: max error `4.6e-3` relative (at the
    /// ends of the range; under `1e-6` for `|x| <= 1.5`).
    ///
    /// ```
    /// use syrp_dsp_core::FastMathApproximations as Fast;
    /// assert_eq!(Fast::exp(0.0_f64), 1.0);
    /// ```
    #[inline(always)]
    #[must_use]
    pub fn exp<S: DspSample<Element: DspFloat>>(x: S) -> S {
        const K: [f64; 7] = [
            1.0,
            0.5,
            0.113_636_363_636_363_63,
            0.015_151_515_151_515_152,
            0.001_262_626_262_626_262_7,
            6.313_131_313_131_313e-5,
            1.503_126_503_126_503e-6,
        ];
        poly(x, &K) / poly(-x, &K)
    }

    /// `ln(1 + x)`, Padé `[6/6]`, for `x` in `-0.8..=5`: max error `7.6e-5` absolute.
    ///
    /// ```
    /// use syrp_dsp_core::FastMathApproximations as Fast;
    /// assert_eq!(Fast::log_nplusone(0.0_f64), 0.0);
    /// ```
    #[doc(alias = "logNPlusOne")]
    #[inline(always)]
    #[must_use]
    pub fn log_nplusone<S: DspSample<Element: DspFloat>>(x: S) -> S {
        let num = poly(
            x,
            &[
                1.0,
                2.5,
                2.242_424_242_424_242,
                0.863_636_363_636_363_6,
                0.131_818_181_818_181_8,
                0.005_303_030_303_030_303,
            ],
        );
        let den = poly(
            x,
            &[
                1.0,
                3.0,
                3.409_090_909_090_909,
                1.818_181_818_181_818_1,
                0.454_545_454_545_454_53,
                0.045_454_545_454_545_456,
                0.001_082_251_082_251_082_3,
            ],
        );
        x * num / den
    }
}

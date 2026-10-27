// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`Phase`]: a wrapping phase accumulator.

use std::f64::consts::TAU;

use syrp_simd::Mask;

use crate::{DspFloat, DspSample};

/// A phase in `0..2π` that advances by an increment and wraps back past `2π` (JUCE's
/// `dsp::Phase`). Lane-generic: `Phase<F32x8>` is eight oscillator phases.
///
/// ```
/// use syrp_dsp_core::Phase;
/// let mut p = Phase::<f64>::default();
/// assert_eq!(p.advance(4.0), 0.0); // returns the value before advancing
/// let before = p.advance(4.0);
/// assert_eq!(before, 4.0);
/// assert!((p.value() - (8.0 - std::f64::consts::TAU)).abs() < 1e-12);
/// ```
#[doc(alias = "dsp::Phase")]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Phase<S> {
    phase: S,
}

impl<S: DspSample<Element: DspFloat>> Phase<S> {
    /// A phase starting at 0.
    ///
    /// ```
    /// assert_eq!(syrp_dsp_core::Phase::<f32>::new().value(), 0.0);
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self { phase: S::zero() }
    }

    /// Resets the phase to 0.
    ///
    /// ```
    /// let mut p = syrp_dsp_core::Phase::<f32>::new();
    /// p.advance(1.0);
    /// p.reset();
    /// assert_eq!(p.value(), 0.0);
    /// ```
    pub fn reset(&mut self) {
        self.phase = S::zero();
    }

    /// Returns the current phase, then sets it to `(phase + increment) % 2π`. Each lane of
    /// `increment` must be in `0..2π` (one wrap per call; checked in debug builds).
    ///
    /// ```
    /// use syrp_dsp_core::Phase;
    /// use syrp_simd::F32x4;
    /// let mut p = Phase::<F32x4>::new();
    /// p.advance(F32x4::from_array([0.1, 0.2, 0.3, 6.0]));
    /// assert_eq!(p.value().to_array()[2], 0.3);
    /// ```
    #[inline(always)]
    pub fn advance(&mut self, increment: S) -> S {
        let tau = S::splat(S::Element::from_f64(TAU));
        debug_assert!(
            (increment.simd_ge(S::zero()) & increment.simd_lt(tau)).all(),
            "phase increment {increment:?} outside 0..2π"
        );
        let current = self.phase;
        let next = self.phase + increment;
        self.phase = S::select(next.simd_ge(tau), next - tau, next);
        current
    }

    /// The current phase.
    ///
    /// ```
    /// let mut p = syrp_dsp_core::Phase::<f64>::new();
    /// p.advance(0.5);
    /// assert_eq!(p.value(), 0.5);
    /// ```
    #[must_use]
    pub fn value(&self) -> S {
        self.phase
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0
//! `ParamRange`: a parameter's plain-value range with step and skew.

/// A parameter's range: start, end, step and skew, with the NormalisableRange semantics JUCE
/// documents. [`normalise`](Self::normalise) maps plain values to `0.0..=1.0` and
/// [`denormalise`](Self::denormalise) back, without snapping ([`snap`](Self::snap) does that).
/// A skew below 1 gives the low end more of the normalised travel, above 1 the high end; a
/// symmetric skew mirrors the curve about the middle. Builders never fail; registration
/// rejects an invalid range ([`is_valid`](Self::is_valid)).
/// ```
/// use syrp_plugin_api::ParamRange;
/// let freq = ParamRange::new(20.0, 20_000.0).with_centre(1_000.0);
/// assert!((freq.denormalise(0.5) - 1_000.0).abs() < 1e-9);
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
#[doc(alias = "NormalisableRange")]
pub struct ParamRange {
    start: f64,
    end: f64,
    step: f64,
    skew: f64,
    symmetric: bool,
}

impl ParamRange {
    /// A linear, continuous range from `start` to `end`.
    /// ```
    /// let r = syrp_plugin_api::ParamRange::new(-1.0, 1.0);
    /// assert_eq!((r.start(), r.end(), r.step(), r.skew()), (-1.0, 1.0, 0.0, 1.0));
    /// ```
    #[must_use]
    pub const fn new(start: f64, end: f64) -> Self {
        Self {
            start,
            end,
            step: 0.0,
            skew: 1.0,
            symmetric: false,
        }
    }

    /// Sets the step interval; `0.0` means continuous.
    /// ```
    /// let r = syrp_plugin_api::ParamRange::new(0.0, 10.0).with_step(0.5);
    /// assert_eq!(r.snap(3.3), 3.5);
    /// ```
    #[must_use]
    pub const fn with_step(mut self, step: f64) -> Self {
        self.step = step;
        self
    }

    /// Sets the skew factor (must be finite and above zero; 1 is linear).
    /// ```
    /// let r = syrp_plugin_api::ParamRange::new(0.0, 1.0).with_skew(0.5);
    /// assert!((r.normalise(0.25) - 0.5).abs() < 1e-12);
    /// ```
    #[must_use]
    pub const fn with_skew(mut self, skew: f64) -> Self {
        self.skew = skew;
        self
    }

    /// Mirrors the skew curve around the middle of the range when `symmetric` is true.
    /// ```
    /// let r = syrp_plugin_api::ParamRange::new(-1.0, 1.0).with_skew(0.5).with_symmetric_skew(true);
    /// assert_eq!(r.normalise(0.0), 0.5);
    /// ```
    #[must_use]
    pub const fn with_symmetric_skew(mut self, symmetric: bool) -> Self {
        self.symmetric = symmetric;
        self
    }

    /// Sets a non-symmetric skew so that `centre` sits at the normalised midpoint.
    ///
    /// `centre` must lie strictly inside the range, or the skew becomes invalid and
    /// registration fails.
    /// ```
    /// let r = syrp_plugin_api::ParamRange::new(0.0, 100.0).with_centre(10.0);
    /// assert!((r.normalise(10.0) - 0.5).abs() < 1e-12);
    /// ```
    #[doc(alias = "setSkewForCentre")]
    #[must_use]
    pub fn with_centre(mut self, centre: f64) -> Self {
        let proportion = (centre - self.start) / (self.end - self.start);
        self.skew = 0.5f64.ln() / proportion.ln();
        self.symmetric = false;
        self
    }

    /// The lowest plain value.
    /// ```
    /// assert_eq!(syrp_plugin_api::ParamRange::new(2.0, 3.0).start(), 2.0);
    /// ```
    #[must_use]
    pub const fn start(&self) -> f64 {
        self.start
    }

    /// The highest plain value.
    /// ```
    /// assert_eq!(syrp_plugin_api::ParamRange::new(2.0, 3.0).end(), 3.0);
    /// ```
    #[must_use]
    pub const fn end(&self) -> f64 {
        self.end
    }

    /// The step interval, `0.0` when continuous.
    /// ```
    /// assert_eq!(syrp_plugin_api::ParamRange::new(0.0, 8.0).with_step(1.0).step(), 1.0);
    /// ```
    #[must_use]
    pub const fn step(&self) -> f64 {
        self.step
    }

    /// The skew factor.
    /// ```
    /// assert_eq!(syrp_plugin_api::ParamRange::new(0.0, 1.0).with_skew(3.0).skew(), 3.0);
    /// ```
    #[must_use]
    pub const fn skew(&self) -> f64 {
        self.skew
    }

    /// Whether the skew is mirrored around the middle of the range.
    /// ```
    /// assert!(!syrp_plugin_api::ParamRange::new(0.0, 1.0).is_symmetric_skew());
    /// ```
    #[must_use]
    pub const fn is_symmetric_skew(&self) -> bool {
        self.symmetric
    }

    /// Whether the bounds are finite with `start < end`, the step is finite and not negative,
    /// and the skew is finite and above zero.
    /// ```
    /// use syrp_plugin_api::ParamRange;
    /// assert!(ParamRange::new(0.0, 1.0).is_valid());
    /// assert!(!ParamRange::new(1.0, 1.0).is_valid());
    /// assert!(!ParamRange::new(0.0, 1.0).with_skew(0.0).is_valid());
    /// ```
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.start.is_finite()
            && self.end.is_finite()
            && self.start < self.end
            && self.step.is_finite()
            && self.step >= 0.0
            && self.skew.is_finite()
            && self.skew > 0.0
    }

    /// Maps a plain value to `0.0..=1.0`, clamping values outside the range (NaN gives 0).
    /// ```
    /// let r = syrp_plugin_api::ParamRange::new(0.0, 10.0);
    /// assert_eq!((r.normalise(2.5), r.normalise(-5.0), r.normalise(99.0)), (0.25, 0.0, 1.0));
    /// ```
    #[doc(alias = "convertTo0to1")]
    #[must_use]
    pub fn normalise(&self, value: f64) -> f64 {
        let span = self.end - self.start;
        let proportion = clamp_unit((value - self.start) / span);
        if self.skew == 1.0 {
            return proportion;
        }
        if self.symmetric {
            let from_middle = 2.0 * proportion - 1.0;
            let curved = from_middle.abs().powf(self.skew).copysign(from_middle);
            (1.0 + curved) / 2.0
        } else {
            proportion.powf(self.skew)
        }
    }

    /// Maps a normalised value in `0.0..=1.0` (clamped; NaN gives 0) back to a plain value.
    /// The result is not snapped to the step.
    /// ```
    /// let r = syrp_plugin_api::ParamRange::new(0.0, 10.0).with_step(1.0);
    /// assert_eq!(r.denormalise(0.25), 2.5);
    /// ```
    #[doc(alias = "convertFrom0to1")]
    #[must_use]
    pub fn denormalise(&self, normalised: f64) -> f64 {
        let span = self.end - self.start;
        let n = clamp_unit(normalised);
        if self.skew == 1.0 {
            return self.start + span * n;
        }
        if self.symmetric {
            let from_middle = 2.0 * n - 1.0;
            let linear = from_middle
                .abs()
                .powf(self.skew.recip())
                .copysign(from_middle);
            self.start + span / 2.0 * (1.0 + linear)
        } else {
            self.start + span * n.powf(self.skew.recip())
        }
    }

    /// Clamps `value` into the range and rounds it to the nearest step (ties round up).
    /// ```
    /// let r = syrp_plugin_api::ParamRange::new(1.0, 2.0).with_step(0.25);
    /// assert_eq!((r.snap(1.3), r.snap(1.375), r.snap(7.0)), (1.25, 1.5, 2.0));
    /// ```
    #[doc(alias = "snapToLegalValue")]
    #[must_use]
    pub fn snap(&self, value: f64) -> f64 {
        // `max`/`min` rather than `clamp`, which panics on an invalid (unregistered) range.
        let clamped = value.max(self.start).min(self.end);
        if self.step > 0.0 {
            let steps = ((clamped - self.start) / self.step + 0.5).floor();
            (self.start + steps * self.step).min(self.end)
        } else {
            clamped
        }
    }
}

/// Clamps to `0.0..=1.0`, mapping NaN (an empty or invalid span) to 0.
fn clamp_unit(x: f64) -> f64 {
    if x.is_nan() { 0.0 } else { x.clamp(0.0, 1.0) }
}

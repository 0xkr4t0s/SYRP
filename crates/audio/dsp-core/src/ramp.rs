// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`LogRampedValue`]: a value that moves to its target along an exponential curve.

use crate::DspFloat;

/// A smoothed value that ramps to its target along an exponential curve (JUCE's
/// `dsp::LogRampedValue`).
///
/// The curve is set by [`set_log_parameters`](Self::set_log_parameters): with the start of
/// a ramp at `-inf` dB and the target at 0 dB, the midpoint of the ramp is at
/// `mid_point_amplitude_db`. When `rate_of_change_should_increase` the ramp starts shallow and
/// steepens; otherwise it starts steep and flattens out. Over normalised time `t` in `0..=1`
/// the rising shape is `g(t) = (e^(kt) - 1) / (e^k - 1)` with `k = 2 ln(1/m - 1)` and
/// `m = 10^(dB/20)`, so `g(1/2) = m`; the falling shape is `1 - g(1 - t)`, whose midpoint is
/// `1 - m`. The JUCE documentation gives no formula, so this one is Syrp's choice. The
/// defaults are `-12` dB, rising (also Syrp's choice).
///
/// Each step is one multiply-add; `next_value`, `skip` and `set_target_value` never
/// allocate.
///
/// ```
/// use syrp_dsp_core::LogRampedValue;
/// let mut v = LogRampedValue::<f64>::new(0.0);
/// v.reset(4);
/// v.set_target_value(1.0);
/// let ramp: Vec<f64> = (0..4).map(|_| v.next_value()).collect();
/// assert!(ramp.windows(2).all(|w| w[1] - w[0] > 0.0)); // rising
/// assert!(ramp[1] < 0.5 && ramp[3] == 1.0); // shallow start, exact end
/// assert!(!v.is_smoothing());
/// ```
#[doc(alias = "dsp::LogRampedValue")]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogRampedValue<T> {
    current: T,
    target: T,
    steps_to_target: usize,
    countdown: usize,
    mid_point_db: f64,
    rising: bool,
    /// `current = current * ratio + offset` per step.
    ratio: f64,
    offset: f64,
    /// `current` in `f64`, so long ramps do not accumulate `f32` rounding.
    state: f64,
}

impl<T: DspFloat> LogRampedValue<T> {
    /// A value resting at `initial`, with no ramp length (changes jump) until
    /// [`reset`](Self::reset).
    ///
    /// ```
    /// let v = syrp_dsp_core::LogRampedValue::<f32>::new(0.5);
    /// assert_eq!((v.current_value(), v.target_value()), (0.5, 0.5));
    /// ```
    #[must_use]
    pub fn new(initial: T) -> Self {
        Self {
            current: initial,
            target: initial,
            steps_to_target: 0,
            countdown: 0,
            mid_point_db: -12.0,
            rising: true,
            ratio: 1.0,
            offset: 0.0,
            state: initial.to_f64(),
        }
    }

    /// Sets the curve: the midpoint level in dB (below 0; clamped to `-120..=-0.001`) and
    /// whether the rate of change increases along the ramp. Takes effect at the next
    /// [`set_target_value`](Self::set_target_value).
    ///
    /// ```
    /// let mut v = syrp_dsp_core::LogRampedValue::<f64>::new(0.0);
    /// v.set_log_parameters(-6.0, false);
    /// ```
    pub fn set_log_parameters(
        &mut self,
        mid_point_amplitude_db: f64,
        rate_of_change_should_increase: bool,
    ) {
        self.mid_point_db = if mid_point_amplitude_db.is_nan() {
            -12.0
        } else {
            mid_point_amplitude_db.clamp(-120.0, -0.001)
        };
        self.rising = rate_of_change_should_increase;
    }

    /// Sets the ramp length to `sample_rate * ramp_length_seconds` steps (rounded down) and
    /// stops any ramp in progress at the target.
    ///
    /// ```
    /// let mut v = syrp_dsp_core::LogRampedValue::<f32>::new(0.0);
    /// v.reset_seconds(48_000.0, 0.01);
    /// v.set_target_value(1.0);
    /// v.skip(479);
    /// assert!(v.is_smoothing());
    /// v.skip(1);
    /// assert_eq!(v.current_value(), 1.0);
    /// ```
    pub fn reset_seconds(&mut self, sample_rate: f64, ramp_length_seconds: f64) {
        let steps = (sample_rate * ramp_length_seconds).max(0.0);
        self.reset(if steps.is_finite() { steps as usize } else { 0 });
    }

    /// Sets the ramp length to `num_steps` steps and stops any ramp in progress at the target.
    ///
    /// ```
    /// let mut v = syrp_dsp_core::LogRampedValue::<f32>::new(0.0);
    /// v.reset(0);
    /// v.set_target_value(1.0); // no ramp: jumps
    /// assert_eq!(v.current_value(), 1.0);
    /// ```
    pub fn reset(&mut self, num_steps: usize) {
        self.steps_to_target = num_steps;
        self.set_current_and_target_value(self.target);
    }

    /// Jumps to `value` with no ramp.
    ///
    /// ```
    /// let mut v = syrp_dsp_core::LogRampedValue::<f64>::new(0.0);
    /// v.set_current_and_target_value(3.0);
    /// assert!(!v.is_smoothing() && v.next_value() == 3.0);
    /// ```
    pub fn set_current_and_target_value(&mut self, value: T) {
        self.current = value;
        self.target = value;
        self.state = value.to_f64();
        self.countdown = 0;
    }

    /// Starts a ramp from the current value to `new_value` over the ramp length. With a
    /// ramp length of zero the value jumps.
    ///
    /// ```
    /// let mut v = syrp_dsp_core::LogRampedValue::<f64>::new(1.0);
    /// v.reset(8);
    /// v.set_target_value(0.0);
    /// assert!(v.is_smoothing() && v.target_value() == 0.0);
    /// ```
    pub fn set_target_value(&mut self, new_value: T) {
        if new_value == self.target && self.countdown == 0 {
            return;
        }
        if self.steps_to_target == 0 {
            self.set_current_and_target_value(new_value);
            return;
        }
        self.target = new_value;
        self.countdown = self.steps_to_target;
        let n = self.steps_to_target as f64;
        let m = 10.0_f64.powf(self.mid_point_db / 20.0);
        let k = 2.0 * (1.0 / m - 1.0).ln();
        let (start, end) = (self.state, new_value.to_f64());
        // Rising: v_j = start + (end - start) (e^(kj/n) - 1) / (e^k - 1), an affine recurrence
        // with ratio e^(k/n). Falling mirrors it in time: ratio e^(-k/n) about the end.
        let (ratio, fixed) = if k.abs() < 1e-12 {
            (1.0, 0.0)
        } else if self.rising {
            let d = (end - start) / k.exp_m1();
            ((k / n).exp(), start - d)
        } else {
            let d = (end - start) / k.exp_m1();
            ((-k / n).exp(), end + d)
        };
        if ratio == 1.0 {
            // Linear: the curve is a straight line (only at m = 1/2).
            self.ratio = 1.0;
            self.offset = (end - start) / n;
        } else {
            self.ratio = ratio;
            self.offset = (1.0 - ratio) * fixed;
        }
    }

    /// Advances one step and returns the new current value; at the last step it is exactly
    /// the target.
    ///
    /// ```
    /// let mut v = syrp_dsp_core::LogRampedValue::<f32>::new(0.0);
    /// v.reset(2);
    /// v.set_target_value(1.0);
    /// v.next_value();
    /// assert_eq!(v.next_value(), 1.0);
    /// ```
    #[inline]
    pub fn next_value(&mut self) -> T {
        if self.countdown == 0 {
            return self.target;
        }
        self.countdown -= 1;
        if self.countdown == 0 {
            self.set_current_and_target_value(self.target);
        } else {
            self.state = self.state * self.ratio + self.offset;
            self.current = T::from_f64(self.state);
        }
        self.current
    }

    /// Advances `num_samples` steps, exactly as that many [`next_value`](Self::next_value)
    /// calls would.
    ///
    /// ```
    /// let mut v = syrp_dsp_core::LogRampedValue::<f64>::new(0.0);
    /// v.reset(10);
    /// v.set_target_value(1.0);
    /// v.skip(100);
    /// assert_eq!(v.current_value(), 1.0);
    /// ```
    pub fn skip(&mut self, num_samples: usize) {
        for _ in 0..num_samples.min(self.countdown) {
            self.next_value();
        }
    }

    /// Whether a ramp is in progress.
    ///
    /// ```
    /// assert!(!syrp_dsp_core::LogRampedValue::<f32>::new(0.0).is_smoothing());
    /// ```
    #[must_use]
    pub fn is_smoothing(&self) -> bool {
        self.countdown > 0
    }

    /// The current value.
    ///
    /// ```
    /// assert_eq!(syrp_dsp_core::LogRampedValue::<f32>::new(2.0).current_value(), 2.0);
    /// ```
    #[must_use]
    pub fn current_value(&self) -> T {
        self.current
    }

    /// The value the ramp is heading to.
    ///
    /// ```
    /// assert_eq!(syrp_dsp_core::LogRampedValue::<f32>::new(2.0).target_value(), 2.0);
    /// ```
    #[must_use]
    pub fn target_value(&self) -> T {
        self.target
    }
}

impl<T: DspFloat> Default for LogRampedValue<T> {
    fn default() -> Self {
        Self::new(T::zero())
    }
}

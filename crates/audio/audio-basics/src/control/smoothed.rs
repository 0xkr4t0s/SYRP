// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`SmoothedValue`]: a lane-generic ramp toward a target (spec B2, B3).

use core::fmt::Debug;
use core::marker::PhantomData;

use syrp_simd::Mask;

use super::ControlSample;

/// How a [`SmoothedValue`] moves between values: [`Linear`] or [`Multiplicative`]. Sealed.
///
/// ```
/// use syrp_audio_basics::control::{Mapping, Multiplicative, SmoothedValue};
/// fn ramp<M: Mapping>(from: f32, to: f32) -> f32 {
///     let mut v = SmoothedValue::<f32, M>::new(from);
///     v.reset_steps(2);
///     v.set_target(to);
///     v.next_value()
/// }
/// assert_eq!(ramp::<Multiplicative>(1.0, 4.0), 2.0);
/// ```
#[doc(alias = "ValueSmoothingTypes")]
pub trait Mapping: mapping::Sealed + Copy + Debug + Default + Send + Sync + 'static {}

/// Equal steps: each sample adds the same amount.
///
/// ```
/// use syrp_audio_basics::control::{Linear, SmoothedValue};
/// let mut v = SmoothedValue::<f32, Linear>::new(0.0);
/// v.reset_steps(2);
/// v.set_target(1.0);
/// assert_eq!(v.next_value(), 0.5);
/// ```
#[doc(alias = "ValueSmoothingTypes::Linear")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Linear;

/// Equal ratios: each sample multiplies by the same factor, so the ramp is linear in
/// decibels. Every value it sees must be non-zero, and a ramp may not cross zero; both are
/// checked with a debug assertion (as JUCE documents).
///
/// ```
/// use syrp_audio_basics::control::{Multiplicative, SmoothedValue};
/// let mut v = SmoothedValue::<f64, Multiplicative>::new(0.01);
/// v.reset_steps(2);
/// v.set_target(1.0);
/// assert!((v.next_value() - 0.1).abs() < 1e-12);
/// ```
#[doc(alias = "ValueSmoothingTypes::Multiplicative")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Multiplicative;

impl Mapping for Linear {}
impl Mapping for Multiplicative {}

mod mapping {
    use super::{ControlSample, Linear, Mask, Multiplicative};

    /// The per-sample arithmetic of a [`Mapping`](super::Mapping).
    pub trait Sealed {
        /// The per-sample step that moves `from` to `to` in `steps` samples.
        fn step<S: ControlSample>(from: S, to: S, steps: S) -> S;
        /// One step from `value` toward `target`, with `left` steps to go after it.
        fn next<S: ControlSample>(value: S, step: S, target: S, left: S) -> S;
        /// `n` steps (per lane, possibly zero) from `value` toward `target`, with `left` steps
        /// to go after them.
        fn skip<S: ControlSample>(value: S, step: S, target: S, n: S, left: S) -> S;
        /// Debug-asserts that a ramp from `from` to `to` is allowed.
        fn check<S: ControlSample>(from: S, to: S);
    }

    /// Positions are measured back from the target, so long ramps do not accumulate rounding.
    impl Sealed for Linear {
        #[inline(always)]
        fn step<S: ControlSample>(from: S, to: S, steps: S) -> S {
            (to - from) / steps
        }
        #[inline(always)]
        fn next<S: ControlSample>(_: S, step: S, target: S, left: S) -> S {
            target - step * left
        }
        #[inline(always)]
        fn skip<S: ControlSample>(_: S, step: S, target: S, _: S, left: S) -> S {
            target - step * left
        }
        #[inline(always)]
        fn check<S: ControlSample>(_: S, _: S) {}
    }

    impl Sealed for Multiplicative {
        #[inline(always)]
        fn step<S: ControlSample>(from: S, to: S, steps: S) -> S {
            (to / from).powf(S::one() / steps)
        }
        #[inline(always)]
        fn next<S: ControlSample>(value: S, step: S, _: S, _: S) -> S {
            value * step
        }
        #[inline(always)]
        fn skip<S: ControlSample>(value: S, step: S, _: S, n: S, _: S) -> S {
            value * step.powf(n)
        }
        #[inline(always)]
        fn check<S: ControlSample>(from: S, to: S) {
            debug_assert!(
                (from * to).simd_gt(S::zero()).all(),
                "Multiplicative smoothing needs non-zero values that do not cross zero \
                 (from {from:?} to {to:?})"
            );
        }
    }
}

/// A value that ramps toward a target over a set number of samples, in the role of JUCE's
/// `SmoothedValue`.
///
/// `S` is `f32`, `f64` or a lane pack; a pack holds one independent value per lane, with its
/// own target and its own position in the ramp, and every lane shares the ramp length. `M`
/// is [`Linear`] (the default) or [`Multiplicative`].
///
/// A new target starts a ramp from the current value that reaches the target exactly on its
/// last sample. Lanes whose target did not change keep their ramp. By default the ramp is
/// computed every sample; [`with_control_rate`](Self::with_control_rate) computes it every
/// `n` samples and interpolates linearly in between (ADR 0003 D5).
///
/// ```
/// use syrp_audio_basics::control::SmoothedValue;
/// use syrp_simd::{F32x4, Mask, Sample};
/// let mut v = SmoothedValue::<F32x4>::new(F32x4::zero());
/// v.reset(1000.0, 0.004); // four samples
/// v.set_target(F32x4::from_array([4.0, 8.0, 0.0, -4.0]));
/// assert_eq!(v.next_value().to_array(), [1.0, 2.0, 0.0, -1.0]);
/// assert_eq!(v.is_smoothing().to_array(), [true, true, false, true]);
/// assert_eq!(v.skip(3).to_array(), [4.0, 8.0, 0.0, -4.0]);
/// assert!(v.is_smoothing().none());
/// ```
#[derive(Clone, Copy, Debug)]
pub struct SmoothedValue<S: ControlSample, M: Mapping = Linear> {
    current: S,
    target: S,
    /// The mapping's per-sample step for each lane's current ramp.
    step: S,
    /// Samples left in each lane's ramp, as a float (exact up to 2^24 for `f32`).
    remaining: S,
    /// Ramp length in samples.
    steps: u32,
    /// Samples between ramp updates; 1 is the per-sample path.
    control: u32,
    /// Per-sample increment inside the current control segment.
    segment_step: S,
    /// Samples left in each lane's control segment; 0 recomputes that lane's segment.
    segment_left: S,
    mapping: PhantomData<M>,
}

impl<S: ControlSample, M: Mapping> SmoothedValue<S, M> {
    /// A value at `initial` (current and target), with a ramp length of zero, so targets are
    /// reached at once until [`reset`](Self::reset) or [`reset_steps`](Self::reset_steps).
    ///
    /// ```
    /// let v = syrp_audio_basics::control::SmoothedValue::<f32>::new(0.5);
    /// assert_eq!((v.current(), v.target()), (0.5, 0.5));
    /// ```
    #[inline(always)]
    pub fn new(initial: S) -> Self {
        M::check(initial, initial);
        Self {
            current: initial,
            target: initial,
            step: S::zero(),
            remaining: S::zero(),
            steps: 0,
            control: 1,
            segment_step: S::zero(),
            segment_left: S::zero(),
            mapping: PhantomData,
        }
    }

    /// Updates the ramp only every `n` samples (16 or 32 are typical) and interpolates
    /// linearly per sample in between. Ramps still end exactly on the target on the same
    /// sample as the per-sample path. A [`Linear`] ramp is unchanged up to rounding; a
    /// [`Multiplicative`] one differs by at most about `(n * ln r)^2 / 8` relative, where `r`
    /// is the per-sample ratio. `n = 1` (the default) is the per-sample path; `0` counts as 1.
    ///
    /// ```
    /// use syrp_audio_basics::control::SmoothedValue;
    /// let v = SmoothedValue::<f32>::new(0.0).with_control_rate(16);
    /// assert_eq!(v.control_rate(), 16);
    /// ```
    #[must_use]
    pub fn with_control_rate(mut self, n: u32) -> Self {
        self.control = n.max(1);
        self.segment_left = S::zero();
        self
    }

    /// Samples between ramp updates; 1 is the per-sample path.
    ///
    /// ```
    /// assert_eq!(syrp_audio_basics::control::SmoothedValue::<f64>::new(0.0).control_rate(), 1);
    /// ```
    pub fn control_rate(&self) -> u32 {
        self.control
    }

    /// Sets the ramp length to `floor(sample_rate * ramp_seconds)` samples (saturating; a
    /// negative or NaN length is zero) and jumps to the target.
    ///
    /// ```
    /// let mut v = syrp_audio_basics::control::SmoothedValue::<f32>::new(0.0);
    /// v.reset(48_000.0, 0.05);
    /// assert_eq!(v.ramp_steps(), 2400);
    /// ```
    pub fn reset(&mut self, sample_rate: f64, ramp_seconds: f64) {
        // `as` saturates, and maps NaN to 0.
        self.reset_steps((sample_rate * ramp_seconds).floor() as u32);
    }

    /// Sets the ramp length to `steps` samples and jumps to the target.
    ///
    /// ```
    /// let mut v = syrp_audio_basics::control::SmoothedValue::<f32>::new(0.0);
    /// v.reset_steps(8);
    /// v.set_target(1.0);
    /// v.reset_steps(8);
    /// assert_eq!(v.current(), 1.0);
    /// ```
    pub fn reset_steps(&mut self, steps: u32) {
        self.steps = steps;
        self.set_current_and_target(self.target);
    }

    /// The ramp length in samples.
    ///
    /// ```
    /// assert_eq!(syrp_audio_basics::control::SmoothedValue::<f32>::new(1.0).ramp_steps(), 0);
    /// ```
    pub fn ramp_steps(&self) -> u32 {
        self.steps
    }

    /// Sets the current value and the target to `value`, ending any ramp.
    ///
    /// ```
    /// let mut v = syrp_audio_basics::control::SmoothedValue::<f32>::new(0.0);
    /// v.reset_steps(8);
    /// v.set_target(1.0);
    /// v.set_current_and_target(0.25);
    /// assert!(!v.is_smoothing());
    /// assert_eq!(v.next_value(), 0.25);
    /// ```
    #[doc(alias = "setCurrentAndTargetValue")]
    #[inline(always)]
    pub fn set_current_and_target(&mut self, value: S) {
        M::check(value, value);
        self.current = value;
        self.target = value;
        self.remaining = S::zero();
        self.segment_left = S::zero();
    }

    /// Starts a ramp from the current value to `target` in each lane whose target changes.
    /// Lanes with the same target keep their ramp. With a ramp length of zero, jumps.
    ///
    /// ```
    /// let mut v = syrp_audio_basics::control::SmoothedValue::<f32>::new(0.0);
    /// v.reset_steps(4);
    /// v.set_target(4.0);
    /// v.next_value();
    /// v.set_target(1.0); // restarts from 1.0: four steps of 0.0
    /// assert_eq!(v.next_value(), 1.0);
    /// ```
    #[doc(alias = "setTargetValue")]
    #[inline(always)]
    pub fn set_target(&mut self, target: S) {
        if self.steps == 0 {
            self.set_current_and_target(target);
            return;
        }
        let changed = !target.simd_eq(self.target);
        if changed.none() {
            return;
        }
        M::check(self.current, target);
        let steps = S::from_count(self.steps);
        self.step = S::select(changed, M::step(self.current, target, steps), self.step);
        self.remaining = S::select(changed, steps, self.remaining);
        self.target = target;
        self.segment_left = S::select(changed, S::zero(), self.segment_left);
    }

    /// Advances one sample and returns the new value.
    ///
    /// ```
    /// let mut v = syrp_audio_basics::control::SmoothedValue::<f32>::new(1.0);
    /// v.reset_steps(2);
    /// v.set_target(2.0);
    /// assert_eq!([v.next_value(), v.next_value(), v.next_value()], [1.5, 2.0, 2.0]);
    /// ```
    #[doc(alias = "getNextValue")]
    #[inline(always)]
    pub fn next_value(&mut self) -> S {
        let active = self.remaining.simd_gt(S::zero());
        if active.none() {
            return self.target;
        }
        let left = S::select(active, self.remaining - S::one(), self.remaining);
        let stepped = if self.control <= 1 {
            M::next(self.current, self.step, self.target, left)
        } else {
            self.segment_advance()
        };
        self.remaining = left;
        self.current = S::select(left.simd_gt(S::zero()), stepped, self.target);
        self.current
    }

    /// The control-rate path: the next value on the straight line toward the true ramp
    /// value `control` samples ahead (or the end of the ramp, if sooner). Each lane starts a
    /// new segment on its own schedule, so a pack lane matches a scalar run.
    #[inline(always)]
    fn segment_advance(&mut self) -> S {
        let due = self.segment_left.simd_le(S::zero());
        if due.any() {
            let control = S::from_count(self.control);
            let span = self.remaining.min(control);
            let left = self.remaining - span;
            let end = M::skip(self.current, self.step, self.target, span, left);
            let moving = span.simd_gt(S::zero());
            let delta = (end - self.current) / S::select(moving, span, S::one());
            let delta = S::select(moving, delta, S::zero());
            self.segment_step = S::select(due, delta, self.segment_step);
            self.segment_left = S::select(due, control, self.segment_left);
        }
        self.segment_left -= S::one();
        self.current + self.segment_step
    }

    /// Advances `n` samples at once and returns the new value; the same as calling
    /// [`next_value`](Self::next_value) `n` times, up to rounding.
    ///
    /// ```
    /// let mut v = syrp_audio_basics::control::SmoothedValue::<f32>::new(0.0);
    /// v.reset_steps(10);
    /// v.set_target(10.0);
    /// assert_eq!(v.skip(4), 4.0);
    /// assert_eq!(v.skip(100), 10.0);
    /// ```
    #[inline(always)]
    pub fn skip(&mut self, n: u32) -> S {
        let span = self.remaining.min(S::from_count(n));
        let left = self.remaining - span;
        let advanced = M::skip(self.current, self.step, self.target, span, left);
        self.remaining = left;
        self.current = S::select(left.simd_gt(S::zero()), advanced, self.target);
        self.segment_left = S::zero();
        self.current
    }

    /// The value most recently returned (or set).
    ///
    /// ```
    /// assert_eq!(syrp_audio_basics::control::SmoothedValue::<f32>::new(0.5).current(), 0.5);
    /// ```
    #[doc(alias = "getCurrentValue")]
    pub fn current(&self) -> S {
        self.current
    }

    /// The value being ramped toward.
    ///
    /// ```
    /// let mut v = syrp_audio_basics::control::SmoothedValue::<f32>::new(0.0);
    /// v.reset_steps(4);
    /// v.set_target(2.0);
    /// assert_eq!((v.current(), v.target()), (0.0, 2.0));
    /// ```
    #[doc(alias = "getTargetValue")]
    pub fn target(&self) -> S {
        self.target
    }

    /// The lanes still ramping toward their target (a `bool` for `f32` and `f64`).
    ///
    /// ```
    /// let mut v = syrp_audio_basics::control::SmoothedValue::<f32>::new(0.0);
    /// v.reset_steps(1);
    /// v.set_target(1.0);
    /// assert!(v.is_smoothing());
    /// v.next_value();
    /// assert!(!v.is_smoothing());
    /// ```
    #[doc(alias = "isSmoothing")]
    #[inline(always)]
    pub fn is_smoothing(&self) -> S::Mask {
        self.remaining.simd_gt(S::zero())
    }

    /// Multiplies `block` by the value, advancing one sample per element; when no lane is
    /// ramping, multiplies by the target.
    ///
    /// ```
    /// let mut v = syrp_audio_basics::control::SmoothedValue::<f32>::new(0.0);
    /// v.reset_steps(4);
    /// v.set_target(1.0);
    /// let mut block = [2.0_f32; 6];
    /// v.apply_gain(&mut block);
    /// assert_eq!(block, [0.5, 1.0, 1.5, 2.0, 2.0, 2.0]);
    /// ```
    #[doc(alias = "applyGain")]
    #[inline(always)]
    pub fn apply_gain(&mut self, block: &mut [S]) {
        if self.is_smoothing().none() {
            let gain = self.target;
            block.iter_mut().for_each(|x| *x *= gain);
            return;
        }
        for x in block {
            *x *= self.next_value();
        }
    }
}

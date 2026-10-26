// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`Adsr`]: a lane-generic attack, decay, sustain, release envelope (spec B4).

use syrp_simd::Mask;

use super::ControlSample;

/// The settings of an [`Adsr`]: attack, decay and release times in seconds and a sustain
/// level, one value per lane for a pack.
///
/// [`new`](Self::new) clamps negative times to zero and the sustain level to `0.0..=1.0`.
///
/// ```
/// use syrp_audio_basics::control::AdsrParams;
/// let p = AdsrParams::new(0.01_f32, 0.1, 0.5, -1.0);
/// assert_eq!((p.attack(), p.decay(), p.sustain(), p.release()), (0.01, 0.1, 0.5, 0.0));
/// ```
#[doc(alias = "ADSR::Parameters")]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdsrParams<S> {
    attack: S,
    decay: S,
    sustain: S,
    release: S,
}

impl<S: ControlSample> AdsrParams<S> {
    /// Settings from attack, decay and release times in seconds and a sustain level.
    ///
    /// ```
    /// use syrp_audio_basics::control::AdsrParams;
    /// use syrp_simd::{F32x4, Sample};
    /// let p = AdsrParams::new(F32x4::splat(0.01), F32x4::splat(0.1), F32x4::splat(2.0), F32x4::splat(0.2));
    /// assert_eq!(p.sustain(), F32x4::one());
    /// ```
    pub fn new(attack: S, decay: S, sustain: S, release: S) -> Self {
        let zero = S::zero();
        Self {
            attack: attack.max(zero),
            decay: decay.max(zero),
            sustain: sustain.max(zero).min(S::one()),
            release: release.max(zero),
        }
    }

    /// The attack time in seconds.
    ///
    /// ```
    /// let p = syrp_audio_basics::control::AdsrParams::new(0.5_f64, 0.0, 1.0, 0.0);
    /// assert_eq!(p.attack(), 0.5);
    /// ```
    pub fn attack(&self) -> S {
        self.attack
    }

    /// The decay time in seconds.
    ///
    /// ```
    /// let p = syrp_audio_basics::control::AdsrParams::new(0.0_f64, 0.25, 1.0, 0.0);
    /// assert_eq!(p.decay(), 0.25);
    /// ```
    pub fn decay(&self) -> S {
        self.decay
    }

    /// The sustain level, from `0.0` to `1.0`.
    ///
    /// ```
    /// let p = syrp_audio_basics::control::AdsrParams::new(0.0_f32, 0.0, 0.7, 0.0);
    /// assert_eq!(p.sustain(), 0.7);
    /// ```
    #[doc(alias = "sustainLevel")]
    pub fn sustain(&self) -> S {
        self.sustain
    }

    /// The release time in seconds.
    ///
    /// ```
    /// let p = syrp_audio_basics::control::AdsrParams::new(0.0_f32, 0.0, 1.0, 0.3);
    /// assert_eq!(p.release(), 0.3);
    /// ```
    pub fn release(&self) -> S {
        self.release
    }
}

/// An attack, decay, sustain, release envelope, in the role of JUCE's `ADSR`.
///
/// `S` is `f32`, `f64` or a lane pack; `Adsr<F32x8>` is eight independent envelopes, each with
/// its own gate, stage and settings. Stage changes are lane masks applied with
/// [`Sample::select`](syrp_simd::Sample::select), never per-lane branches.
///
/// Segments are linear (Syrp's choice, not yet compared with JUCE) and move in equal steps that
/// end exactly on their goal. The attack rises from the current level to `1.0` at about the
/// slope of a full attack, the decay falls to the sustain level, and the release falls from
/// the level at [`note_off`](Self::note_off) to `0.0` over the release time. A segment of `t`
/// seconds lasts `t * sample_rate` samples rounded to nearest (at least one), so a zero-length
/// segment jumps on the next sample. A `note_on` during the release (or any stage) restarts
/// the attack from the current level.
///
/// ```
/// use syrp_audio_basics::control::{Adsr, AdsrParams};
/// let mut env = Adsr::new(1000.0, AdsrParams::new(0.004_f32, 0.002, 0.5, 0.004));
/// env.note_on(true);
/// let up: Vec<f32> = (0..7).map(|_| env.next_sample()).collect();
/// assert_eq!(up, [0.25, 0.5, 0.75, 1.0, 0.75, 0.5, 0.5]);
/// env.note_off(true);
/// let down: Vec<f32> = (0..5).map(|_| env.next_sample()).collect();
/// assert_eq!(down, [0.375, 0.25, 0.125, 0.0, 0.0]);
/// assert!(!env.is_active());
/// ```
#[doc(alias = "ADSR")]
#[derive(Clone, Copy, Debug)]
pub struct Adsr<S: ControlSample> {
    params: AdsrParams<S>,
    sample_rate: f64,
    level: S,
    attack_step: S,
    decay_step: S,
    /// Release length in samples (at least 1); the step is set at `note_off`.
    release_len: S,
    release_step: S,
    /// Per-sample change of the current stage (0 when idle or sustaining).
    slope: S,
    /// Where the current stage ends.
    goal: S,
    /// Samples left in the current stage (0 when idle or sustaining); the level is
    /// `goal - slope * left`, so a stage ends exactly on its goal.
    left: S,
    /// Samples until the first lane ends its stage (`u64::MAX`: none is moving).
    countdown: u64,
    attack: S::Mask,
    decay: S::Mask,
    sustain: S::Mask,
    release: S::Mask,
}

impl<S: ControlSample> Adsr<S> {
    /// An idle envelope (level `0.0`) at `sample_rate` Hz with `params`.
    ///
    /// ```
    /// use syrp_audio_basics::control::{Adsr, AdsrParams};
    /// let env = Adsr::new(48_000.0, AdsrParams::new(0.01_f32, 0.1, 0.8, 0.2));
    /// assert!(!env.is_active());
    /// ```
    pub fn new(sample_rate: f64, params: AdsrParams<S>) -> Self {
        let off = S::Mask::splat(false);
        let mut env = Self {
            params,
            sample_rate,
            level: S::zero(),
            attack_step: S::zero(),
            decay_step: S::zero(),
            release_len: S::one(),
            release_step: S::zero(),
            slope: S::zero(),
            goal: S::zero(),
            left: S::zero(),
            countdown: u64::MAX,
            attack: off,
            decay: off,
            sustain: off,
            release: off,
        };
        env.set_sample_rate(sample_rate);
        env
    }

    /// Sets the sample rate in Hz (debug-asserted positive) and recomputes the segment slopes.
    /// A release in progress keeps its slope.
    ///
    /// ```
    /// use syrp_audio_basics::control::{Adsr, AdsrParams};
    /// let mut env = Adsr::new(1000.0, AdsrParams::new(0.002_f32, 0.0, 1.0, 0.0));
    /// env.set_sample_rate(2000.0);
    /// env.note_on(true);
    /// assert_eq!(env.next_sample(), 0.25);
    /// ```
    #[doc(alias = "setSampleRate")]
    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        debug_assert!(sample_rate > 0.0, "Adsr sample rate must be positive");
        self.sample_rate = sample_rate;
        let rate = S::from_f64(sample_rate);
        // Segment lengths in samples, at least one; `aim` rounds them to whole samples.
        let len = |seconds: S| (seconds * rate).max(S::one());
        let p = self.params;
        self.attack_step = S::one() / len(p.attack);
        self.decay_step = (S::one() - p.sustain) / len(p.decay);
        self.release_len = len(p.release);
        self.aim(S::Mask::splat(true));
    }

    /// The sample rate in Hz.
    ///
    /// ```
    /// use syrp_audio_basics::control::{Adsr, AdsrParams};
    /// let env = Adsr::new(44_100.0, AdsrParams::new(0.0_f64, 0.0, 1.0, 0.0));
    /// assert_eq!(env.sample_rate(), 44_100.0);
    /// ```
    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    /// Replaces the settings. Stages in progress continue with the new slopes and sustain
    /// level (a sustaining lane jumps to the new level), except a release in progress, which
    /// keeps its slope.
    ///
    /// ```
    /// use syrp_audio_basics::control::{Adsr, AdsrParams};
    /// let mut env = Adsr::new(1000.0, AdsrParams::new(0.0_f32, 0.0, 1.0, 0.0));
    /// env.note_on(true);
    /// env.next_sample();
    /// env.set_params(AdsrParams::new(0.0, 0.0, 0.25, 0.0));
    /// assert_eq!(env.next_sample(), 0.25);
    /// ```
    #[doc(alias = "setParameters")]
    pub fn set_params(&mut self, params: AdsrParams<S>) {
        self.params = params;
        self.set_sample_rate(self.sample_rate);
    }

    /// The current settings.
    ///
    /// ```
    /// use syrp_audio_basics::control::{Adsr, AdsrParams};
    /// let p = AdsrParams::new(0.1_f32, 0.2, 0.3, 0.4);
    /// assert_eq!(Adsr::new(1000.0, p).params(), p);
    /// ```
    #[doc(alias = "getParameters")]
    pub fn params(&self) -> AdsrParams<S> {
        self.params
    }

    /// Starts (or restarts) the attack from the current level in the `lanes` set (`true`
    /// for `f32` and `f64`).
    ///
    /// ```
    /// use syrp_audio_basics::control::{Adsr, AdsrParams};
    /// use syrp_simd::{F32x4, Mask32x4, Sample};
    /// let p = AdsrParams::new(F32x4::splat(0.002), F32x4::zero(), F32x4::one(), F32x4::zero());
    /// let mut env = Adsr::new(1000.0, p);
    /// env.note_on(Mask32x4::from_array([true, false, true, false]));
    /// assert_eq!(env.next_sample().to_array(), [0.5, 0.0, 0.5, 0.0]);
    /// ```
    #[doc(alias = "noteOn")]
    #[inline(always)]
    pub fn note_on(&mut self, lanes: S::Mask) {
        let keep = !lanes;
        self.attack = self.attack | lanes;
        self.decay = self.decay & keep;
        self.sustain = self.sustain & keep;
        self.release = self.release & keep;
        self.aim(lanes);
    }

    /// Starts the release from the current level in the active `lanes`. Idle lanes and lanes
    /// already releasing are unchanged.
    ///
    /// ```
    /// use syrp_audio_basics::control::{Adsr, AdsrParams};
    /// let mut env = Adsr::new(1000.0, AdsrParams::new(0.0_f32, 0.0, 0.8, 0.002));
    /// env.note_on(true);
    /// env.next_sample();
    /// env.note_off(true);
    /// assert_eq!([env.next_sample(), env.next_sample()], [0.5, 0.0]);
    /// ```
    #[doc(alias = "noteOff")]
    #[inline(always)]
    pub fn note_off(&mut self, lanes: S::Mask) {
        let start = lanes & (self.attack | self.decay | self.sustain);
        let keep = !start;
        self.release_step = S::select(start, self.level / self.release_len, self.release_step);
        self.release = self.release | start;
        self.attack = self.attack & keep;
        self.decay = self.decay & keep;
        self.sustain = self.sustain & keep;
        self.aim(start);
    }

    /// Advances one sample and returns the new level.
    ///
    /// ```
    /// use syrp_audio_basics::control::{Adsr, AdsrParams};
    /// let mut env = Adsr::new(1000.0, AdsrParams::new(0.0_f64, 0.0, 0.5, 0.0));
    /// assert_eq!(env.next_sample(), 0.0);
    /// env.note_on(true);
    /// assert_eq!([env.next_sample(), env.next_sample()], [1.0, 0.5]);
    /// ```
    #[doc(alias = "getNextSample")]
    #[inline(always)]
    pub fn next_sample(&mut self) -> S {
        // Per sample: one subtract, max, multiply and subtract; stage changes run only on the
        // sample where some lane ends its stage.
        self.left = (self.left - S::one()).max(S::zero());
        self.level = self.goal - self.slope * self.left;
        self.countdown = self.countdown.saturating_sub(1);
        if self.countdown == 0 {
            self.advance_stage();
        }
        self.level
    }

    /// Moves the lanes that ended their stage on: attack to decay, decay to sustain, release
    /// to idle.
    #[inline(always)]
    fn advance_stage(&mut self) {
        let moving = self.attack | self.decay | self.release;
        let done = moving & self.left.simd_le(S::zero());
        let (attack, decay) = (self.attack & done, self.decay & done);
        self.attack = self.attack & !attack;
        self.decay = (self.decay & !decay) | attack;
        self.sustain = self.sustain | decay;
        self.release = self.release & !done;
        self.aim(done);
    }

    /// Starts the stage each of `lanes` is in from its current level: its goal, its length
    /// (the distance over the stage's nominal step, rounded, at least one sample) and the
    /// equal step that covers it. Then finds the next sample on which any lane ends a stage.
    #[inline(always)]
    fn aim(&mut self, lanes: S::Mask) {
        let (one, zero) = (S::one(), S::zero());
        let moving = self.attack | self.decay | self.release;
        let level = S::select(self.decay | self.sustain, self.params.sustain, zero);
        let goal = S::select(self.attack, one, level);
        let decay_or_release = S::select(self.decay, self.decay_step, self.release_step);
        let nominal = S::select(self.attack, self.attack_step, decay_or_release);
        let distance = (goal - self.level).abs() / nominal.max(S::from_f64(1e-30));
        let left = S::select(moving, distance.round().max(one), zero);
        let slope = (goal - self.level) / S::select(moving, left, one);
        self.goal = S::select(lanes, goal, self.goal);
        self.left = S::select(lanes, left, self.left);
        self.slope = S::select(lanes, S::select(moving, slope, zero), self.slope);
        let pending = S::select(moving, self.left, S::from_f64(f64::INFINITY));
        // `as` saturates: no lane moving gives `u64::MAX`.
        self.countdown = pending.min_lane() as u64;
    }

    /// Multiplies `block` by the envelope, advancing one sample per element. Idle lanes
    /// multiply by `0.0`.
    ///
    /// ```
    /// use syrp_audio_basics::control::{Adsr, AdsrParams};
    /// let mut env = Adsr::new(1000.0, AdsrParams::new(0.002_f32, 0.0, 1.0, 0.0));
    /// env.note_on(true);
    /// let mut block = [2.0_f32; 3];
    /// env.apply_to(&mut block);
    /// assert_eq!(block, [1.0, 2.0, 2.0]);
    /// ```
    #[doc(alias = "applyEnvelopeToBuffer")]
    #[inline(always)]
    pub fn apply_to(&mut self, block: &mut [S]) {
        if self.is_active().none() {
            block.iter_mut().for_each(|x| *x *= S::zero());
            return;
        }
        for x in block {
            *x *= self.next_sample();
        }
    }

    /// The lanes in any stage (attack, decay, sustain or release).
    ///
    /// ```
    /// use syrp_audio_basics::control::{Adsr, AdsrParams};
    /// let mut env = Adsr::new(1000.0, AdsrParams::new(0.0_f32, 0.0, 1.0, 0.0));
    /// env.note_on(true);
    /// assert!(env.is_active());
    /// ```
    #[doc(alias = "isActive")]
    #[inline(always)]
    pub fn is_active(&self) -> S::Mask {
        self.attack | self.decay | self.sustain | self.release
    }

    /// The level most recently returned (`0.0` when idle).
    ///
    /// ```
    /// use syrp_audio_basics::control::{Adsr, AdsrParams};
    /// let mut env = Adsr::new(1000.0, AdsrParams::new(0.0_f32, 0.0, 1.0, 0.0));
    /// env.note_on(true);
    /// env.next_sample();
    /// assert_eq!(env.level(), 1.0);
    /// ```
    pub fn level(&self) -> S {
        self.level
    }

    /// Returns every lane to idle at level `0.0`.
    ///
    /// ```
    /// use syrp_audio_basics::control::{Adsr, AdsrParams};
    /// let mut env = Adsr::new(1000.0, AdsrParams::new(0.0_f32, 0.0, 1.0, 0.0));
    /// env.note_on(true);
    /// env.next_sample();
    /// env.reset();
    /// assert_eq!((env.is_active(), env.level()), (false, 0.0));
    /// ```
    pub fn reset(&mut self) {
        let off = S::Mask::splat(false);
        self.level = S::zero();
        (self.attack, self.decay, self.sustain, self.release) = (off, off, off, off);
        self.aim(S::Mask::splat(true));
    }
}

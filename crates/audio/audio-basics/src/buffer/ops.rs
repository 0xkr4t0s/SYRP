// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The cleared flag (spec B3) and the buffer operations (spec B4).
//!
//! Every operation here is allocation-free and lock-free, so it is safe on the audio thread
//! once [`Level::detect`] has run once (it picks the SIMD level `vector_ops` runs at). Frame
//! ranges are relative to the channel start; an out-of-range channel or frame range panics.

use std::ops::Range;

use syrp_simd::{Level, kernels};

use super::{AudioBuffer, BufferSample};
use crate::vector_ops;

/// The gain `start + (end - start) * i / n` that a ramp of `n` samples applies to sample `i`.
/// The ramp reaches `end` at sample `n`, the first sample of the next block, so consecutive
/// blocks join without a step.
#[inline(always)]
fn ramp_gain<T: BufferSample>(start: T, end: T, i: usize, n: usize) -> T {
    start + (end - start) * T::from_f64(i as f64 / n as f64)
}

impl<T: BufferSample> AudioBuffer<T> {
    /// The samples of `range` in channel `ch`, as indices into `data`.
    fn span(&self, ch: usize, range: Range<usize>) -> Range<usize> {
        self.check_channel(ch);
        assert!(
            range.start <= range.end && range.end <= self.frames,
            "frame range {range:?} out of bounds ({} frames)",
            self.frames
        );
        ch * self.stride + range.start..ch * self.stride + range.end
    }

    /// True when the buffer is known to be silent: it was created or [cleared](Self::clear)
    /// and nothing has written to it since. Any mutable access (`channel_mut`,
    /// `as_block_mut`) or an operation that writes non-zero samples resets it. It is a hint,
    /// not a measurement: a buffer whose samples happen to be zero may report `false`.
    ///
    /// ```
    /// let mut buf = syrp_audio_basics::AudioBuffer::<f32>::new(2, 8)?;
    /// assert!(buf.is_clear());
    /// buf.channel_mut(0)[0] = 1.0;
    /// assert!(!buf.is_clear());
    /// buf.clear();
    /// assert!(buf.is_clear());
    /// # Ok::<(), syrp_audio_basics::BufferError>(())
    /// ```
    #[doc(alias = "hasBeenCleared")]
    #[must_use]
    pub fn is_clear(&self) -> bool {
        self.cleared
    }

    /// Resets the cleared flag, for code that wrote to the samples through a path the buffer
    /// cannot see.
    ///
    /// ```
    /// let mut buf = syrp_audio_basics::AudioBuffer::<f32>::new(1, 8)?;
    /// buf.set_not_clear();
    /// assert!(!buf.is_clear());
    /// # Ok::<(), syrp_audio_basics::BufferError>(())
    /// ```
    #[doc(alias = "setNotClear")]
    pub fn set_not_clear(&mut self) {
        self.cleared = false;
    }

    /// Sets every sample to `0.0` and marks the buffer clear. Does nothing when it is clear.
    ///
    /// ```
    /// let mut buf = syrp_audio_basics::AudioBuffer::<f64>::new(1, 2)?;
    /// buf.channel_mut(0).fill(1.0);
    /// buf.clear();
    /// assert_eq!(buf.channel(0), &[0.0; 2]);
    /// # Ok::<(), syrp_audio_basics::BufferError>(())
    /// ```
    pub fn clear(&mut self) {
        self.clear_frames(0..self.frames);
        self.cleared = true;
    }

    /// Sets the frames in `range` of every channel to `0.0`. Does nothing when the buffer is
    /// clear.
    ///
    /// ```
    /// let mut buf = syrp_audio_basics::AudioBuffer::<f32>::new(2, 3)?;
    /// buf.channel_mut(1).fill(1.0);
    /// buf.clear_frames(1..3);
    /// assert_eq!(buf.channel(1), &[1.0, 0.0, 0.0]);
    /// # Ok::<(), syrp_audio_basics::BufferError>(())
    /// ```
    pub fn clear_frames(&mut self, range: Range<usize>) {
        for ch in 0..self.channels {
            self.clear_channel(ch, range.clone());
        }
    }

    /// Sets the frames in `range` of channel `ch` to `0.0`. Does nothing when the buffer is
    /// clear.
    ///
    /// ```
    /// let mut buf = syrp_audio_basics::AudioBuffer::<f32>::new(2, 3)?;
    /// buf.channel_mut(0).fill(1.0);
    /// buf.clear_channel(0, 0..2);
    /// assert_eq!(buf.channel(0), &[0.0, 0.0, 1.0]);
    /// # Ok::<(), syrp_audio_basics::BufferError>(())
    /// ```
    pub fn clear_channel(&mut self, ch: usize, range: Range<usize>) {
        let r = self.span(ch, range);
        if !self.cleared {
            vector_ops::clear(&mut self.data[r]);
        }
    }

    /// Multiplies every sample by `gain`. A gain of `1.0` does nothing; a gain of `0.0`
    /// [clears](Self::clear) the buffer.
    ///
    /// ```
    /// let mut buf = syrp_audio_basics::AudioBuffer::<f32>::new(2, 2)?;
    /// buf.channel_mut(1).fill(0.5);
    /// buf.apply_gain(2.0);
    /// assert_eq!(buf.channel(1), &[1.0; 2]);
    /// # Ok::<(), syrp_audio_basics::BufferError>(())
    /// ```
    #[doc(alias = "applyGain")]
    pub fn apply_gain(&mut self, gain: T) {
        if gain == T::zero() {
            return self.clear();
        }
        for ch in 0..self.channels {
            self.apply_gain_channel(ch, 0..self.frames, gain);
        }
    }

    /// Multiplies the frames in `range` of channel `ch` by `gain`. Skipped when the buffer is
    /// clear or `gain` is `1.0`; a gain of `0.0` clears the range.
    ///
    /// ```
    /// let mut buf = syrp_audio_basics::AudioBuffer::<f64>::new(1, 3)?;
    /// buf.channel_mut(0).fill(1.0);
    /// buf.apply_gain_channel(0, 1..3, 0.5);
    /// assert_eq!(buf.channel(0), &[1.0, 0.5, 0.5]);
    /// # Ok::<(), syrp_audio_basics::BufferError>(())
    /// ```
    #[doc(alias = "applyGain")]
    pub fn apply_gain_channel(&mut self, ch: usize, range: Range<usize>, gain: T) {
        let r = self.span(ch, range);
        if self.cleared || gain == T::one() {
            return;
        }
        if gain == T::zero() {
            vector_ops::clear(&mut self.data[r]);
        } else {
            vector_ops::multiply_scalar(Level::detect(), &mut self.data[r], gain);
        }
    }

    /// Multiplies the frames in `range` of channel `ch` by a gain that moves linearly from
    /// `start` towards `end`: sample `i` of `n` gets `start + (end - start) * i / n`, so the
    /// next block can start at `end` without a step. Skipped when the buffer is clear.
    ///
    /// ```
    /// let mut buf = syrp_audio_basics::AudioBuffer::<f32>::new(1, 4)?;
    /// buf.channel_mut(0).fill(1.0);
    /// buf.apply_gain_ramp(0, 0..4, 0.0, 1.0);
    /// assert_eq!(buf.channel(0), &[0.0, 0.25, 0.5, 0.75]);
    /// # Ok::<(), syrp_audio_basics::BufferError>(())
    /// ```
    #[doc(alias = "applyGainRamp")]
    pub fn apply_gain_ramp(&mut self, ch: usize, range: Range<usize>, start: T, end: T) {
        if start == end {
            return self.apply_gain_channel(ch, range, start);
        }
        let r = self.span(ch, range);
        if self.cleared {
            return;
        }
        let n = r.len();
        for (i, x) in self.data[r].iter_mut().enumerate() {
            *x *= ramp_gain(start, end, i, n);
        }
    }

    /// Adds `source_range` of `source`'s channel `source_ch`, times `gain`, to channel
    /// `dest_ch` from frame `dest_start`. Skipped when `source` is clear or `gain` is `0.0`;
    /// when this buffer is clear it copies instead of adding.
    ///
    /// ```
    /// use syrp_audio_basics::AudioBuffer;
    /// let mut src = AudioBuffer::<f32>::new(1, 4)?;
    /// src.channel_mut(0).fill(1.0);
    /// let mut mix = AudioBuffer::<f32>::new(2, 4)?;
    /// mix.add_from(1, 2, &src, 0, 0..2, 0.5);
    /// mix.add_from(1, 2, &src, 0, 0..2, 0.5);
    /// assert_eq!(mix.channel(1), &[0.0, 0.0, 1.0, 1.0]);
    /// # Ok::<(), syrp_audio_basics::BufferError>(())
    /// ```
    #[doc(alias = "addFrom")]
    pub fn add_from(
        &mut self,
        dest_ch: usize,
        dest_start: usize,
        source: &AudioBuffer<T>,
        source_ch: usize,
        source_range: Range<usize>,
        gain: T,
    ) {
        let d = self.span(dest_ch, dest_start..dest_start + source_range.len());
        let s = &source.data[source.span(source_ch, source_range)];
        if source.cleared || gain == T::zero() {
            return;
        }
        let (level, d) = (Level::detect(), &mut self.data[d]);
        match (self.cleared, gain == T::one()) {
            (true, true) => vector_ops::copy(d, s),
            (true, false) => vector_ops::copy_with_multiply(level, d, s, gain),
            (false, true) => vector_ops::add(level, d, s),
            (false, false) => vector_ops::add_with_multiply(level, d, s, gain),
        }
        self.cleared = false;
    }

    /// Copies `source_range` of `source`'s channel `source_ch`, times `gain`, into channel
    /// `dest_ch` from frame `dest_start`. When `source` is clear or `gain` is `0.0` the
    /// destination range is zeroed (and the buffer stays clear if it was).
    ///
    /// ```
    /// use syrp_audio_basics::AudioBuffer;
    /// let mut src = AudioBuffer::<f64>::new(1, 2)?;
    /// src.channel_mut(0).copy_from_slice(&[1.0, 2.0]);
    /// let mut dst = AudioBuffer::<f64>::new(1, 3)?;
    /// dst.copy_from(0, 1, &src, 0, 0..2, 2.0);
    /// assert_eq!(dst.channel(0), &[0.0, 2.0, 4.0]);
    /// # Ok::<(), syrp_audio_basics::BufferError>(())
    /// ```
    #[doc(alias = "copyFrom")]
    pub fn copy_from(
        &mut self,
        dest_ch: usize,
        dest_start: usize,
        source: &AudioBuffer<T>,
        source_ch: usize,
        source_range: Range<usize>,
        gain: T,
    ) {
        let d = self.span(dest_ch, dest_start..dest_start + source_range.len());
        let s = &source.data[source.span(source_ch, source_range)];
        if source.cleared || gain == T::zero() {
            if !self.cleared {
                vector_ops::clear(&mut self.data[d]);
            }
            return;
        }
        let d = &mut self.data[d];
        if gain == T::one() {
            vector_ops::copy(d, s);
        } else {
            vector_ops::copy_with_multiply(Level::detect(), d, s, gain);
        }
        self.cleared = false;
    }

    /// Adds `source`, times a gain ramp from `start` towards `end` (as in
    /// [`apply_gain_ramp`](Self::apply_gain_ramp)), to channel `dest_ch` from frame
    /// `dest_start`. Skipped when both gains are `0.0`.
    ///
    /// ```
    /// let mut buf = syrp_audio_basics::AudioBuffer::<f32>::new(1, 2)?;
    /// buf.add_from_with_ramp(0, 0, &[1.0, 1.0], 0.0, 1.0);
    /// assert_eq!(buf.channel(0), &[0.0, 0.5]);
    /// # Ok::<(), syrp_audio_basics::BufferError>(())
    /// ```
    #[doc(alias = "addFromWithRamp")]
    pub fn add_from_with_ramp(
        &mut self,
        dest_ch: usize,
        dest_start: usize,
        source: &[T],
        start: T,
        end: T,
    ) {
        let r = self.span(dest_ch, dest_start..dest_start + source.len());
        if start == T::zero() && end == T::zero() {
            return;
        }
        let n = r.len();
        for (i, (d, &s)) in self.data[r].iter_mut().zip(source).enumerate() {
            *d += s * ramp_gain(start, end, i, n);
        }
        self.cleared = false;
    }

    /// Copies `source`, times a gain ramp from `start` towards `end` (as in
    /// [`apply_gain_ramp`](Self::apply_gain_ramp)), into channel `dest_ch` from frame
    /// `dest_start`. When both gains are `0.0` the destination range is zeroed instead.
    ///
    /// ```
    /// let mut buf = syrp_audio_basics::AudioBuffer::<f64>::new(1, 2)?;
    /// buf.copy_from_with_ramp(0, 0, &[2.0, 2.0], 1.0, 0.0);
    /// assert_eq!(buf.channel(0), &[2.0, 1.0]);
    /// # Ok::<(), syrp_audio_basics::BufferError>(())
    /// ```
    #[doc(alias = "copyFromWithRamp")]
    pub fn copy_from_with_ramp(
        &mut self,
        dest_ch: usize,
        dest_start: usize,
        source: &[T],
        start: T,
        end: T,
    ) {
        let r = self.span(dest_ch, dest_start..dest_start + source.len());
        if start == T::zero() && end == T::zero() {
            if !self.cleared {
                vector_ops::clear(&mut self.data[r]);
            }
            return;
        }
        let n = r.len();
        for (i, (d, &s)) in self.data[r].iter_mut().zip(source).enumerate() {
            *d = s * ramp_gain(start, end, i, n);
        }
        self.cleared = false;
    }

    /// The peak: the largest absolute sample in `range` of channel `ch`, `0.0` for an empty
    /// range or a clear buffer.
    ///
    /// ```
    /// let mut buf = syrp_audio_basics::AudioBuffer::<f32>::new(1, 3)?;
    /// buf.channel_mut(0).copy_from_slice(&[0.5, -0.75, 0.25]);
    /// assert_eq!(buf.magnitude(0, 0..3), 0.75);
    /// # Ok::<(), syrp_audio_basics::BufferError>(())
    /// ```
    #[doc(alias = "getMagnitude")]
    #[must_use]
    pub fn magnitude(&self, ch: usize, range: Range<usize>) -> T {
        let r = self.span(ch, range);
        if self.cleared {
            return T::zero();
        }
        kernels::abs_max(Level::detect(), &self.data[r])
    }

    /// The root mean square of `range` in channel `ch`, accumulated in `f64`; `0.0` for an
    /// empty range or a clear buffer.
    ///
    /// ```
    /// let mut buf = syrp_audio_basics::AudioBuffer::<f32>::new(1, 2)?;
    /// buf.channel_mut(0).copy_from_slice(&[0.5, -0.5]);
    /// assert_eq!(buf.rms_level(0, 0..2), 0.5);
    /// # Ok::<(), syrp_audio_basics::BufferError>(())
    /// ```
    #[doc(alias = "getRMSLevel")]
    #[must_use]
    pub fn rms_level(&self, ch: usize, range: Range<usize>) -> T {
        let r = self.span(ch, range);
        if self.cleared || r.is_empty() {
            return T::zero();
        }
        let n = r.len() as f64;
        let sum: f64 = self.data[r].iter().map(|&x| x.to_f64() * x.to_f64()).sum();
        T::from_f64((sum / n).sqrt())
    }

    /// The smallest and largest samples in `range` of channel `ch`, or `None` for an empty
    /// range. A clear buffer gives `(0.0, 0.0)`.
    ///
    /// ```
    /// let mut buf = syrp_audio_basics::AudioBuffer::<f64>::new(1, 3)?;
    /// buf.channel_mut(0).copy_from_slice(&[0.5, -0.75, 0.25]);
    /// assert_eq!(buf.find_min_max(0, 0..3), Some((-0.75, 0.5)));
    /// # Ok::<(), syrp_audio_basics::BufferError>(())
    /// ```
    #[doc(alias = "findMinMax")]
    #[must_use]
    pub fn find_min_max(&self, ch: usize, range: Range<usize>) -> Option<(T, T)> {
        let r = self.span(ch, range);
        if self.cleared && !r.is_empty() {
            return Some((T::zero(), T::zero()));
        }
        vector_ops::find_min_max(Level::detect(), &self.data[r])
    }

    /// Reverses the order of the frames in `range` of channel `ch`. Skipped when the buffer
    /// is clear.
    ///
    /// ```
    /// let mut buf = syrp_audio_basics::AudioBuffer::<f32>::new(1, 3)?;
    /// buf.channel_mut(0).copy_from_slice(&[1.0, 2.0, 3.0]);
    /// buf.reverse(0, 0..2);
    /// assert_eq!(buf.channel(0), &[2.0, 1.0, 3.0]);
    /// # Ok::<(), syrp_audio_basics::BufferError>(())
    /// ```
    pub fn reverse(&mut self, ch: usize, range: Range<usize>) {
        let r = self.span(ch, range);
        if !self.cleared {
            self.data[r].reverse();
        }
    }
}

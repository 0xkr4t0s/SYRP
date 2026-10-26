// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`AudioBuffer`], the owned planar audio buffer (spec B2).
//!
//! All channels live in one [`AlignedBuf`] allocation. Channel `c` starts `c * stride` samples
//! after channel 0, every channel starts on a 64-byte boundary, and the stride in bytes is a
//! multiple of 64 (ADR 0003 D2), so the [`AudioBlock`] and [`AudioBlockMut`] views it lends
//! always report [`is_simd_aligned`](AudioBlock::is_simd_aligned). The samples between
//! `frames()` and the stride are padding.

use std::fmt;

use syrp_audio_api::{AudioBlock, AudioBlockMut, AudioSample, SIMD_ALIGN};
use syrp_error::{Diagnostic, ErrorCode};
use syrp_simd::{AlignedBuf, Zeroable, kernels};

use crate::sealed;

mod ops;

/// The sample types an [`AudioBuffer`] holds: `f32` and `f64`. Sealed.
///
/// It combines the view contract ([`AudioSample`]), aligned storage ([`Zeroable`]) and the
/// `syrp-simd` kernels ([`kernels::Element`]).
///
/// ```
/// fn silence<T: syrp_audio_basics::BufferSample>(frames: usize) -> syrp_audio_basics::AudioBuffer<T> {
///     syrp_audio_basics::AudioBuffer::new(1, frames).expect("small buffer")
/// }
/// assert_eq!(silence::<f64>(8).channel(0), &[0.0; 8]);
/// ```
pub trait BufferSample: AudioSample + Zeroable + kernels::Element + sealed::Sealed {}
impl BufferSample for f32 {}
impl BufferSample for f64 {}

/// Errors from sizing an [`AudioBuffer`], catalogued in `errors/audio.toml`.
///
/// ```
/// use syrp_error::Diagnostic;
/// let err = syrp_audio_basics::AudioBuffer::<f32>::new(usize::MAX, 2).unwrap_err();
/// assert_eq!(err.code().as_str(), "SYRP-AUDIO-0005");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BufferError {
    /// `channels` times the padded channel stride does not fit in memory: the sample count
    /// overflows `usize` or the byte size overflows `isize` (`SYRP-AUDIO-0005`).
    #[non_exhaustive]
    TooLarge {
        /// Requested channels.
        channels: usize,
        /// Requested frames per channel.
        frames: usize,
    },
}

impl fmt::Display for BufferError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            BufferError::TooLarge { channels, frames } => write!(
                f,
                "an audio buffer of {channels} channels by {frames} frames does not fit in memory"
            ),
        }
    }
}

impl std::error::Error for BufferError {}

impl Diagnostic for BufferError {
    fn code(&self) -> ErrorCode {
        ErrorCode::new(match self {
            BufferError::TooLarge { .. } => "SYRP-AUDIO-0005",
        })
    }
}

/// The channel stride and total sample count for `channels` channels of `frames` frames.
///
/// The stride is `frames` rounded up to a whole number of 64-byte groups, and at least one
/// group, so every channel (even an empty one) starts on its own 64-byte boundary.
fn layout<T>(channels: usize, frames: usize) -> Result<(usize, usize), BufferError> {
    let too_large = BufferError::TooLarge { channels, frames };
    let group = SIMD_ALIGN / size_of::<T>();
    let stride = frames
        .max(1)
        .checked_next_multiple_of(group)
        .ok_or(too_large)?;
    let len = channels.checked_mul(stride).ok_or(too_large)?;
    match len.checked_mul(size_of::<T>()) {
        Some(bytes) if bytes <= isize::MAX as usize - SIMD_ALIGN => Ok((stride, len)),
        _ => Err(too_large),
    }
}

/// Moves the first `frames` samples of channels `0..channels` from stride `from` to stride
/// `to` inside one region, in the order that never overwrites a channel not yet moved.
fn relocate<T: Copy>(data: &mut [T], channels: usize, frames: usize, from: usize, to: usize) {
    if from == to || frames == 0 {
        return;
    }
    let mut move_channel = |ch: usize| data.copy_within(ch * from..ch * from + frames, ch * to);
    if to > from {
        (0..channels).rev().for_each(&mut move_channel);
    } else {
        (0..channels).for_each(&mut move_channel);
    }
}

/// An owned buffer of planar audio: `channels()` channels of `frames()` samples of `f32` or
/// `f64`, in one 64-byte aligned allocation (ADR 0003 D2).
///
/// New buffers are zeroed. Read and write it through [`channel`](Self::channel) and
/// [`channel_mut`](Self::channel_mut), or lend it to processors as an [`AudioBlock`] or
/// [`AudioBlockMut`]. Fields are private (ADR 0003 D3). Creating, cloning, resizing past the
/// capacity and dropping a buffer allocate or free, so do them off the audio thread.
///
/// ```
/// use syrp_audio_basics::AudioBuffer;
/// let mut buf = AudioBuffer::<f32>::new(2, 100)?;
/// buf.channel_mut(0).fill(0.25);
/// let block = buf.as_block();
/// assert!(block.is_simd_aligned());
/// assert_eq!((block.channels(), block.frames()), (2, 100));
/// assert_eq!(block.channel(0)[99], 0.25);
/// # Ok::<(), syrp_audio_basics::BufferError>(())
/// ```
#[doc(alias = "AudioSampleBuffer")]
pub struct AudioBuffer<T: BufferSample> {
    data: AlignedBuf<T>,
    channels: usize,
    frames: usize,
    stride: usize,
    /// True when every sample in `0..frames` of every channel is known to be `0.0` (B3).
    cleared: bool,
}

impl<T: BufferSample> AudioBuffer<T> {
    /// A zeroed buffer of `channels` channels by `frames` frames.
    ///
    /// # Errors
    /// [`BufferError::TooLarge`] when the padded size does not fit in memory. (A failed
    /// allocation of a size that does fit aborts, as it does for `Vec`.)
    ///
    /// ```
    /// let buf = syrp_audio_basics::AudioBuffer::<f64>::new(3, 10)?;
    /// assert_eq!(buf.channel(2), &[0.0; 10]);
    /// # Ok::<(), syrp_audio_basics::BufferError>(())
    /// ```
    pub fn new(channels: usize, frames: usize) -> Result<Self, BufferError> {
        let (stride, len) = layout::<T>(channels, frames)?;
        Ok(AudioBuffer {
            data: AlignedBuf::zeroed(len),
            channels,
            frames,
            stride,
            cleared: true,
        })
    }

    /// Number of channels.
    ///
    /// ```
    /// assert_eq!(syrp_audio_basics::AudioBuffer::<f32>::new(4, 8)?.channels(), 4);
    /// # Ok::<(), syrp_audio_basics::BufferError>(())
    /// ```
    #[doc(alias = "getNumChannels")]
    #[must_use]
    pub fn channels(&self) -> usize {
        self.channels
    }

    /// Number of samples per channel.
    ///
    /// ```
    /// assert_eq!(syrp_audio_basics::AudioBuffer::<f32>::new(4, 8)?.frames(), 8);
    /// # Ok::<(), syrp_audio_basics::BufferError>(())
    /// ```
    #[doc(alias = "getNumSamples")]
    #[must_use]
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// A read-only view of every channel, borrowed from the buffer. It is always
    /// [SIMD-aligned](AudioBlock::is_simd_aligned).
    ///
    /// ```
    /// let buf = syrp_audio_basics::AudioBuffer::<f32>::new(2, 5)?;
    /// let block = buf.as_block();
    /// assert_eq!(block.stride(), 16); // 5 frames padded to 64 bytes
    /// assert_eq!(block.channel(1).len(), 5);
    /// # Ok::<(), syrp_audio_basics::BufferError>(())
    /// ```
    #[doc(alias = "getArrayOfReadPointers")]
    #[must_use]
    pub fn as_block(&self) -> AudioBlock<'_, T> {
        AudioBlock::new(&self.data, self.channels, self.frames, self.stride)
            .expect("AudioBuffer keeps channels * stride <= capacity and frames <= stride")
    }

    /// A mutable view of every channel, borrowed from the buffer. It is always
    /// [SIMD-aligned](AudioBlockMut::is_simd_aligned).
    ///
    /// ```
    /// let mut buf = syrp_audio_basics::AudioBuffer::<f64>::new(2, 4)?;
    /// buf.as_block_mut().fill(1.0);
    /// assert_eq!(buf.channel(1), &[1.0; 4]);
    /// # Ok::<(), syrp_audio_basics::BufferError>(())
    /// ```
    #[doc(alias = "getArrayOfWritePointers")]
    #[must_use]
    pub fn as_block_mut(&mut self) -> AudioBlockMut<'_, T> {
        self.cleared = false;
        AudioBlockMut::new(&mut self.data, self.channels, self.frames, self.stride)
            .expect("AudioBuffer keeps channels * stride <= capacity and frames <= stride")
    }

    /// Channel `ch`, `frames()` samples long.
    ///
    /// # Panics
    /// When `ch >= channels()`.
    ///
    /// ```
    /// let buf = syrp_audio_basics::AudioBuffer::<f32>::new(1, 3)?;
    /// assert_eq!(buf.channel(0), &[0.0; 3]);
    /// # Ok::<(), syrp_audio_basics::BufferError>(())
    /// ```
    #[doc(alias = "getReadPointer")]
    #[must_use]
    pub fn channel(&self, ch: usize) -> &[T] {
        self.check_channel(ch);
        &self.data[ch * self.stride..][..self.frames]
    }

    /// Channel `ch` for writing, `frames()` samples long.
    ///
    /// # Panics
    /// When `ch >= channels()`.
    ///
    /// ```
    /// let mut buf = syrp_audio_basics::AudioBuffer::<f32>::new(2, 3)?;
    /// buf.channel_mut(1)[2] = 0.5;
    /// assert_eq!(buf.channel(1), &[0.0, 0.0, 0.5]);
    /// # Ok::<(), syrp_audio_basics::BufferError>(())
    /// ```
    #[doc(alias = "getWritePointer")]
    pub fn channel_mut(&mut self, ch: usize) -> &mut [T] {
        self.check_channel(ch);
        self.cleared = false;
        &mut self.data[ch * self.stride..][..self.frames]
    }

    fn check_channel(&self, ch: usize) {
        assert!(
            ch < self.channels,
            "channel {ch} out of range ({} channels)",
            self.channels
        );
    }

    /// Changes the number of channels and frames, as JUCE documents `AudioBuffer::setSize`.
    ///
    /// - `keep_existing`: the overlapping region (the first `min` channels and `min` frames of
    ///   the old and new shapes) keeps its samples. Otherwise the content is unspecified.
    /// - `clear_extra`: samples outside the kept region are set to `0.0` (with
    ///   `keep_existing` false, that is the whole buffer). Otherwise they are unspecified,
    ///   though always initialised: they hold zeros or stale samples.
    /// - `avoid_reallocating`: the allocation is reused, never shrunk, whenever the new shape
    ///   fits in it; then the call never allocates and is safe on the audio thread. Without
    ///   it, the buffer reallocates unless the new shape needs exactly the current capacity,
    ///   so it uses the minimum memory.
    ///
    /// A newly allocated region always starts zeroed. The buffer stays
    /// [clear](Self::is_clear) when it was clear (or nothing was kept) and no stale samples
    /// can remain.
    ///
    /// # Errors
    /// [`BufferError::TooLarge`] when the new size does not fit in memory; the buffer is then
    /// unchanged. (A failed allocation of a size that does fit aborts, as it does for `Vec`.)
    ///
    /// ```
    /// let mut buf = syrp_audio_basics::AudioBuffer::<f32>::new(1, 4)?;
    /// buf.channel_mut(0).copy_from_slice(&[1.0, 2.0, 3.0, 4.0]);
    /// buf.set_size(2, 6, true, true, true)?;
    /// assert_eq!(buf.channel(0), &[1.0, 2.0, 3.0, 4.0, 0.0, 0.0]);
    /// assert_eq!(buf.channel(1), &[0.0; 6]);
    /// # Ok::<(), syrp_audio_basics::BufferError>(())
    /// ```
    #[doc(alias = "setSize")]
    pub fn set_size(
        &mut self,
        channels: usize,
        frames: usize,
        keep_existing: bool,
        clear_extra: bool,
        avoid_reallocating: bool,
    ) -> Result<(), BufferError> {
        let (stride, len) = layout::<T>(channels, frames)?;
        let keep_channels = if keep_existing {
            self.channels.min(channels)
        } else {
            0
        };
        let keep_frames = self.frames.min(frames);
        let kept_nothing = keep_channels == 0 || keep_frames == 0;
        let fits = if avoid_reallocating {
            len <= self.data.len()
        } else {
            len == self.data.len()
        };
        if fits {
            relocate(
                &mut self.data,
                keep_channels,
                keep_frames,
                self.stride,
                stride,
            );
            if clear_extra {
                for ch in 0..channels {
                    let from = if ch < keep_channels { keep_frames } else { 0 };
                    self.data[ch * stride + from..ch * stride + frames].fill(T::default());
                }
            }
        } else {
            let mut data = AlignedBuf::zeroed(len);
            for ch in 0..keep_channels {
                data[ch * stride..][..keep_frames]
                    .copy_from_slice(&self.data[ch * self.stride..][..keep_frames]);
            }
            self.data = data;
        }
        self.channels = channels;
        self.frames = frames;
        self.stride = stride;
        // Known silent only if nothing non-zero was kept and nothing stale is left.
        self.cleared = (self.cleared || kept_nothing) && (clear_extra || !fits);
        Ok(())
    }

    /// Makes this buffer a copy of `other`, resizing it to `other`'s shape and converting
    /// each sample between `f32` and `f64` (rounding to nearest when narrowing), as JUCE
    /// documents `AudioBuffer::makeCopyOf`. With `avoid_reallocating`, the resize reuses the
    /// allocation when it fits, as in [`set_size`](Self::set_size).
    ///
    /// # Errors
    /// [`BufferError::TooLarge`], as for [`set_size`](Self::set_size); allocation failure
    /// aborts.
    ///
    /// ```
    /// use syrp_audio_basics::AudioBuffer;
    /// let mut wide = AudioBuffer::<f64>::new(1, 2)?;
    /// wide.channel_mut(0).copy_from_slice(&[0.5, -0.25]);
    /// let mut narrow = AudioBuffer::<f32>::new(0, 0)?;
    /// narrow.make_copy_of(&wide, false)?;
    /// assert_eq!(narrow.channel(0), &[0.5_f32, -0.25]);
    /// # Ok::<(), syrp_audio_basics::BufferError>(())
    /// ```
    #[doc(alias = "makeCopyOf")]
    pub fn make_copy_of<U: BufferSample>(
        &mut self,
        other: &AudioBuffer<U>,
        avoid_reallocating: bool,
    ) -> Result<(), BufferError> {
        self.set_size(
            other.channels,
            other.frames,
            false,
            false,
            avoid_reallocating,
        )?;
        if other.cleared {
            self.clear();
            return Ok(());
        }
        for ch in 0..self.channels {
            let src = other.channel(ch);
            for (d, &s) in self.channel_mut(ch).iter_mut().zip(src) {
                *d = T::from_f64(s.to_f64());
            }
        }
        Ok(())
    }
}

impl<T: BufferSample> Default for AudioBuffer<T> {
    /// An empty buffer: no channels, no frames, no allocation.
    ///
    /// ```
    /// let buf = syrp_audio_basics::AudioBuffer::<f32>::default();
    /// assert_eq!((buf.channels(), buf.frames()), (0, 0));
    /// ```
    fn default() -> Self {
        AudioBuffer {
            data: AlignedBuf::zeroed(0),
            channels: 0,
            frames: 0,
            stride: SIMD_ALIGN / size_of::<T>(),
            cleared: true,
        }
    }
}

impl<T: BufferSample> Clone for AudioBuffer<T> {
    /// A buffer of the same shape and samples, in a new allocation of minimum size.
    ///
    /// ```
    /// let mut a = syrp_audio_basics::AudioBuffer::<f32>::new(1, 2)?;
    /// a.channel_mut(0)[1] = 1.0;
    /// assert_eq!(a.clone().channel(0), &[0.0, 1.0]);
    /// # Ok::<(), syrp_audio_basics::BufferError>(())
    /// ```
    fn clone(&self) -> Self {
        let len = self.channels * self.stride;
        let mut data = AlignedBuf::zeroed(len);
        data.copy_from_slice(&self.data[..len]);
        AudioBuffer {
            data,
            channels: self.channels,
            frames: self.frames,
            stride: self.stride,
            cleared: self.cleared,
        }
    }
}

impl<T: BufferSample> fmt::Debug for AudioBuffer<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AudioBuffer")
            .field("channels", &self.channels)
            .field("frames", &self.frames)
            .field("stride", &self.stride)
            .field("capacity", &self.data.len())
            .field("cleared", &self.cleared)
            .finish_non_exhaustive()
    }
}

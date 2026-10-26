// SPDX-License-Identifier: MIT OR Apache-2.0
//! Non-owning planar buffer views (spec B1 and B5).
//!
//! Every view describes `channels` channels of `frames` samples. Channel `c` starts `c * stride`
//! samples after channel 0, so all channels live in one region (ADR 0003 D2). The samples between
//! `frames` and `stride` are padding that SIMD kernels may read and overwrite.

use std::fmt;
use std::marker::PhantomData;
use std::ops::Range;
use std::ptr::NonNull;
use std::slice;

use syrp_rt::RtSafe;

use crate::{AudioError, Precision};

/// The alignment, in bytes, that owning buffers give every channel start and channel stride.
/// ```
/// assert_eq!(syrp_audio_api::SIMD_ALIGN, 64);
/// ```
pub const SIMD_ALIGN: usize = 64;

mod sealed {
    pub trait Sealed {}
    impl Sealed for f32 {}
    impl Sealed for f64 {}
}

/// A sample type that audio views can hold: `f32` or `f64` (sealed).
///
/// This is only the storage type of a buffer. Lane-generic DSP uses the `Sample` trait from
/// `syrp-simd` (ADR 0003 D5).
/// ```
/// use syrp_audio_api::{AudioSample, Precision};
/// assert_eq!(<f64 as AudioSample>::PRECISION, Precision::Double);
/// ```
pub trait AudioSample:
    sealed::Sealed + Copy + Default + PartialEq + fmt::Debug + Send + Sync + RtSafe + 'static
{
    /// The processing precision this sample type stands for.
    const PRECISION: Precision;
}

impl AudioSample for f32 {
    const PRECISION: Precision = Precision::Single;
}

impl AudioSample for f64 {
    const PRECISION: Precision = Precision::Double;
}

/// The shape shared by both view kinds.
///
/// Invariants: `frames <= padded <= stride`; for every channel `c < channels`, the `padded`
/// samples from `ptr + c * stride` are valid for the view's lifetime (and its kind of access).
#[derive(Debug, Clone, Copy)]
struct Raw<T> {
    ptr: NonNull<T>,
    channels: usize,
    frames: usize,
    stride: usize,
    padded: usize,
}

impl<T> Raw<T> {
    /// Checks a slice-backed layout of `len` samples.
    fn check(len: usize, channels: usize, frames: usize, stride: usize) -> Result<(), AudioError> {
        if frames > stride {
            return Err(AudioError::FramesExceedStride { frames, stride });
        }
        let needed = channels.saturating_mul(stride);
        if len < needed {
            return Err(AudioError::BufferTooShort { needed, len });
        }
        Ok(())
    }

    fn new(ptr: *mut T, channels: usize, frames: usize, stride: usize) -> Self {
        assert!(
            frames <= stride,
            "{frames} frames exceed the stride of {stride}"
        );
        Raw {
            ptr: NonNull::new(ptr).unwrap_or(NonNull::dangling()),
            channels,
            frames,
            stride,
            padded: stride,
        }
    }

    /// The first sample of channel `ch`. Panics when `ch` is out of range.
    fn start(&self, ch: usize) -> *mut T {
        assert!(
            ch < self.channels,
            "channel {ch} out of range ({} channels)",
            self.channels
        );
        self.ptr.as_ptr().wrapping_add(ch * self.stride)
    }

    fn slice_frames(&self, r: Range<usize>) -> Self {
        assert!(
            r.start <= r.end && r.end <= self.frames,
            "frame range {r:?} out of bounds"
        );
        // Padding stays usable only when the range reaches the end of the view: anything else
        // after the range is samples the caller asked us to leave alone.
        let padded = if r.end == self.frames {
            self.padded
        } else {
            r.end
        };
        Raw {
            ptr: NonNull::new(self.ptr.as_ptr().wrapping_add(r.start)).unwrap_or(self.ptr),
            channels: self.channels,
            frames: r.end - r.start,
            stride: self.stride,
            padded: padded - r.start,
        }
    }

    fn slice_channels(&self, r: Range<usize>) -> Self {
        assert!(
            r.start <= r.end && r.end <= self.channels,
            "channel range {r:?} out of bounds"
        );
        Raw {
            ptr: NonNull::new(self.ptr.as_ptr().wrapping_add(r.start * self.stride))
                .unwrap_or(self.ptr),
            channels: r.end - r.start,
            ..*self
        }
    }

    fn is_simd_aligned(&self) -> bool {
        self.channels == 0
            || (self.ptr.as_ptr() as usize).is_multiple_of(SIMD_ALIGN)
                && (self.stride * size_of::<T>()).is_multiple_of(SIMD_ALIGN)
    }
}

/// A read-only view of planar audio: `channels()` channels of `frames()` samples, `stride()`
/// samples apart in one region. Copying or slicing a view never allocates.
/// ```
/// use syrp_audio_api::AudioBlock;
/// let data = [1.0_f32, 2.0, 0.0, 0.0, 3.0, 4.0, 0.0, 0.0];
/// let block = AudioBlock::new(&data, 2, 2, 4)?;
/// assert_eq!(block.channel(1), &[3.0, 4.0]);
/// assert_eq!(block.slice_frames(1..2).channel(0), &[2.0]);
/// # Ok::<(), syrp_audio_api::AudioError>(())
/// ```
#[doc(alias = "dsp::AudioBlock")]
#[derive(Debug, Clone, Copy)]
pub struct AudioBlock<'a, T> {
    raw: Raw<T>,
    _p: PhantomData<&'a [T]>,
}

/// A mutable view of planar audio, with the same layout as [`AudioBlock`].
/// ```
/// use syrp_audio_api::AudioBlockMut;
/// let mut data = vec![0.0_f64; 2 * 8];
/// let mut block = AudioBlockMut::new(&mut data, 2, 6, 8)?;
/// for ch in block.split_channels_mut() {
///     ch.fill(0.5);
/// }
/// assert_eq!(data[8..14], [0.5; 6]);
/// assert_eq!(data[14], 0.0); // padding untouched
/// # Ok::<(), syrp_audio_api::AudioError>(())
/// ```
#[doc(alias = "AudioBuffer")]
pub struct AudioBlockMut<'a, T> {
    raw: Raw<T>,
    _p: PhantomData<&'a mut [T]>,
}

/// Accessors shared by both view kinds.
///
/// A macro because the two view types need the same five documented methods; a shared trait
/// would force callers to import it for `frames()`, and copies would drift apart.
macro_rules! shape_accessors {
    () => {
        /// Number of channels.
        #[must_use]
        pub fn channels(&self) -> usize {
            self.raw.channels
        }

        /// Number of samples per channel.
        #[must_use]
        pub fn frames(&self) -> usize {
            self.raw.frames
        }

        /// Distance between the starts of adjacent channels, in samples.
        #[must_use]
        pub fn stride(&self) -> usize {
            self.raw.stride
        }

        /// Length of the padded channel slices: `stride()` for a view of a whole buffer.
        /// A frame sub-range keeps the padding only when it reaches the end of its parent;
        /// otherwise this equals `frames()`.
        #[must_use]
        pub fn padded_frames(&self) -> usize {
            self.raw.padded
        }

        /// True when every channel starts on a [`SIMD_ALIGN`] boundary and the stride in bytes
        /// is a multiple of it (16 `f32` or 8 `f64` samples). Owning buffers always produce
        /// aligned views; frame sub-ranges may not be. A view with no channels is aligned.
        #[must_use]
        pub fn is_simd_aligned(&self) -> bool {
            self.raw.is_simd_aligned()
        }
    };
}

impl<'a, T: AudioSample> AudioBlock<'a, T> {
    /// A view over `data`, which must hold at least `channels * stride` samples, with
    /// `frames <= stride`.
    /// ```
    /// use syrp_audio_api::AudioBlock;
    /// assert!(AudioBlock::new(&[0.0_f32; 8], 2, 4, 4).is_ok());
    /// assert!(AudioBlock::new(&[0.0_f32; 7], 2, 4, 4).is_err());
    /// ```
    pub fn new(
        data: &'a [T],
        channels: usize,
        frames: usize,
        stride: usize,
    ) -> Result<Self, AudioError> {
        Raw::<T>::check(data.len(), channels, frames, stride)?;
        Ok(Self::wrap(Raw::new(
            data.as_ptr().cast_mut(),
            channels,
            frames,
            stride,
        )))
    }

    /// A view over memory the caller vouches for, without the SIMD alignment contract.
    ///
    /// # Panics
    /// If `frames > stride`.
    ///
    /// # Safety
    /// `ptr` must be non-null and aligned for `T`, even when `channels` or `stride` is 0.
    /// For every channel `c < channels`, the `stride` samples starting at `ptr + c * stride`
    /// must be initialised, valid for reads and not written through any other path for `'a`.
    /// ```
    /// let data = [0.0_f32; 4];
    /// // SAFETY: `data` holds 1 channel of `stride` 4 samples and outlives the view.
    /// let block = unsafe { syrp_audio_api::AudioBlock::from_raw_parts(data.as_ptr(), 1, 3, 4) };
    /// assert_eq!(block.frames(), 3);
    /// ```
    #[must_use]
    pub unsafe fn from_raw_parts(
        ptr: *const T,
        channels: usize,
        frames: usize,
        stride: usize,
    ) -> Self {
        Self::wrap(Raw::new(ptr.cast_mut(), channels, frames, stride))
    }

    /// Like [`from_raw_parts`](Self::from_raw_parts), for memory that meets the alignment
    /// contract ([`is_simd_aligned`](Self::is_simd_aligned)).
    ///
    /// # Panics
    /// In debug builds, if the layout is not SIMD-aligned. Always, if `frames > stride`.
    ///
    /// # Safety
    /// As for [`from_raw_parts`](Self::from_raw_parts).
    /// ```
    /// #[repr(C, align(64))]
    /// struct Aligned([f32; 32]);
    /// let data = Aligned([0.0; 32]);
    /// // SAFETY: two channels of 16 samples, 64-byte aligned, alive for the view.
    /// let block = unsafe { syrp_audio_api::AudioBlock::from_raw_parts_aligned(data.0.as_ptr(), 2, 10, 16) };
    /// assert!(block.is_simd_aligned());
    /// ```
    #[must_use]
    pub unsafe fn from_raw_parts_aligned(
        ptr: *const T,
        channels: usize,
        frames: usize,
        stride: usize,
    ) -> Self {
        // SAFETY: forwarded from the caller.
        let view = unsafe { Self::from_raw_parts(ptr, channels, frames, stride) };
        debug_assert!(
            view.is_simd_aligned(),
            "audio view breaks the 64-byte alignment contract"
        );
        view
    }

    fn wrap(raw: Raw<T>) -> Self {
        AudioBlock {
            raw,
            _p: PhantomData,
        }
    }

    shape_accessors!();

    /// Channel `ch`, `frames()` samples long. Panics when `ch >= channels()`.
    #[must_use]
    pub fn channel(&self, ch: usize) -> &'a [T] {
        // SAFETY: `start` checked `ch`; the `Raw` invariant covers `frames <= padded` samples.
        unsafe { slice::from_raw_parts(self.raw.start(ch), self.raw.frames) }
    }

    /// Channel `ch` including padding, `padded_frames()` samples long, for SIMD kernels.
    #[must_use]
    pub fn channel_padded(&self, ch: usize) -> &'a [T] {
        // SAFETY: as in `channel`, with the `padded` length the invariant covers.
        unsafe { slice::from_raw_parts(self.raw.start(ch), self.raw.padded) }
    }

    /// The channels in order, each `frames()` samples long.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &'a [T]> + use<'a, T> {
        let view = *self;
        (0..view.channels()).map(move |ch| view.channel(ch))
    }

    /// The frames in `range` of every channel. Panics when the range is out of bounds.
    #[doc(alias = "getSubBlock")]
    #[must_use]
    pub fn slice_frames(&self, range: Range<usize>) -> AudioBlock<'a, T> {
        Self::wrap(self.raw.slice_frames(range))
    }

    /// The channels in `range`. Panics when the range is out of bounds.
    #[doc(alias = "getSubsetChannelBlock")]
    #[must_use]
    pub fn slice_channels(&self, range: Range<usize>) -> AudioBlock<'a, T> {
        Self::wrap(self.raw.slice_channels(range))
    }
}

impl<'a, T: AudioSample> AudioBlockMut<'a, T> {
    /// A mutable view over `data`, which must hold at least `channels * stride` samples, with
    /// `frames <= stride`.
    /// ```
    /// let mut data = [0.0_f32; 8];
    /// assert!(syrp_audio_api::AudioBlockMut::new(&mut data, 2, 5, 4).is_err());
    /// ```
    pub fn new(
        data: &'a mut [T],
        channels: usize,
        frames: usize,
        stride: usize,
    ) -> Result<Self, AudioError> {
        Raw::<T>::check(data.len(), channels, frames, stride)?;
        Ok(Self::wrap(Raw::new(
            data.as_mut_ptr(),
            channels,
            frames,
            stride,
        )))
    }

    /// A mutable view over memory the caller vouches for, without the SIMD alignment contract.
    ///
    /// # Panics
    /// If `frames > stride`.
    ///
    /// # Safety
    /// `ptr` must be non-null and aligned for `T`, even when `channels` or `stride` is 0.
    /// For every channel `c < channels`, the `stride` samples starting at `ptr + c * stride`
    /// must be initialised, valid for reads and writes, and not accessed through any other
    /// path for `'a`.
    /// ```
    /// let mut data = [0.0_f32; 4];
    /// // SAFETY: one channel of `stride` 4, borrowed exclusively by the view.
    /// let mut block = unsafe { syrp_audio_api::AudioBlockMut::from_raw_parts(data.as_mut_ptr(), 1, 4, 4) };
    /// block.channel_mut(0)[3] = 1.0;
    /// assert_eq!(data[3], 1.0);
    /// ```
    #[must_use]
    pub unsafe fn from_raw_parts(
        ptr: *mut T,
        channels: usize,
        frames: usize,
        stride: usize,
    ) -> Self {
        Self::wrap(Raw::new(ptr, channels, frames, stride))
    }

    /// Like [`from_raw_parts`](Self::from_raw_parts), for memory that meets the alignment
    /// contract ([`is_simd_aligned`](Self::is_simd_aligned)).
    ///
    /// # Panics
    /// In debug builds, if the layout is not SIMD-aligned. Always, if `frames > stride`.
    ///
    /// # Safety
    /// As for [`from_raw_parts`](Self::from_raw_parts).
    /// ```
    /// #[repr(C, align(64))]
    /// struct Aligned([f64; 16]);
    /// let mut data = Aligned([0.0; 16]);
    /// // SAFETY: two channels of 8 samples, 64-byte aligned, borrowed exclusively.
    /// let block = unsafe { syrp_audio_api::AudioBlockMut::from_raw_parts_aligned(data.0.as_mut_ptr(), 2, 8, 8) };
    /// assert!(block.is_simd_aligned());
    /// ```
    #[must_use]
    pub unsafe fn from_raw_parts_aligned(
        ptr: *mut T,
        channels: usize,
        frames: usize,
        stride: usize,
    ) -> Self {
        // SAFETY: forwarded from the caller.
        let view = unsafe { Self::from_raw_parts(ptr, channels, frames, stride) };
        debug_assert!(
            view.is_simd_aligned(),
            "audio view breaks the 64-byte alignment contract"
        );
        view
    }

    fn wrap(raw: Raw<T>) -> Self {
        AudioBlockMut {
            raw,
            _p: PhantomData,
        }
    }

    shape_accessors!();

    /// A read-only view of the same samples, borrowed from `self`.
    #[must_use]
    pub fn as_block(&self) -> AudioBlock<'_, T> {
        AudioBlock::wrap(self.raw)
    }

    /// A shorter-lived mutable view of the same samples, so `self` can be used again later.
    #[must_use]
    pub fn reborrow(&mut self) -> AudioBlockMut<'_, T> {
        Self::wrap(self.raw)
    }

    /// Channel `ch`, `frames()` samples long. Panics when `ch >= channels()`.
    #[must_use]
    pub fn channel(&self, ch: usize) -> &[T] {
        self.as_block().channel(ch)
    }

    /// Channel `ch` for writing, `frames()` samples long. Panics when `ch >= channels()`.
    #[doc(alias = "getWritePointer")]
    pub fn channel_mut(&mut self, ch: usize) -> &mut [T] {
        // SAFETY: `start` checked `ch`; `&mut self` makes this the only live borrow.
        unsafe { slice::from_raw_parts_mut(self.raw.start(ch), self.raw.frames) }
    }

    /// Channel `ch` for writing, including padding (`padded_frames()` samples), for SIMD
    /// kernels that run past `frames()` in whole vectors.
    pub fn channel_padded_mut(&mut self, ch: usize) -> &mut [T] {
        // SAFETY: as in `channel_mut`, with the `padded` length the invariant covers.
        unsafe { slice::from_raw_parts_mut(self.raw.start(ch), self.raw.padded) }
    }

    /// Every channel at once as disjoint mutable slices, `frames()` samples each.
    pub fn split_channels_mut(&mut self) -> ChannelsMut<'_, T> {
        ChannelsMut {
            raw: self.raw,
            next: 0,
            _p: PhantomData,
        }
    }

    /// The frames in `range` of every channel. Panics when the range is out of bounds.
    #[must_use]
    pub fn slice_frames_mut(&mut self, range: Range<usize>) -> AudioBlockMut<'_, T> {
        Self::wrap(self.raw.slice_frames(range))
    }

    /// The channels in `range`. Panics when the range is out of bounds.
    #[must_use]
    pub fn slice_channels_mut(&mut self, range: Range<usize>) -> AudioBlockMut<'_, T> {
        Self::wrap(self.raw.slice_channels(range))
    }

    /// Sets the first `frames()` samples of every channel to `value` (padding is untouched).
    #[doc(alias = "clear")]
    pub fn fill(&mut self, value: T) {
        self.split_channels_mut().for_each(|ch| ch.fill(value));
    }
}

impl<T: fmt::Debug> fmt::Debug for AudioBlockMut<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AudioBlockMut")
            .field("raw", &self.raw)
            .finish()
    }
}

/// Iterator over the disjoint mutable channels of an [`AudioBlockMut`]; see
/// [`AudioBlockMut::split_channels_mut`].
/// ```
/// let mut data = [0.0_f32; 6];
/// let mut block = syrp_audio_api::AudioBlockMut::new(&mut data, 3, 2, 2)?;
/// let mut it = block.split_channels_mut();
/// let (a, b) = (it.next().unwrap(), it.next().unwrap());
/// b.copy_from_slice(a);
/// assert_eq!(it.len(), 1);
/// # Ok::<(), syrp_audio_api::AudioError>(())
/// ```
pub struct ChannelsMut<'b, T> {
    raw: Raw<T>,
    next: usize,
    _p: PhantomData<&'b mut [T]>,
}

impl<'b, T> Iterator for ChannelsMut<'b, T> {
    type Item = &'b mut [T];

    fn next(&mut self) -> Option<&'b mut [T]> {
        if self.next == self.raw.channels {
            return None;
        }
        let start = self.raw.start(self.next);
        self.next += 1;
        // SAFETY: each channel is yielded once and channels do not overlap (`frames <= stride`);
        // the iterator borrows the view mutably for `'b`.
        Some(unsafe { slice::from_raw_parts_mut(start, self.raw.frames) })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.raw.channels - self.next;
        (n, Some(n))
    }
}

impl<T> ExactSizeIterator for ChannelsMut<'_, T> {}

// SAFETY: an `AudioBlock` is a shared borrow of `[T]` (like `&[T]`).
unsafe impl<T: Sync> Send for AudioBlock<'_, T> {}
// SAFETY: as above.
unsafe impl<T: Sync> Sync for AudioBlock<'_, T> {}
// SAFETY: an `AudioBlockMut` is an exclusive borrow of `[T]` (like `&mut [T]`).
unsafe impl<T: Send> Send for AudioBlockMut<'_, T> {}
// SAFETY: as above; `&AudioBlockMut` only hands out shared slices.
unsafe impl<T: Sync> Sync for AudioBlockMut<'_, T> {}
// SAFETY: as for `AudioBlockMut`.
unsafe impl<T: Send> Send for ChannelsMut<'_, T> {}

// SAFETY: views are a pointer and four integers with no destructor; no method allocates,
// locks or blocks.
unsafe impl<T: AudioSample> RtSafe for AudioBlock<'_, T> {}
// SAFETY: as above.
unsafe impl<T: AudioSample> RtSafe for AudioBlockMut<'_, T> {}
// SAFETY: as above.
unsafe impl<T: AudioSample> RtSafe for ChannelsMut<'_, T> {}

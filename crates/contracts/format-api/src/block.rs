// SPDX-License-Identifier: MIT OR Apache-2.0
//! Which `syrp-audio-api` view a read or write carries.
//!
//! These enums own no samples and add no layout: each variant is an `AudioBlock` or
//! `AudioBlockMut` exactly as `syrp-audio-api` defines it. They exist so the reader and writer
//! traits stay object-safe while accepting both `f32` and `f64`.

use std::ops::Range;

use syrp_audio_api::{AudioBlock, AudioBlockMut, Precision};

/// A read-only `f32` or `f64` view, the source of [`AudioWriter::write`](crate::AudioWriter::write).
///
/// ```
/// use syrp_audio_api::{AudioBlock, Precision};
/// use syrp_format_api::AnyBlock;
/// let data = [0.0_f64; 8];
/// let any = AnyBlock::from(AudioBlock::new(&data, 2, 4, 4)?);
/// assert_eq!((any.channels(), any.frames()), (2, 4));
/// assert_eq!(any.precision(), Precision::Double);
/// # Ok::<(), syrp_audio_api::AudioError>(())
/// ```
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum AnyBlock<'a> {
    /// Single-precision samples.
    F32(AudioBlock<'a, f32>),
    /// Double-precision samples.
    F64(AudioBlock<'a, f64>),
}

impl AnyBlock<'_> {
    /// Number of channels.
    ///
    /// ```
    /// # use syrp_format_api::AnyBlock;
    /// let any = AnyBlock::from(syrp_audio_api::AudioBlock::new(&[0.0_f32; 3], 3, 1, 1)?);
    /// assert_eq!(any.channels(), 3);
    /// # Ok::<(), syrp_audio_api::AudioError>(())
    /// ```
    #[must_use]
    pub fn channels(&self) -> usize {
        match self {
            Self::F32(b) => b.channels(),
            Self::F64(b) => b.channels(),
        }
    }

    /// Number of samples per channel.
    ///
    /// ```
    /// # use syrp_format_api::AnyBlock;
    /// let any = AnyBlock::from(syrp_audio_api::AudioBlock::new(&[0.0_f32; 3], 1, 3, 3)?);
    /// assert_eq!(any.frames(), 3);
    /// # Ok::<(), syrp_audio_api::AudioError>(())
    /// ```
    #[must_use]
    pub fn frames(&self) -> usize {
        match self {
            Self::F32(b) => b.frames(),
            Self::F64(b) => b.frames(),
        }
    }

    /// The sample type the view holds.
    ///
    /// ```
    /// # use syrp_format_api::AnyBlock;
    /// let any = AnyBlock::from(syrp_audio_api::AudioBlock::new(&[0.0_f32; 1], 1, 1, 1)?);
    /// assert_eq!(any.precision(), syrp_audio_api::Precision::Single);
    /// # Ok::<(), syrp_audio_api::AudioError>(())
    /// ```
    #[must_use]
    pub fn precision(&self) -> Precision {
        match self {
            Self::F32(_) => Precision::Single,
            Self::F64(_) => Precision::Double,
        }
    }
}

impl<'a> From<AudioBlock<'a, f32>> for AnyBlock<'a> {
    fn from(b: AudioBlock<'a, f32>) -> Self {
        Self::F32(b)
    }
}

impl<'a> From<AudioBlock<'a, f64>> for AnyBlock<'a> {
    fn from(b: AudioBlock<'a, f64>) -> Self {
        Self::F64(b)
    }
}

/// A mutable `f32` or `f64` view, the destination of [`AudioReader::read`](crate::AudioReader::read).
///
/// ```
/// use syrp_audio_api::AudioBlockMut;
/// use syrp_format_api::AnyBlockMut;
/// let mut data = [1.0_f32; 8];
/// let mut any = AnyBlockMut::from(AudioBlockMut::new(&mut data, 2, 4, 4)?);
/// any.slice_frames_mut(2..4).clear();
/// assert_eq!(data, [1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0]);
/// # Ok::<(), syrp_audio_api::AudioError>(())
/// ```
#[derive(Debug)]
#[non_exhaustive]
pub enum AnyBlockMut<'a> {
    /// Single-precision samples.
    F32(AudioBlockMut<'a, f32>),
    /// Double-precision samples.
    F64(AudioBlockMut<'a, f64>),
}

impl AnyBlockMut<'_> {
    /// Number of channels.
    ///
    /// ```
    /// # use syrp_format_api::AnyBlockMut;
    /// let mut d = [0.0_f64; 4];
    /// let any = AnyBlockMut::from(syrp_audio_api::AudioBlockMut::new(&mut d, 4, 1, 1)?);
    /// assert_eq!(any.channels(), 4);
    /// # Ok::<(), syrp_audio_api::AudioError>(())
    /// ```
    #[must_use]
    pub fn channels(&self) -> usize {
        match self {
            Self::F32(b) => b.channels(),
            Self::F64(b) => b.channels(),
        }
    }

    /// Number of samples per channel.
    ///
    /// ```
    /// # use syrp_format_api::AnyBlockMut;
    /// let mut d = [0.0_f64; 4];
    /// let any = AnyBlockMut::from(syrp_audio_api::AudioBlockMut::new(&mut d, 1, 4, 4)?);
    /// assert_eq!(any.frames(), 4);
    /// # Ok::<(), syrp_audio_api::AudioError>(())
    /// ```
    #[must_use]
    pub fn frames(&self) -> usize {
        match self {
            Self::F32(b) => b.frames(),
            Self::F64(b) => b.frames(),
        }
    }

    /// The sample type the view holds.
    ///
    /// ```
    /// # use syrp_format_api::AnyBlockMut;
    /// let mut d = [0.0_f64; 1];
    /// let any = AnyBlockMut::from(syrp_audio_api::AudioBlockMut::new(&mut d, 1, 1, 1)?);
    /// assert_eq!(any.precision(), syrp_audio_api::Precision::Double);
    /// # Ok::<(), syrp_audio_api::AudioError>(())
    /// ```
    #[must_use]
    pub fn precision(&self) -> Precision {
        match self {
            Self::F32(_) => Precision::Single,
            Self::F64(_) => Precision::Double,
        }
    }

    /// The frames in `range` of every channel. Panics when the range is out of bounds.
    ///
    /// ```
    /// # use syrp_format_api::AnyBlockMut;
    /// let mut d = [0.0_f32; 4];
    /// let mut any = AnyBlockMut::from(syrp_audio_api::AudioBlockMut::new(&mut d, 1, 4, 4)?);
    /// assert_eq!(any.slice_frames_mut(1..3).frames(), 2);
    /// # Ok::<(), syrp_audio_api::AudioError>(())
    /// ```
    #[must_use]
    pub fn slice_frames_mut(&mut self, range: Range<usize>) -> AnyBlockMut<'_> {
        match self {
            Self::F32(b) => AnyBlockMut::F32(b.slice_frames_mut(range)),
            Self::F64(b) => AnyBlockMut::F64(b.slice_frames_mut(range)),
        }
    }

    /// The channels in `range`. Panics when the range is out of bounds.
    ///
    /// ```
    /// # use syrp_format_api::AnyBlockMut;
    /// let mut d = [0.0_f32; 4];
    /// let mut any = AnyBlockMut::from(syrp_audio_api::AudioBlockMut::new(&mut d, 4, 1, 1)?);
    /// assert_eq!(any.slice_channels_mut(1..4).channels(), 3);
    /// # Ok::<(), syrp_audio_api::AudioError>(())
    /// ```
    #[must_use]
    pub fn slice_channels_mut(&mut self, range: Range<usize>) -> AnyBlockMut<'_> {
        match self {
            Self::F32(b) => AnyBlockMut::F32(b.slice_channels_mut(range)),
            Self::F64(b) => AnyBlockMut::F64(b.slice_channels_mut(range)),
        }
    }

    /// Sets the first `frames()` samples of every channel to zero.
    ///
    /// ```
    /// # use syrp_format_api::AnyBlockMut;
    /// let mut d = [1.0_f64; 2];
    /// AnyBlockMut::from(syrp_audio_api::AudioBlockMut::new(&mut d, 1, 2, 2)?).clear();
    /// assert_eq!(d, [0.0, 0.0]);
    /// # Ok::<(), syrp_audio_api::AudioError>(())
    /// ```
    pub fn clear(&mut self) {
        match self {
            Self::F32(b) => b.fill(0.0),
            Self::F64(b) => b.fill(0.0),
        }
    }
}

impl<'a> From<AudioBlockMut<'a, f32>> for AnyBlockMut<'a> {
    fn from(b: AudioBlockMut<'a, f32>) -> Self {
        Self::F32(b)
    }
}

impl<'a> From<AudioBlockMut<'a, f64>> for AnyBlockMut<'a> {
    fn from(b: AudioBlockMut<'a, f64>) -> Self {
        Self::F64(b)
    }
}

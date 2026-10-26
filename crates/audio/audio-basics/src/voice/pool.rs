// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`VoicePool`]: voices stored as lane packs.

use core::fmt;
use core::marker::PhantomData;

use syrp_error::{Diagnostic, ErrorCode};

use super::{Voice, VoiceHandle, VoiceSample};

/// The most output channels a [`VoicePool`] renders. The per-call channel table lives on the
/// stack so that rendering never allocates.
///
/// ```
/// assert_eq!(syrp_audio_basics::voice::MAX_OUT_CHANNELS, 64);
/// ```
pub const MAX_OUT_CHANNELS: usize = 64;

/// The most voices a [`VoicePool`] holds. The cap keeps a pool of zero-sized voices, whose
/// allocation always succeeds, from running the factory an unbounded number of times.
///
/// ```
/// assert_eq!(syrp_audio_basics::voice::MAX_VOICES, 65_536);
/// ```
pub const MAX_VOICES: usize = 65_536;

/// The error of [`VoicePool::new`] and [`VoicePool::prepare`] (`SYRP-AUDIO-0006`).
///
/// ```
/// use syrp_audio_basics::voice::{NoteStart, Voice, VoicePool, VoicePoolError};
/// use syrp_error::Diagnostic;
/// # struct V; impl Voice<f32> for V { fn prepare(&mut self, _: f64, _: usize, _: usize) {} fn start(&mut self, _: usize, _: &NoteStart<'_>) {} fn stop(&mut self, _: usize, _: f32, _: bool) {} fn render(&mut self, _: &mut [&mut [f32]], _: usize) {} fn active(&self) -> bool { false } }
/// let err = VoicePool::<f32, _>::new(usize::MAX, |_| V).unwrap_err();
/// assert!(matches!(err, VoicePoolError::TooManyVoices { .. }));
/// assert_eq!(err.code().as_str(), "SYRP-AUDIO-0006");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum VoicePoolError {
    /// `voices` exceeds [`MAX_VOICES`], or its packs cannot be allocated.
    #[non_exhaustive]
    TooManyVoices {
        /// Requested voices.
        voices: usize,
    },
    /// `out_channels` exceeds [`MAX_OUT_CHANNELS`], or the render scratch
    /// (`out_channels * max_frames` packs) cannot be allocated.
    #[non_exhaustive]
    CannotPrepare {
        /// Requested frames per render.
        max_frames: usize,
        /// Requested output channels.
        out_channels: usize,
    },
}

impl fmt::Display for VoicePoolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::TooManyVoices { voices } => {
                write!(
                    f,
                    "a voice pool of {voices} voices (at most {MAX_VOICES}) cannot be allocated"
                )
            }
            Self::CannotPrepare {
                max_frames,
                out_channels,
            } => write!(
                f,
                "a voice pool cannot render {out_channels} channels (at most \
                 {MAX_OUT_CHANNELS}) of {max_frames} frames"
            ),
        }
    }
}

impl std::error::Error for VoicePoolError {}

impl Diagnostic for VoicePoolError {
    fn code(&self) -> ErrorCode {
        ErrorCode::new("SYRP-AUDIO-0006")
    }
}

/// Polyphonic voices stored as lane packs of `S` (ADR 0003 D5); JUCE's `Synthesiser` role.
///
/// `VoicePool<F32x8, MyVoice<F32x8>>` keeps `ceil(voices / 8)` packs and renders each with one
/// call; `VoicePool<f32, MyVoice<f32>>` keeps one voice per pack (the compat layer's scalar
/// path).
///
/// ```
/// use syrp_audio_basics::voice::{NoteStart, Voice, VoicePool};
/// use syrp_simd::F32x8;
/// # struct V; impl Voice<F32x8> for V { fn prepare(&mut self, _: f64, _: usize, _: usize) {} fn start(&mut self, _: usize, _: &NoteStart<'_>) {} fn stop(&mut self, _: usize, _: f32, _: bool) {} fn render(&mut self, _: &mut [&mut [F32x8]], _: usize) {} fn active(&self) -> syrp_simd::Mask32x8 { syrp_simd::Mask::splat(false) } }
/// let pool = VoicePool::<F32x8, _>::new(20, |_| V)?;
/// assert_eq!((pool.voices(), pool.packs()), (20, 3));
/// # Ok::<(), syrp_audio_basics::voice::VoicePoolError>(())
/// ```
#[doc(alias = "Synthesiser")]
pub struct VoicePool<S: VoiceSample, V: Voice<S>> {
    voices: usize,
    packs: Vec<V>,
    /// `out_channels` regions of `max_frames` packs each.
    scratch: Vec<S>,
    max_frames: usize,
    out_channels: usize,
    _sample: PhantomData<fn() -> S>,
}

impl<S: VoiceSample, V: Voice<S>> fmt::Debug for VoicePool<S, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VoicePool")
            .field("voices", &self.voices)
            .field("packs", &self.packs.len())
            .field("max_frames", &self.max_frames)
            .field("out_channels", &self.out_channels)
            .field("scratch", &self.scratch.len())
            .finish_non_exhaustive()
    }
}

/// Fallible `Vec` with exactly `n` elements made by `f`.
fn try_vec<T>(n: usize, f: impl FnMut(usize) -> T) -> Option<Vec<T>> {
    let mut v = Vec::new();
    v.try_reserve_exact(n).ok()?;
    v.extend((0..n).map(f));
    Some(v)
}

impl<S: VoiceSample, V: Voice<S>> VoicePool<S, V> {
    /// A pool of `voices` voices in `ceil(voices / S::LANES)` packs, each made by
    /// `factory(pack_index)`. Lanes past `voices` in the last pack are never assigned. Allocates;
    /// fails with [`VoicePoolError::TooManyVoices`] above [`MAX_VOICES`] or when allocation fails,
    /// instead of aborting.
    ///
    /// ```
    /// # use syrp_audio_basics::voice::{NoteStart, Voice, VoicePool};
    /// # struct V; impl Voice<f32> for V { fn prepare(&mut self, _: f64, _: usize, _: usize) {} fn start(&mut self, _: usize, _: &NoteStart<'_>) {} fn stop(&mut self, _: usize, _: f32, _: bool) {} fn render(&mut self, _: &mut [&mut [f32]], _: usize) {} fn active(&self) -> bool { false } }
    /// let pool = VoicePool::<f32, _>::new(16, |_| V)?;
    /// assert_eq!(pool.packs(), 16);
    /// # Ok::<(), syrp_audio_basics::voice::VoicePoolError>(())
    /// ```
    pub fn new(voices: usize, factory: impl FnMut(usize) -> V) -> Result<Self, VoicePoolError> {
        let err = VoicePoolError::TooManyVoices { voices };
        if voices > MAX_VOICES {
            return Err(err);
        }
        let n_packs = voices.div_ceil(S::LANES);
        let packs = try_vec(n_packs, factory).ok_or(err)?;
        Ok(Self {
            voices,
            packs,
            scratch: Vec::new(),
            max_frames: 0,
            out_channels: 0,
            _sample: PhantomData,
        })
    }

    /// Prepares every pack and allocates the render scratch for blocks of up to `max_frames`
    /// frames into `out_channels` channels. Allocates;
    /// call it off the audio thread.
    ///
    /// ```
    /// # use syrp_audio_basics::voice::{NoteStart, Voice, VoicePool, VoicePoolError};
    /// # struct V; impl Voice<f32> for V { fn prepare(&mut self, _: f64, _: usize, _: usize) {} fn start(&mut self, _: usize, _: &NoteStart<'_>) {} fn stop(&mut self, _: usize, _: f32, _: bool) {} fn render(&mut self, _: &mut [&mut [f32]], _: usize) {} fn active(&self) -> bool { false } }
    /// let mut pool = VoicePool::<f32, _>::new(4, |_| V)?;
    /// pool.prepare(44_100.0, 256, 2)?;
    /// assert!(matches!(pool.prepare(44_100.0, 256, 65), Err(VoicePoolError::CannotPrepare { .. })));
    /// # Ok::<(), VoicePoolError>(())
    /// ```
    pub fn prepare(
        &mut self,
        sample_rate: f64,
        max_frames: usize,
        out_channels: usize,
    ) -> Result<(), VoicePoolError> {
        let err = VoicePoolError::CannotPrepare {
            max_frames,
            out_channels,
        };
        let len = max_frames.checked_mul(out_channels).ok_or(err)?;
        if out_channels > MAX_OUT_CHANNELS {
            return Err(err);
        }
        self.scratch = try_vec(len, |_| S::zero()).ok_or(err)?;
        self.max_frames = max_frames;
        self.out_channels = out_channels;
        for pack in &mut self.packs {
            pack.prepare(sample_rate, max_frames, out_channels);
        }
        Ok(())
    }

    /// The number of voices the pool was built with.
    ///
    /// ```
    /// # use syrp_audio_basics::voice::{NoteStart, Voice, VoicePool};
    /// # struct V; impl Voice<f32> for V { fn prepare(&mut self, _: f64, _: usize, _: usize) {} fn start(&mut self, _: usize, _: &NoteStart<'_>) {} fn stop(&mut self, _: usize, _: f32, _: bool) {} fn render(&mut self, _: &mut [&mut [f32]], _: usize) {} fn active(&self) -> bool { false } }
    /// assert_eq!(VoicePool::<f32, _>::new(6, |_| V)?.voices(), 6);
    /// # Ok::<(), syrp_audio_basics::voice::VoicePoolError>(())
    /// ```
    pub fn voices(&self) -> usize {
        self.voices
    }

    /// The number of packs, `ceil(voices / S::LANES)`.
    ///
    /// ```
    /// # use syrp_audio_basics::voice::{NoteStart, Voice, VoicePool};
    /// # use syrp_simd::{F32x4, Mask, Mask32x4};
    /// # struct V; impl Voice<F32x4> for V { fn prepare(&mut self, _: f64, _: usize, _: usize) {} fn start(&mut self, _: usize, _: &NoteStart<'_>) {} fn stop(&mut self, _: usize, _: f32, _: bool) {} fn render(&mut self, _: &mut [&mut [F32x4]], _: usize) {} fn active(&self) -> Mask32x4 { Mask::splat(false) } }
    /// assert_eq!(VoicePool::<F32x4, _>::new(5, |_| V)?.packs(), 2);
    /// # Ok::<(), syrp_audio_basics::voice::VoicePoolError>(())
    /// ```
    pub fn packs(&self) -> usize {
        self.packs.len()
    }

    /// The stable handle of voice `index` (`0..voices()`): pack `index / S::LANES`, lane
    /// `index % S::LANES`. `None` when `index` is out of range.
    ///
    /// ```
    /// # use syrp_audio_basics::voice::{NoteStart, Voice, VoicePool, VoiceHandle};
    /// # use syrp_simd::{F32x4, Mask, Mask32x4};
    /// # struct V; impl Voice<F32x4> for V { fn prepare(&mut self, _: f64, _: usize, _: usize) {} fn start(&mut self, _: usize, _: &NoteStart<'_>) {} fn stop(&mut self, _: usize, _: f32, _: bool) {} fn render(&mut self, _: &mut [&mut [F32x4]], _: usize) {} fn active(&self) -> Mask32x4 { Mask::splat(false) } }
    /// let pool = VoicePool::<F32x4, _>::new(6, |_| V)?;
    /// assert_eq!(pool.handle(5), Some(VoiceHandle::new(1, 1)));
    /// assert_eq!(pool.handle(6), None);
    /// # Ok::<(), syrp_audio_basics::voice::VoicePoolError>(())
    /// ```
    pub fn handle(&self, index: usize) -> Option<VoiceHandle> {
        (index < self.voices).then(|| VoiceHandle::new(index / S::LANES, index % S::LANES))
    }

    /// The pack holding the voice at `handle`, to address it directly; `None` when the
    /// handle is out of range.
    ///
    /// ```
    /// # use syrp_audio_basics::voice::{NoteStart, Voice, VoicePool};
    /// # struct V(usize); impl Voice<f32> for V { fn prepare(&mut self, _: f64, _: usize, _: usize) {} fn start(&mut self, _: usize, _: &NoteStart<'_>) {} fn stop(&mut self, _: usize, _: f32, _: bool) {} fn render(&mut self, _: &mut [&mut [f32]], _: usize) {} fn active(&self) -> bool { false } }
    /// let mut pool = VoicePool::<f32, _>::new(3, V)?;
    /// let h = pool.handle(0).unwrap();
    /// assert_eq!(pool.voice_mut(h).map(|v| v.0), Some(0));
    /// # Ok::<(), syrp_audio_basics::voice::VoicePoolError>(())
    /// ```
    pub fn voice_mut(&mut self, handle: VoiceHandle) -> Option<&mut V> {
        if !self.contains(handle) {
            return None;
        }
        self.packs.get_mut(handle.pack())
    }

    /// Whether `handle` addresses one of the pool's voices.
    fn contains(&self, handle: VoiceHandle) -> bool {
        let index = handle
            .pack()
            .checked_mul(S::LANES)
            .and_then(|i| i.checked_add(handle.lane()));
        handle.lane() < S::LANES && index.is_some_and(|i| i < self.voices)
    }
}

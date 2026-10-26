// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`VoicePool`]: lane packs, note-to-lane assignment and sample-accurate rendering.

use core::fmt;
use core::marker::PhantomData;

use syrp_audio_api::AudioBlockMut;
use syrp_error::{Diagnostic, ErrorCode};
use syrp_simd::Mask;

use super::{
    NoteStart, PITCH_WHEEL_CENTRE, Voice, VoiceEvent, VoiceEventKind, VoiceHandle, VoiceSample,
};

/// The most output channels a [`VoicePool`] renders. The per-call channel table lives on the
/// stack so that [`VoicePool::render`] never allocates.
///
/// ```
/// assert_eq!(syrp_audio_basics::voice::MAX_OUT_CHANNELS, 64);
/// ```
pub const MAX_OUT_CHANNELS: usize = 64;

/// The most frames per render a [`VoicePool`] can be prepared for (2^20, about 22 seconds at
/// 48 kHz). Longer blocks are still rendered, in pieces; the cap bounds the scratch and what
/// each voice is asked to prepare for.
///
/// ```
/// assert_eq!(syrp_audio_basics::voice::MAX_FRAMES, 1 << 20);
/// ```
pub const MAX_FRAMES: usize = 1 << 20;

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
    /// `out_channels` exceeds [`MAX_OUT_CHANNELS`], `max_frames` exceeds [`MAX_FRAMES`], or the
    /// render scratch
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
                 {MAX_OUT_CHANNELS}) of {max_frames} frames (at most {MAX_FRAMES})"
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

/// What one lane is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LaneState {
    /// Silent and assignable.
    Free,
    /// Playing a note that has not been stopped (its key, or a pedal, holds it).
    Held,
    /// Stopped with tail-off; free once the voice reports the lane inactive.
    Released,
}

/// One row of the lane table (one per lane of every pack, including unused trailing lanes).
#[derive(Debug, Clone, Copy)]
struct Lane {
    state: LaneState,
    /// The note's key is still down; a held lane with the key up is kept by a pedal.
    key_down: bool,
    /// Held by sostenuto: the key was down when the pedal went down.
    sostenuto: bool,
    channel: u8,
    note: u8,
}

const FREE_LANE: Lane = Lane {
    state: LaneState::Free,
    key_down: false,
    sostenuto: false,
    channel: 0,
    note: 0,
};

/// The pitch wheel, controllers and pedals of one MIDI channel.
#[derive(Debug, Clone, Copy)]
struct ChannelState {
    pitch_wheel: u16,
    controllers: [u8; 128],
    sustain: bool,
    sostenuto: bool,
}

const CHANNEL_RESET: ChannelState = ChannelState {
    pitch_wheel: PITCH_WHEEL_CENTRE,
    controllers: [0; 128],
    sustain: false,
    sostenuto: false,
};

/// Polyphonic voices stored as lane packs of `S` (ADR 0003 D5); JUCE's `Synthesiser` role.
///
/// `VoicePool<F32x8, MyVoice<F32x8>>` keeps `ceil(voices / 8)` packs and renders each with one
/// call; `VoicePool<f32, MyVoice<f32>>` keeps one voice per pack (the compat layer's scalar
/// path). See the [module docs](super) for lane assignment.
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
    /// `packs.len() * S::LANES` rows; row `p * S::LANES + l` is lane `l` of pack `p`.
    lanes: Vec<Lane>,
    /// `out_channels` regions of `max_frames` packs each.
    scratch: Vec<S>,
    max_frames: usize,
    out_channels: usize,
    channels: [ChannelState; 16],
    min_subdivision: usize,
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
            .field("active_voices", &self.active_voices())
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
        let lanes = try_vec(n_packs * S::LANES, |_| FREE_LANE).ok_or(err)?;
        let packs = try_vec(n_packs, factory).ok_or(err)?;
        Ok(Self {
            voices,
            packs,
            lanes,
            scratch: Vec::new(),
            max_frames: 0,
            out_channels: 0,
            channels: [CHANNEL_RESET; 16],
            min_subdivision: 1,
            _sample: PhantomData,
        })
    }

    /// Prepares every pack and allocates the render scratch for blocks of up to `max_frames`
    /// frames into `out_channels` channels. Longer blocks are rendered in pieces. Allocates;
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
        if out_channels > MAX_OUT_CHANNELS || max_frames > MAX_FRAMES {
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

    /// Splits that would be shorter than `frames` are merged forward: events falling less
    /// than `frames` samples after the current split point are applied at that point (early),
    /// as JUCE's `Synthesiser::setMinimumRenderingSubdivisionSize` documents. The default, 1,
    /// makes every event sample-accurate; 0 is treated as 1.
    ///
    /// ```
    /// # use syrp_audio_basics::voice::{NoteStart, Voice, VoicePool};
    /// # struct V; impl Voice<f32> for V { fn prepare(&mut self, _: f64, _: usize, _: usize) {} fn start(&mut self, _: usize, _: &NoteStart<'_>) {} fn stop(&mut self, _: usize, _: f32, _: bool) {} fn render(&mut self, _: &mut [&mut [f32]], _: usize) {} fn active(&self) -> bool { false } }
    /// let mut pool = VoicePool::<f32, _>::new(4, |_| V)?;
    /// pool.set_minimum_subdivision(32);
    /// # Ok::<(), syrp_audio_basics::voice::VoicePoolError>(())
    /// ```
    #[doc(alias = "setMinimumRenderingSubdivisionSize")]
    pub fn set_minimum_subdivision(&mut self, frames: usize) {
        self.min_subdivision = frames.max(1);
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

    /// The pack (a `V` of `S::LANES` voices) holding the voice at `handle`, to address it
    /// directly through `handle.lane()`; `None` when the
    /// handle is out of range.
    ///
    /// ```
    /// # use syrp_audio_basics::voice::{NoteStart, Voice, VoicePool};
    /// # struct V(usize); impl Voice<f32> for V { fn prepare(&mut self, _: f64, _: usize, _: usize) {} fn start(&mut self, _: usize, _: &NoteStart<'_>) {} fn stop(&mut self, _: usize, _: f32, _: bool) {} fn render(&mut self, _: &mut [&mut [f32]], _: usize) {} fn active(&self) -> bool { false } }
    /// let mut pool = VoicePool::<f32, _>::new(3, V)?;
    /// let h = pool.handle(0).unwrap();
    /// assert_eq!(pool.pack_mut(h).map(|v| v.0), Some(0));
    /// # Ok::<(), syrp_audio_basics::voice::VoicePoolError>(())
    /// ```
    pub fn pack_mut(&mut self, handle: VoiceHandle) -> Option<&mut V> {
        self.lane_index(handle)?;
        self.packs.get_mut(handle.pack())
    }

    /// The `(channel, note)` the voice at `handle` is playing or releasing; `None` when it is
    /// free or the handle is out of range.
    ///
    /// ```
    /// # use syrp_audio_basics::voice::{NoteStart, Voice, VoicePool};
    /// # struct V; impl Voice<f32> for V { fn prepare(&mut self, _: f64, _: usize, _: usize) {} fn start(&mut self, _: usize, _: &NoteStart<'_>) {} fn stop(&mut self, _: usize, _: f32, _: bool) {} fn render(&mut self, _: &mut [&mut [f32]], _: usize) {} fn active(&self) -> bool { false } }
    /// let pool = VoicePool::<f32, _>::new(2, |_| V)?;
    /// assert_eq!(pool.note(pool.handle(0).unwrap()), None);
    /// # Ok::<(), syrp_audio_basics::voice::VoicePoolError>(())
    /// ```
    pub fn note(&self, handle: VoiceHandle) -> Option<(u8, u8)> {
        let lane = self.lanes[self.lane_index(handle)?];
        (lane.state != LaneState::Free).then_some((lane.channel, lane.note))
    }

    /// The number of lanes playing or releasing a note.
    ///
    /// ```
    /// # use syrp_audio_basics::voice::{NoteStart, Voice, VoicePool};
    /// # struct V; impl Voice<f32> for V { fn prepare(&mut self, _: f64, _: usize, _: usize) {} fn start(&mut self, _: usize, _: &NoteStart<'_>) {} fn stop(&mut self, _: usize, _: f32, _: bool) {} fn render(&mut self, _: &mut [&mut [f32]], _: usize) {} fn active(&self) -> bool { false } }
    /// assert_eq!(VoicePool::<f32, _>::new(2, |_| V)?.active_voices(), 0);
    /// # Ok::<(), syrp_audio_basics::voice::VoicePoolError>(())
    /// ```
    pub fn active_voices(&self) -> usize {
        self.lanes
            .iter()
            .filter(|l| l.state != LaneState::Free)
            .count()
    }

    /// Applies `events` (sorted by offset) and adds the voices' output to `out`.
    ///
    /// The block is split at each event offset, so events are sample-accurate, except that
    /// splits shorter than the [minimum subdivision](Self::set_minimum_subdivision) are merged
    /// forward. Every pack's lanes are summed into each output channel; channels beyond the
    /// prepared count are left alone. Events at or past `out.frames()` are applied after the
    /// block. Before [`prepare`](Self::prepare) only the events are applied. Never allocates.
    ///
    /// ```
    /// # use syrp_audio_basics::voice::{NoteStart, Voice, VoicePool, VoiceEvent};
    /// # use syrp_audio_basics::AudioBuffer;
    /// # struct V(f32); impl Voice<f32> for V { fn prepare(&mut self, _: f64, _: usize, _: usize) {} fn start(&mut self, _: usize, n: &NoteStart<'_>) { self.0 = n.velocity() } fn stop(&mut self, _: usize, _: f32, _: bool) { self.0 = 0.0 } fn render(&mut self, out: &mut [&mut [f32]], n: usize) { out.iter_mut().for_each(|c| c[..n].fill(self.0)) } fn active(&self) -> bool { self.0 > 0.0 } }
    /// let mut pool = VoicePool::<f32, _>::new(2, |_| V(0.0))?;
    /// pool.prepare(48_000.0, 16, 1)?;
    /// let mut buf = AudioBuffer::<f32>::new(1, 16)?;
    /// buf.channel_mut(0).fill(1.0);
    /// pool.render(buf.as_block_mut(), &[VoiceEvent::note_on(8, 1, 60, 0.5)]);
    /// assert_eq!((buf.channel(0)[7], buf.channel(0)[8]), (1.0, 1.5));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn render(&mut self, mut out: AudioBlockMut<'_, f32>, events: &[VoiceEvent]) {
        debug_assert!(
            events.is_sorted_by_key(VoiceEvent::offset),
            "voice events must be sorted by offset"
        );
        let frames = out.frames();
        let (mut pos, mut next) = (0, 0);
        while pos < frames {
            // Clamped so an event at or past the block end is never pulled into the block.
            let horizon = pos.saturating_add(self.min_subdivision).min(frames);
            while let Some(e) = events.get(next).filter(|e| (e.offset() as usize) < horizon) {
                self.apply(e.kind());
                next += 1;
            }
            let end = events
                .get(next)
                .map_or(frames, |e| (e.offset() as usize).min(frames));
            self.render_span(&mut out, pos, end);
            pos = end;
        }
        for e in &events[next..] {
            self.apply(e.kind());
        }
    }

    /// Renders frames `start..end` of `out`, in pieces of at most `max_frames`.
    fn render_span(&mut self, out: &mut AudioBlockMut<'_, f32>, start: usize, end: usize) {
        if self.max_frames == 0 {
            return;
        }
        let mixed = self.out_channels.min(out.channels());
        let mut at = start;
        while at < end {
            let n = (end - at).min(self.max_frames);
            for (p, pack) in self.packs.iter_mut().enumerate() {
                let lanes = &mut self.lanes[p * S::LANES..(p + 1) * S::LANES];
                if pack.active().none() {
                    lanes.iter_mut().for_each(|l| *l = FREE_LANE);
                    continue;
                }
                let mut regions = self.scratch.chunks_exact_mut(self.max_frames);
                let mut table: [&mut [S]; MAX_OUT_CHANNELS] =
                    core::array::from_fn(|_| match regions.next() {
                        Some(region) => &mut region[..n],
                        None => &mut [],
                    });
                let table = &mut table[..self.out_channels];
                pack.render(table, n);
                for (ch, src) in table.iter().enumerate().take(mixed) {
                    let dst = &mut out.channel_mut(ch)[at..at + n];
                    for (d, s) in dst.iter_mut().zip(src.iter()) {
                        *d += s.lane_sum();
                    }
                }
                let active = pack.active();
                for (l, lane) in lanes.iter_mut().enumerate() {
                    if !S::mask_lane(active, l) {
                        *lane = FREE_LANE;
                    }
                }
            }
            at += n;
        }
    }

    /// The lane-table row of `handle`, if it addresses one of the pool's voices.
    fn lane_index(&self, handle: VoiceHandle) -> Option<usize> {
        let index = handle
            .pack()
            .checked_mul(S::LANES)?
            .checked_add(handle.lane())?;
        (handle.lane() < S::LANES && index < self.voices).then_some(index)
    }

    /// Calls `f(pack, lane, row)` for every assignable lane whose row matches `pred`.
    fn for_lanes(
        &mut self,
        pred: impl Fn(&Lane) -> bool,
        mut f: impl FnMut(&mut V, usize, &mut Lane),
    ) {
        for (i, lane) in self.lanes[..self.voices].iter_mut().enumerate() {
            if pred(lane) {
                f(&mut self.packs[i / S::LANES], i % S::LANES, lane);
            }
        }
    }

    /// Stops (with tail-off) every held lane on `channel` that matches `pred`.
    fn release(&mut self, channel: u8, velocity: f32, pred: impl Fn(&Lane) -> bool) {
        let hit = |l: &Lane| l.state == LaneState::Held && l.channel == channel && pred(l);
        self.for_lanes(hit, |v, l, row| {
            v.stop(l, velocity, true);
            row.state = LaneState::Released;
        });
    }

    /// Applies one event; events naming a channel outside 1–16 are ignored.
    fn apply(&mut self, kind: VoiceEventKind) {
        let valid = |c: u8| (1..=16).contains(&c);
        match kind {
            VoiceEventKind::NoteOn {
                channel,
                note,
                velocity,
            } if valid(channel) => self.note_on(channel, note, velocity),
            VoiceEventKind::NoteOff {
                channel,
                note,
                velocity,
            } if valid(channel) => {
                let pedal = self.channels[usize::from(channel - 1)].sustain;
                let key =
                    |l: &Lane| l.state == LaneState::Held && (l.channel, l.note) == (channel, note);
                self.for_lanes(key, |_, _, row| row.key_down = false);
                self.release(channel, velocity, |l| {
                    l.note == note && !pedal && !l.sostenuto
                });
            }
            VoiceEventKind::Sustain { channel, down } if valid(channel) => {
                self.channels[usize::from(channel - 1)].sustain = down;
                if !down {
                    self.release(channel, 0.0, |l| !l.key_down && !l.sostenuto);
                }
            }
            VoiceEventKind::Sostenuto { channel, down } if valid(channel) => {
                // Only a change of pedal state acts. A continuous pedal sends many "down"
                // values; re-pressing must neither capture keys pressed since the first press
                // nor drop the notes that press captured.
                let pedal = &mut self.channels[usize::from(channel - 1)].sostenuto;
                if core::mem::replace(pedal, down) == down {
                    return;
                }
                let on = |l: &Lane| l.state == LaneState::Held && l.channel == channel;
                if down {
                    self.for_lanes(on, |_, _, row| row.sostenuto = row.key_down);
                } else {
                    self.for_lanes(on, |_, _, row| row.sostenuto = false);
                    if !self.channels[usize::from(channel - 1)].sustain {
                        self.release(channel, 0.0, |l| !l.key_down);
                    }
                }
            }
            VoiceEventKind::PitchWheel { channel, value } if valid(channel) => {
                self.channels[usize::from(channel - 1)].pitch_wheel = value;
                let on = |l: &Lane| l.state != LaneState::Free && l.channel == channel;
                self.for_lanes(on, |v, l, _| v.pitch_wheel(l, value));
            }
            VoiceEventKind::Controller {
                channel,
                number,
                value,
            } if valid(channel) && number < 128 => {
                self.channels[usize::from(channel - 1)].controllers[usize::from(number)] = value;
                let on = |l: &Lane| l.state != LaneState::Free && l.channel == channel;
                self.for_lanes(on, |v, l, _| v.controller(l, number, value));
            }
            VoiceEventKind::AllNotesOff { channel } => {
                for c in 1..=16 {
                    if channel.is_none_or(|ch| ch == c) {
                        self.release(c, 0.0, |_| true);
                    }
                }
            }
            VoiceEventKind::AllSoundOff => {
                self.for_lanes(
                    |l| l.state != LaneState::Free,
                    |v, l, row| {
                        v.stop(l, 0.0, false);
                        *row = FREE_LANE;
                    },
                );
            }
            _ => {}
        }
    }

    /// Starts a note (spec B4): a lane already holding the same channel and note is stopped
    /// with tail-off, then the note takes the lowest free lane, else that same lane, else it is
    /// dropped.
    fn note_on(&mut self, channel: u8, note: u8, velocity: f32) {
        let mut retriggered = None;
        for i in 0..self.voices {
            let row = &mut self.lanes[i];
            if row.state == LaneState::Held && (row.channel, row.note) == (channel, note) {
                self.packs[i / S::LANES].stop(i % S::LANES, 0.0, true);
                row.state = LaneState::Released;
                retriggered.get_or_insert(i);
            }
        }
        let free = self.lanes[..self.voices]
            .iter()
            .position(|l| l.state == LaneState::Free);
        let Some(i) = free.or(retriggered) else {
            return;
        };
        let state = &self.channels[usize::from(channel - 1)];
        let start = NoteStart::new(channel, note, velocity)
            .with_channel_state(state.pitch_wheel, &state.controllers);
        self.packs[i / S::LANES].start(i % S::LANES, &start);
        self.lanes[i] = Lane {
            state: LaneState::Held,
            key_down: true,
            sostenuto: false,
            channel,
            note,
        };
    }
}

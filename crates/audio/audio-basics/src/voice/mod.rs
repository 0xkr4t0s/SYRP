// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The [`VoicePool`]: polyphonic voices stored as SIMD lane packs (ADR 0003 D2 and D5).
//!
//! A [`Voice<S>`](Voice) is written once against a [`Sample`] type. `MyVoice<F32x8>` is eight
//! voices in struct-of-arrays form, one per lane, and `MyVoice<f32>` is one voice. The pool
//! keeps `ceil(voices / S::LANES)` such packs, assigns notes to lanes, and renders a whole pack
//! per call. Its input is plain [`VoiceEvent`]s, not MIDI: the compat layer and the processor
//! runtime translate MIDI into events. `VoicePool<f32, V>` (one lane per pack) is the scalar
//! path that `syrp-compat-audio` builds JUCE's `Synthesiser` on.
//!
//! # Lane assignment
//! These rules follow JUCE's `Synthesiser` documentation where it decides them; the rest are
//! Syrp's own and are listed for the differential harness.
//!
//! - A note-on takes the lowest free lane, lowest pack first. A note-on for a channel and note
//!   already held first stops that lane with tail-off; if no other lane is free the note
//!   starts in that same lane. With every lane busy otherwise, the note is dropped
//!   (stealing, spec B5, follows in the next increment).
//! - A note-off stops, with tail-off, every lane holding its channel and note, unless the
//!   channel's sustain pedal is down (the lane is stopped when the pedal lifts) or the lane is
//!   held by sostenuto. Sostenuto holds only the notes whose keys were down when it was
//!   pressed, and releases them when it lifts unless sustain still holds them.
//! - A lane is free again once its voice reports it inactive after a render, and a pack with
//!   no active lane is not rendered.
//! - Syrp's own rules: only held lanes are retriggered (a lane already releasing the same note
//!   keeps its tail and the new note takes another lane); a repeated sostenuto "down", as a
//!   continuous pedal sends, changes nothing; events at or past the block end are applied
//!   after it, whatever the minimum subdivision; `AllNotesOff` stops held lanes with
//!   tail-off regardless of the pedals; `AllSoundOff` stops every lane without tail-off and
//!   frees it at once.
//!
//! # Real-time safety
//! [`VoicePool::new`] and [`VoicePool::prepare`] allocate; call them off the audio thread.
//! [`VoicePool::render`] and the accessors never allocate, lock or do I/O.
//!
//! ```
//! use syrp_audio_basics::voice::{NoteStart, Voice, VoicePool, VoiceSample};
//! use syrp_simd::{F32x4, Mask, Sample};
//!
//! /// A DC level per lane, on while the note is held.
//! struct Dc<S> { level: S }
//! impl<S: VoiceSample> Voice<S> for Dc<S> {
//!     fn prepare(&mut self, _: f64, _: usize, _: usize) {}
//!     fn start(&mut self, lane: usize, note: &NoteStart<'_>) {
//!         self.level.set_lane(lane, note.velocity());
//!     }
//!     fn stop(&mut self, lane: usize, _: f32, _: bool) { self.level.set_lane(lane, 0.0) }
//!     fn render(&mut self, out: &mut [&mut [S]], frames: usize) {
//!         out.iter_mut().for_each(|ch| ch[..frames].fill(self.level));
//!     }
//!     fn active(&self) -> S::Mask { self.level.simd_gt(S::zero()) }
//! }
//!
//! let mut pool = VoicePool::<F32x4, _>::new(8, |_| Dc { level: F32x4::zero() })?;
//! pool.prepare(48_000.0, 64, 2)?;
//! let h = pool.handle(5).expect("voice 5 exists");
//! assert_eq!((h.pack(), h.lane()), (1, 1));
//! let pack = pool.pack_mut(h).expect("in range");
//! pack.start(h.lane(), &NoteStart::new(1, 60, 0.5));
//! assert_eq!(pack.active().to_array(), [false, true, false, false]);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod event;
mod pool;

pub use event::{VoiceEvent, VoiceEventKind};

pub use pool::{MAX_FRAMES, MAX_OUT_CHANNELS, MAX_VOICES, VoicePool, VoicePoolError};

use syrp_simd::{F32x4, F32x8, Sample};

/// The pitch-wheel value of a centred wheel (14-bit range `0..=16383`).
///
/// ```
/// assert_eq!(syrp_audio_basics::voice::PITCH_WHEEL_CENTRE, 8192);
/// ```
pub const PITCH_WHEEL_CENTRE: u16 = 8192;

/// A per-instance voice written against a sample type `S`: `f32` for one voice, a lane pack
/// such as `F32x8` for eight.
///
/// Every method that addresses one voice takes its `lane` (`0..S::LANES`). The pool calls
/// [`render`](Voice::render) only for packs with at least one active lane.
///
/// ```
/// use syrp_audio_basics::voice::{NoteStart, Voice};
/// /// One scalar voice that holds its velocity as a DC level.
/// struct Dc(f32);
/// impl Voice<f32> for Dc {
///     fn prepare(&mut self, _: f64, _: usize, _: usize) {}
///     fn start(&mut self, _: usize, note: &NoteStart<'_>) { self.0 = note.velocity() }
///     fn stop(&mut self, _: usize, _: f32, _: bool) { self.0 = 0.0 }
///     fn render(&mut self, out: &mut [&mut [f32]], frames: usize) {
///         out.iter_mut().for_each(|ch| ch[..frames].fill(self.0));
///     }
///     fn active(&self) -> bool { self.0 > 0.0 }
/// }
/// let mut v = Dc(0.0);
/// v.start(0, &NoteStart::new(1, 60, 0.5));
/// assert!(v.active());
/// ```
#[doc(alias = "SynthesiserVoice")]
pub trait Voice<S: Sample>: Send {
    /// Prepares for playback. `render` will be called with at most `max_frames` frames and
    /// exactly `out_channels` output channels. Called off the audio thread; may allocate.
    ///
    /// ```
    /// # use syrp_audio_basics::voice::{NoteStart, Voice};
    /// # struct V(usize); impl Voice<f32> for V { fn prepare(&mut self, _: f64, m: usize, _: usize) { self.0 = m } fn start(&mut self, _: usize, _: &NoteStart<'_>) {} fn stop(&mut self, _: usize, _: f32, _: bool) {} fn render(&mut self, _: &mut [&mut [f32]], _: usize) {} fn active(&self) -> bool { false } }
    /// let mut v = V(0);
    /// v.prepare(48_000.0, 512, 2);
    /// assert_eq!(v.0, 512);
    /// ```
    fn prepare(&mut self, sample_rate: f64, max_frames: usize, out_channels: usize);

    /// Starts `note` in `lane`, replacing whatever the lane was playing.
    ///
    /// ```
    /// # use syrp_audio_basics::voice::{NoteStart, Voice};
    /// # struct V(u8); impl Voice<f32> for V { fn prepare(&mut self, _: f64, _: usize, _: usize) {} fn start(&mut self, _: usize, n: &NoteStart<'_>) { self.0 = n.note() } fn stop(&mut self, _: usize, _: f32, _: bool) {} fn render(&mut self, _: &mut [&mut [f32]], _: usize) {} fn active(&self) -> bool { false } }
    /// let mut v = V(0);
    /// v.start(0, &NoteStart::new(1, 69, 1.0));
    /// assert_eq!(v.0, 69);
    /// ```
    fn start(&mut self, lane: usize, note: &NoteStart<'_>);

    /// Releases the note in `lane` with release `velocity` (0–1). With `allow_tail_off` the
    /// voice may keep sounding (a release tail) and reports the lane inactive when it is done;
    /// without it the lane must be silent and inactive from the next render on.
    ///
    /// ```
    /// # use syrp_audio_basics::voice::{NoteStart, Voice};
    /// # struct V(bool); impl Voice<f32> for V { fn prepare(&mut self, _: f64, _: usize, _: usize) {} fn start(&mut self, _: usize, _: &NoteStart<'_>) { self.0 = true } fn stop(&mut self, _: usize, _: f32, _: bool) { self.0 = false } fn render(&mut self, _: &mut [&mut [f32]], _: usize) {} fn active(&self) -> bool { self.0 } }
    /// let mut v = V(true);
    /// v.stop(0, 0.0, false);
    /// assert!(!v.active());
    /// ```
    fn stop(&mut self, lane: usize, velocity: f32, allow_tail_off: bool);

    /// The pitch wheel of the channel `lane` is playing on moved to `value` (`0..=16383`,
    /// centre [`PITCH_WHEEL_CENTRE`]). The default ignores it.
    ///
    /// ```
    /// # use syrp_audio_basics::voice::{NoteStart, Voice};
    /// # struct V; impl Voice<f32> for V { fn prepare(&mut self, _: f64, _: usize, _: usize) {} fn start(&mut self, _: usize, _: &NoteStart<'_>) {} fn stop(&mut self, _: usize, _: f32, _: bool) {} fn render(&mut self, _: &mut [&mut [f32]], _: usize) {} fn active(&self) -> bool { false } }
    /// V.pitch_wheel(0, 0); // the default does nothing
    /// ```
    fn pitch_wheel(&mut self, lane: usize, value: u16) {
        let _ = (lane, value);
    }

    /// Controller `number` (`0..=127`) of the channel `lane` is playing on moved to `value`
    /// (`0..=127`). The default ignores it.
    ///
    /// ```
    /// # use syrp_audio_basics::voice::{NoteStart, Voice};
    /// # struct V; impl Voice<f32> for V { fn prepare(&mut self, _: f64, _: usize, _: usize) {} fn start(&mut self, _: usize, _: &NoteStart<'_>) {} fn stop(&mut self, _: usize, _: f32, _: bool) {} fn render(&mut self, _: &mut [&mut [f32]], _: usize) {} fn active(&self) -> bool { false } }
    /// V.controller(0, 1, 64); // the default does nothing
    /// ```
    fn controller(&mut self, lane: usize, number: u8, value: u8) {
        let _ = (lane, number, value);
    }

    /// Writes (overwrites) `frames` samples of every lane into each of `out`'s channels:
    /// `out[ch][..frames]`, one pack per sample. Inactive lanes must be written as zero.
    ///
    /// ```
    /// # use syrp_audio_basics::voice::{NoteStart, Voice};
    /// # struct V; impl Voice<f32> for V { fn prepare(&mut self, _: f64, _: usize, _: usize) {} fn start(&mut self, _: usize, _: &NoteStart<'_>) {} fn stop(&mut self, _: usize, _: f32, _: bool) {} fn render(&mut self, out: &mut [&mut [f32]], n: usize) { out.iter_mut().for_each(|c| c[..n].fill(0.0)) } fn active(&self) -> bool { false } }
    /// let mut ch = [1.0_f32; 4];
    /// V.render(&mut [&mut ch[..]], 4);
    /// assert_eq!(ch, [0.0; 4]);
    /// ```
    fn render(&mut self, out: &mut [&mut [S]], frames: usize);

    /// The lanes still sounding, including release tails.
    ///
    /// ```
    /// # use syrp_audio_basics::voice::{NoteStart, Voice};
    /// # struct V; impl Voice<f32> for V { fn prepare(&mut self, _: f64, _: usize, _: usize) {} fn start(&mut self, _: usize, _: &NoteStart<'_>) {} fn stop(&mut self, _: usize, _: f32, _: bool) {} fn render(&mut self, _: &mut [&mut [f32]], _: usize) {} fn active(&self) -> bool { false } }
    /// assert!(!V.active());
    /// ```
    fn active(&self) -> S::Mask;
}

/// Everything a voice needs to start a note: the channel (1–16), note number, velocity (0–1)
/// and the channel's pitch wheel and controllers at that moment.
///
/// ```
/// use syrp_audio_basics::voice::NoteStart;
/// let mut ccs = [0_u8; 128];
/// ccs[1] = 90;
/// let n = NoteStart::new(2, 60, 0.75).with_channel_state(1000, &ccs);
/// assert_eq!((n.channel(), n.note(), n.velocity()), (2, 60, 0.75));
/// assert_eq!((n.pitch_wheel(), n.controller(1)), (1000, 90));
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoteStart<'a> {
    channel: u8,
    note: u8,
    velocity: f32,
    pitch_wheel: u16,
    controllers: &'a [u8; 128],
}

/// All controllers at zero, for a [`NoteStart`] built without controller state.
static NO_CONTROLLERS: [u8; 128] = [0; 128];

impl<'a> NoteStart<'a> {
    /// A note with a centred pitch wheel and every controller at zero.
    ///
    /// ```
    /// use syrp_audio_basics::voice::{NoteStart, PITCH_WHEEL_CENTRE};
    /// let n = NoteStart::new(1, 60, 1.0);
    /// assert_eq!((n.pitch_wheel(), n.controller(7)), (PITCH_WHEEL_CENTRE, 0));
    /// ```
    pub fn new(channel: u8, note: u8, velocity: f32) -> NoteStart<'static> {
        NoteStart {
            channel,
            note,
            velocity,
            pitch_wheel: PITCH_WHEEL_CENTRE,
            controllers: &NO_CONTROLLERS,
        }
    }

    /// The same note with the channel's pitch wheel (`0..=16383`) and 128 controller values.
    ///
    /// ```
    /// # use syrp_audio_basics::voice::NoteStart;
    /// let ccs = [64_u8; 128];
    /// let n = NoteStart::new(1, 60, 1.0).with_channel_state(0, &ccs);
    /// assert_eq!((n.pitch_wheel(), n.controller(10)), (0, 64));
    /// ```
    pub fn with_channel_state<'b>(
        self,
        pitch_wheel: u16,
        controllers: &'b [u8; 128],
    ) -> NoteStart<'b> {
        NoteStart {
            channel: self.channel,
            note: self.note,
            velocity: self.velocity,
            pitch_wheel,
            controllers,
        }
    }

    /// The MIDI channel, 1–16.
    ///
    /// ```
    /// # use syrp_audio_basics::voice::NoteStart;
    /// assert_eq!(NoteStart::new(16, 60, 1.0).channel(), 16);
    /// ```
    pub fn channel(&self) -> u8 {
        self.channel
    }

    /// The note number, 0–127.
    ///
    /// ```
    /// # use syrp_audio_basics::voice::NoteStart;
    /// assert_eq!(NoteStart::new(1, 61, 1.0).note(), 61);
    /// ```
    pub fn note(&self) -> u8 {
        self.note
    }

    /// The velocity, 0–1.
    ///
    /// ```
    /// # use syrp_audio_basics::voice::NoteStart;
    /// assert_eq!(NoteStart::new(1, 60, 0.5).velocity(), 0.5);
    /// ```
    pub fn velocity(&self) -> f32 {
        self.velocity
    }

    /// The channel's pitch wheel, `0..=16383`.
    ///
    /// ```
    /// # use syrp_audio_basics::voice::NoteStart;
    /// assert_eq!(NoteStart::new(1, 60, 1.0).pitch_wheel(), 8192);
    /// ```
    pub fn pitch_wheel(&self) -> u16 {
        self.pitch_wheel
    }

    /// The channel's value of controller `number`; 0 for numbers above 127.
    ///
    /// ```
    /// # use syrp_audio_basics::voice::NoteStart;
    /// assert_eq!(NoteStart::new(1, 60, 1.0).controller(200), 0);
    /// ```
    pub fn controller(&self, number: u8) -> u8 {
        self.controllers
            .get(usize::from(number))
            .copied()
            .unwrap_or(0)
    }
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for f32 {}
    impl Sealed for syrp_simd::F32x4 {}
    impl Sealed for syrp_simd::F32x8 {}
}

/// The sample types a [`VoicePool`] runs: `f32` (one voice per pack), [`F32x4`] and
/// [`F32x8`]. Adds the per-lane access that the pool and lane-generic voices need on top of
/// [`Sample`]. Sealed.
///
/// ```
/// use syrp_audio_basics::voice::VoiceSample;
/// use syrp_simd::{F32x4, Sample};
/// let mut x = F32x4::zero();
/// x.set_lane(2, 1.5);
/// assert_eq!((x.to_array()[2], x.lane_sum()), (1.5, 1.5));
/// assert!(F32x4::mask_lane(x.simd_gt(F32x4::zero()), 2));
/// ```
pub trait VoiceSample: Sample<Element = f32> + sealed::Sealed {
    /// Sets lane `i` (`0..LANES`) to `value`.
    ///
    /// ```
    /// # use syrp_audio_basics::voice::VoiceSample;
    /// let mut x = 0.0_f32;
    /// x.set_lane(0, 3.0);
    /// assert_eq!(x, 3.0);
    /// ```
    fn set_lane(&mut self, i: usize, value: f32);

    /// Lane `i` of `mask`.
    ///
    /// ```
    /// # use syrp_audio_basics::voice::VoiceSample;
    /// assert!(<f32 as VoiceSample>::mask_lane(true, 0));
    /// ```
    fn mask_lane(mask: Self::Mask, i: usize) -> bool;

    /// The sum of every lane, first lane first.
    ///
    /// ```
    /// # use syrp_audio_basics::voice::VoiceSample;
    /// # use syrp_simd::{F32x8, Sample};
    /// assert_eq!(F32x8::splat(0.5).lane_sum(), 4.0);
    /// ```
    fn lane_sum(self) -> f32;
}

impl VoiceSample for f32 {
    #[inline(always)]
    fn set_lane(&mut self, i: usize, value: f32) {
        debug_assert_eq!(i, 0, "f32 has one lane");
        *self = value;
    }
    #[inline(always)]
    fn mask_lane(mask: bool, i: usize) -> bool {
        debug_assert_eq!(i, 0, "f32 has one lane");
        mask
    }
    #[inline(always)]
    fn lane_sum(self) -> f32 {
        self
    }
}

impl VoiceSample for F32x4 {
    #[inline(always)]
    fn set_lane(&mut self, i: usize, value: f32) {
        let mut lanes = self.to_array();
        lanes[i] = value;
        *self = F32x4::from_array(lanes);
    }
    #[inline(always)]
    fn mask_lane(mask: Self::Mask, i: usize) -> bool {
        mask.lane(i)
    }
    #[inline(always)]
    fn lane_sum(self) -> f32 {
        self.to_array().into_iter().sum()
    }
}

impl VoiceSample for F32x8 {
    #[inline(always)]
    fn set_lane(&mut self, i: usize, value: f32) {
        let mut lanes = self.to_array();
        lanes[i] = value;
        *self = F32x8::from_array(lanes);
    }
    #[inline(always)]
    fn mask_lane(mask: Self::Mask, i: usize) -> bool {
        mask.lane(i)
    }
    #[inline(always)]
    fn lane_sum(self) -> f32 {
        self.to_array().into_iter().sum()
    }
}

/// A stable address of one voice: its pack and its lane in that pack (ADR 0003 D1).
///
/// ```
/// use syrp_audio_basics::voice::VoiceHandle;
/// let h = VoiceHandle::new(1, 3);
/// assert_eq!((h.pack(), h.lane()), (1, 3));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VoiceHandle {
    pack: usize,
    lane: usize,
}

impl VoiceHandle {
    /// The voice in `lane` of pack `pack`.
    ///
    /// ```
    /// # use syrp_audio_basics::voice::VoiceHandle;
    /// assert_eq!(VoiceHandle::new(0, 0).pack(), 0);
    /// ```
    pub const fn new(pack: usize, lane: usize) -> Self {
        Self { pack, lane }
    }

    /// The pack index.
    ///
    /// ```
    /// # use syrp_audio_basics::voice::VoiceHandle;
    /// assert_eq!(VoiceHandle::new(4, 0).pack(), 4);
    /// ```
    pub const fn pack(self) -> usize {
        self.pack
    }

    /// The lane in the pack.
    ///
    /// ```
    /// # use syrp_audio_basics::voice::VoiceHandle;
    /// assert_eq!(VoiceHandle::new(0, 7).lane(), 7);
    /// ```
    pub const fn lane(self) -> usize {
        self.lane
    }
}

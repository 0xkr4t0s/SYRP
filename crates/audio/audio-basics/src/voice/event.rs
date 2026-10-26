// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`VoiceEvent`]: the timed, MIDI-free input of a [`VoicePool`](super::VoicePool).

/// What a [`VoiceEvent`] does. Channels are 1–16; events naming any other channel are ignored
/// by the pool. Velocities are 0–1, notes and controller values 0–127, the pitch wheel
/// `0..=16383` (centre [`PITCH_WHEEL_CENTRE`](super::PITCH_WHEEL_CENTRE)).
///
/// ```
/// use syrp_audio_basics::voice::{VoiceEvent, VoiceEventKind};
/// let e = VoiceEvent::new(10, VoiceEventKind::Sustain { channel: 1, down: true });
/// assert!(matches!(e.kind(), VoiceEventKind::Sustain { down: true, .. }));
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum VoiceEventKind {
    /// Start `note` on `channel`.
    NoteOn {
        /// 1–16.
        channel: u8,
        /// 0–127.
        note: u8,
        /// 0–1.
        velocity: f32,
    },
    /// Release `note` on `channel`, with tail-off.
    NoteOff {
        /// 1–16.
        channel: u8,
        /// 0–127.
        note: u8,
        /// Release velocity, 0–1.
        velocity: f32,
    },
    /// The sustain pedal of `channel` went down or up.
    Sustain {
        /// 1–16.
        channel: u8,
        /// True when pressed.
        down: bool,
    },
    /// The sostenuto pedal of `channel` went down or up.
    Sostenuto {
        /// 1–16.
        channel: u8,
        /// True when pressed.
        down: bool,
    },
    /// The pitch wheel of `channel` moved.
    PitchWheel {
        /// 1–16.
        channel: u8,
        /// `0..=16383`.
        value: u16,
    },
    /// Controller `number` of `channel` moved. Numbers above 127 are ignored.
    Controller {
        /// 1–16.
        channel: u8,
        /// 0–127.
        number: u8,
        /// 0–127.
        value: u8,
    },
    /// Release every note on `channel` (every channel when `None`), with tail-off.
    AllNotesOff {
        /// 1–16, or `None` for all channels.
        channel: Option<u8>,
    },
    /// Silence every voice at once, without tail-off.
    AllSoundOff,
}

/// One event for [`VoicePool::render`](super::VoicePool::render): a [`VoiceEventKind`] at a
/// sample `offset` from the start of the block. A block's events must be sorted by offset.
///
/// ```
/// use syrp_audio_basics::voice::{VoiceEvent, VoiceEventKind};
/// let e = VoiceEvent::note_on(64, 1, 60, 0.8);
/// assert_eq!(e.offset(), 64);
/// assert_eq!(e.kind(), VoiceEventKind::NoteOn { channel: 1, note: 60, velocity: 0.8 });
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoiceEvent {
    offset: u32,
    kind: VoiceEventKind,
}

impl VoiceEvent {
    /// `kind` at sample `offset`.
    ///
    /// ```
    /// # use syrp_audio_basics::voice::{VoiceEvent, VoiceEventKind};
    /// assert_eq!(VoiceEvent::new(3, VoiceEventKind::AllSoundOff).offset(), 3);
    /// ```
    pub const fn new(offset: u32, kind: VoiceEventKind) -> Self {
        Self { offset, kind }
    }

    /// A [`VoiceEventKind::NoteOn`] at `offset`.
    ///
    /// ```
    /// # use syrp_audio_basics::voice::{VoiceEvent, VoiceEventKind};
    /// let e = VoiceEvent::note_on(0, 2, 64, 1.0);
    /// assert!(matches!(e.kind(), VoiceEventKind::NoteOn { channel: 2, note: 64, .. }));
    /// ```
    pub const fn note_on(offset: u32, channel: u8, note: u8, velocity: f32) -> Self {
        Self::new(
            offset,
            VoiceEventKind::NoteOn {
                channel,
                note,
                velocity,
            },
        )
    }

    /// A [`VoiceEventKind::NoteOff`] at `offset`.
    ///
    /// ```
    /// # use syrp_audio_basics::voice::{VoiceEvent, VoiceEventKind};
    /// let e = VoiceEvent::note_off(9, 2, 64, 0.0);
    /// assert!(matches!(e.kind(), VoiceEventKind::NoteOff { note: 64, .. }));
    /// ```
    pub const fn note_off(offset: u32, channel: u8, note: u8, velocity: f32) -> Self {
        Self::new(
            offset,
            VoiceEventKind::NoteOff {
                channel,
                note,
                velocity,
            },
        )
    }

    /// The sample offset from the start of the block.
    ///
    /// ```
    /// # use syrp_audio_basics::voice::VoiceEvent;
    /// assert_eq!(VoiceEvent::note_off(7, 1, 60, 0.0).offset(), 7);
    /// ```
    pub const fn offset(&self) -> u32 {
        self.offset
    }

    /// What the event does.
    ///
    /// ```
    /// # use syrp_audio_basics::voice::{VoiceEvent, VoiceEventKind};
    /// let e = VoiceEvent::new(0, VoiceEventKind::AllNotesOff { channel: None });
    /// assert_eq!(e.kind(), VoiceEventKind::AllNotesOff { channel: None });
    /// ```
    pub const fn kind(&self) -> VoiceEventKind {
        self.kind
    }
}

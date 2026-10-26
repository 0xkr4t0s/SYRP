// SPDX-License-Identifier: MIT OR Apache-2.0
//! The Syrp MIDI contract (master plan §5, ADR 0003 D2 and D3).
//!
//! [`MidiMessage`] (owned, message thread) and [`MidiMessageRef`] (borrowed view) share the
//! [`MidiData`] accessors. [`MidiBuffer`] is a packed, sample-ordered event list whose
//! [`try_add_event`](MidiBuffer::try_add_event) never allocates. UMP follows later.
//!
//! Messages are stored as their MIDI 1.0 bytes: channel messages as status plus data bytes,
//! SysEx framed by `0xF0` and `0xF7`, and Standard MIDI File meta events as `0xFF`, type,
//! variable-length size and data. A lone `0xFF` is the System Reset real-time message.
//!
//! ```
//! use syrp_midi::{MidiData, MidiMessage};
//! let m = MidiMessage::from_bytes(&[0x90, 60, 100]).unwrap();
//! assert_eq!((m.view(32).sample_offset(), m.note_number()), (32, Some(60)));
//! ```

use std::fmt;

use syrp_error::{Diagnostic, ErrorCode};

mod buffer;
mod message;

pub use buffer::{Events, MidiBuffer};
pub use message::{MidiMessage, MidiMessageRef};

/// What kind of MIDI message a byte sequence holds.
///
/// A note-on with velocity 0 classifies as [`NoteOff`](Self::NoteOff), as MIDI 1.0 defines.
/// ```
/// use syrp_midi::{MidiData, MidiMessage, MidiMessageKind};
/// assert_eq!(MidiMessage::note_on(1, 60, 0).unwrap().kind(), MidiMessageKind::NoteOff);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MidiMessageKind {
    /// Note on with a non-zero velocity (`0x9n`).
    NoteOn,
    /// Note off (`0x8n`), or note on with velocity 0.
    NoteOff,
    /// Polyphonic key pressure, also called aftertouch (`0xAn`).
    PolyPressure,
    /// Control change (`0xBn`), channel mode messages included.
    Controller,
    /// Program change (`0xCn`).
    ProgramChange,
    /// Channel pressure (`0xDn`).
    ChannelPressure,
    /// Pitch bend (`0xEn`).
    PitchBend,
    /// System exclusive, `0xF0` ... `0xF7`.
    SysEx,
    /// System common other than SysEx (`0xF1` to `0xF6`).
    SystemCommon,
    /// System real-time (`0xF8` to `0xFF`, single byte).
    SystemRealtime,
    /// A Standard MIDI File meta event (`0xFF`, type, length, data).
    Meta,
}

/// Errors from building, parsing and buffering MIDI messages.
/// ```
/// use syrp_error::Diagnostic;
/// let err = syrp_midi::MidiMessage::from_bytes(&[0x90, 60]).unwrap_err();
/// assert_eq!(err.code().as_str(), "SYRP-MIDI-0001");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MidiError {
    /// The bytes are not exactly one well-formed message (`SYRP-MIDI-0001`).
    Malformed,
    /// A constructor argument is outside its MIDI range (`SYRP-MIDI-0002`).
    ValueOutOfRange,
    /// A non-allocating add found the event or byte capacity full (`SYRP-MIDI-0003`).
    BufferFull,
}

impl fmt::Display for MidiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            MidiError::Malformed => "malformed MIDI message bytes",
            MidiError::ValueOutOfRange => "MIDI value out of range",
            MidiError::BufferFull => "MidiBuffer capacity is full",
        })
    }
}

impl std::error::Error for MidiError {}

impl Diagnostic for MidiError {
    fn code(&self) -> ErrorCode {
        ErrorCode::new(match self {
            MidiError::Malformed => "SYRP-MIDI-0001",
            MidiError::ValueOutOfRange => "SYRP-MIDI-0002",
            MidiError::BufferFull => "SYRP-MIDI-0003",
        })
    }
}

mod sealed {
    pub trait Sealed {}
}

/// Read access to one MIDI message's bytes, shared by [`MidiMessage`] and [`MidiMessageRef`].
///
/// Accessors return `None` when the message is not of the kind they read. Channels are
/// numbered 1 to 16. The trait is sealed: only this crate implements it.
/// ```
/// use syrp_midi::{MidiData, MidiMessage};
/// let cc = MidiMessage::controller(2, 7, 100).unwrap();
/// assert_eq!((cc.channel(), cc.controller_value(), cc.note_number()), (Some(2), Some(100), None));
/// ```
pub trait MidiData: sealed::Sealed {
    /// The message's raw MIDI 1.0 bytes, status byte first.
    /// ```
    /// use syrp_midi::{MidiData, MidiMessage};
    /// assert_eq!(MidiMessage::program_change(1, 5).unwrap().bytes(), &[0xC0, 5]);
    /// ```
    #[doc(alias = "getRawData")]
    fn bytes(&self) -> &[u8];

    /// Classifies the message.
    /// ```
    /// use syrp_midi::{MidiData, MidiMessage, MidiMessageKind};
    /// assert_eq!(MidiMessage::pitch_bend(1, 8192).unwrap().kind(), MidiMessageKind::PitchBend);
    /// ```
    fn kind(&self) -> MidiMessageKind {
        classify(self.bytes())
    }

    /// The channel (1 to 16) of a channel voice message.
    /// ```
    /// use syrp_midi::{MidiData, MidiMessage};
    /// assert_eq!(MidiMessage::note_off(16, 60, 0).unwrap().channel(), Some(16));
    /// assert_eq!(MidiMessage::sysex(&[1, 2]).unwrap().channel(), None);
    /// ```
    #[doc(alias = "getChannel")]
    fn channel(&self) -> Option<u8> {
        let s = *self.bytes().first()?;
        (0x80..0xF0).contains(&s).then_some((s & 0x0F) + 1)
    }

    /// True for a note on with a non-zero velocity.
    /// ```
    /// use syrp_midi::{MidiData, MidiMessage};
    /// assert!(MidiMessage::note_on(1, 60, 1).unwrap().is_note_on());
    /// assert!(!MidiMessage::note_on(1, 60, 0).unwrap().is_note_on());
    /// ```
    #[doc(alias = "isNoteOn")]
    fn is_note_on(&self) -> bool {
        self.kind() == MidiMessageKind::NoteOn
    }

    /// True for a note off, or a note on with velocity 0.
    /// ```
    /// use syrp_midi::{MidiData, MidiMessage};
    /// assert!(MidiMessage::note_on(1, 60, 0).unwrap().is_note_off());
    /// ```
    #[doc(alias = "isNoteOff")]
    fn is_note_off(&self) -> bool {
        self.kind() == MidiMessageKind::NoteOff
    }

    /// The note number of a note on, note off or poly pressure message.
    /// ```
    /// use syrp_midi::{MidiData, MidiMessage};
    /// assert_eq!(MidiMessage::poly_pressure(1, 64, 9).unwrap().note_number(), Some(64));
    /// ```
    #[doc(alias = "getNoteNumber")]
    fn note_number(&self) -> Option<u8> {
        match self.bytes() {
            [s, n, _] if (0x80..0xB0).contains(s) => Some(*n),
            _ => None,
        }
    }

    /// The velocity of a note on or note off.
    /// ```
    /// use syrp_midi::{MidiData, MidiMessage};
    /// assert_eq!(MidiMessage::note_on(1, 60, 99).unwrap().velocity(), Some(99));
    /// ```
    #[doc(alias = "getVelocity")]
    fn velocity(&self) -> Option<u8> {
        match self.bytes() {
            [s, _, v] if (0x80..0xA0).contains(s) => Some(*v),
            _ => None,
        }
    }

    /// The controller number of a control change.
    /// ```
    /// use syrp_midi::{MidiData, MidiMessage};
    /// assert_eq!(MidiMessage::controller(1, 64, 127).unwrap().controller_number(), Some(64));
    /// ```
    #[doc(alias = "getControllerNumber")]
    fn controller_number(&self) -> Option<u8> {
        channel_data(self.bytes(), 0xB0).map(|(a, _)| a)
    }

    /// The value of a control change.
    /// ```
    /// use syrp_midi::{MidiData, MidiMessage};
    /// assert_eq!(MidiMessage::controller(1, 64, 127).unwrap().controller_value(), Some(127));
    /// ```
    #[doc(alias = "getControllerValue")]
    fn controller_value(&self) -> Option<u8> {
        channel_data(self.bytes(), 0xB0).map(|(_, b)| b)
    }

    /// The program number of a program change.
    /// ```
    /// use syrp_midi::{MidiData, MidiMessage};
    /// assert_eq!(MidiMessage::program_change(3, 42).unwrap().program_number(), Some(42));
    /// ```
    #[doc(alias = "getProgramChangeNumber")]
    fn program_number(&self) -> Option<u8> {
        channel_data(self.bytes(), 0xC0).map(|(a, _)| a)
    }

    /// The 14-bit value (0 to 16383, centre 8192) of a pitch bend.
    /// ```
    /// use syrp_midi::{MidiData, MidiMessage};
    /// assert_eq!(MidiMessage::pitch_bend(1, 16383).unwrap().pitch_bend_value(), Some(16383));
    /// ```
    #[doc(alias = "getPitchWheelValue")]
    fn pitch_bend_value(&self) -> Option<u16> {
        channel_data(self.bytes(), 0xE0).map(|(lsb, msb)| u16::from(lsb) | (u16::from(msb) << 7))
    }

    /// The pressure of a channel pressure message.
    /// ```
    /// use syrp_midi::{MidiData, MidiMessage};
    /// assert_eq!(MidiMessage::channel_pressure(1, 77).unwrap().channel_pressure_value(), Some(77));
    /// ```
    #[doc(alias = "getChannelPressureValue")]
    fn channel_pressure_value(&self) -> Option<u8> {
        channel_data(self.bytes(), 0xD0).map(|(a, _)| a)
    }

    /// The pressure of a polyphonic key pressure message.
    /// ```
    /// use syrp_midi::{MidiData, MidiMessage};
    /// assert_eq!(MidiMessage::poly_pressure(1, 64, 9).unwrap().poly_pressure_value(), Some(9));
    /// ```
    #[doc(alias = "getAfterTouchValue")]
    fn poly_pressure_value(&self) -> Option<u8> {
        channel_data(self.bytes(), 0xA0).map(|(_, b)| b)
    }

    /// The payload of a SysEx message, without the `0xF0` and `0xF7` framing bytes.
    /// ```
    /// use syrp_midi::{MidiData, MidiMessage}; let m = MidiMessage::sysex(&[0x7E, 0x01]).unwrap();
    /// assert_eq!(m.bytes(), &[0xF0, 0x7E, 0x01, 0xF7]);
    /// assert_eq!(m.sysex_data(), Some(&[0x7E, 0x01][..]));
    /// ```
    #[doc(alias = "getSysExData")]
    fn sysex_data(&self) -> Option<&[u8]> {
        sysex_payload(self.bytes())
    }

    /// The type byte of a meta event (for example `0x51` for a tempo change).
    /// ```
    /// use syrp_midi::{MidiData, MidiMessage};
    /// assert_eq!(MidiMessage::meta(0x2F, &[]).unwrap().meta_type(), Some(0x2F));
    /// ```
    #[doc(alias = "getMetaEventType")]
    fn meta_type(&self) -> Option<u8> {
        match self.bytes() {
            [0xFF, t, _, ..] => Some(*t),
            _ => None,
        }
    }

    /// The data of a meta event, after its variable-length size field.
    /// ```
    /// use syrp_midi::{MidiData, MidiMessage};
    /// let tempo = MidiMessage::meta(0x51, &[0x07, 0xA1, 0x20]).unwrap();
    /// assert_eq!(tempo.meta_data(), Some(&[0x07, 0xA1, 0x20][..]));
    /// ```
    #[doc(alias = "getMetaEventData")]
    fn meta_data(&self) -> Option<&[u8]> {
        meta_payload(self.bytes())
    }
}

/// The SysEx payload of `bytes`, without the `0xF0` and `0xF7` framing.
pub(crate) fn sysex_payload(bytes: &[u8]) -> Option<&[u8]> {
    match bytes {
        [0xF0, rest @ .., 0xF7] => Some(rest),
        _ => None,
    }
}

/// The meta event data of `bytes`, after its variable-length size field.
pub(crate) fn meta_payload(bytes: &[u8]) -> Option<&[u8]> {
    match bytes {
        [0xFF, _, rest @ ..] if !rest.is_empty() => read_var_len(rest).map(|(_, n)| &rest[n..]),
        _ => None,
    }
}

/// The data bytes of a channel message whose status high nibble is `status` (0 when absent).
fn channel_data(bytes: &[u8], status: u8) -> Option<(u8, u8)> {
    match bytes {
        [s, a] if s & 0xF0 == status => Some((*a, 0)),
        [s, a, b] if s & 0xF0 == status => Some((*a, *b)),
        _ => None,
    }
}

/// Classifies bytes already checked by [`validate`].
fn classify(bytes: &[u8]) -> MidiMessageKind {
    use MidiMessageKind as K;
    match bytes {
        [0xFF, _, ..] => K::Meta,
        [0xF0, ..] => K::SysEx,
        [s, ..] if *s >= 0xF8 => K::SystemRealtime,
        [s, ..] if *s >= 0xF0 => K::SystemCommon,
        [s, _, 0] if s & 0xF0 == 0x90 => K::NoteOff,
        [s, ..] => match s & 0xF0 {
            0x80 => K::NoteOff,
            0x90 => K::NoteOn,
            0xA0 => K::PolyPressure,
            0xB0 => K::Controller,
            0xC0 => K::ProgramChange,
            0xD0 => K::ChannelPressure,
            _ => K::PitchBend,
        },
        // Unreachable for validated bytes; classify defensively rather than panic.
        [] => K::SystemCommon,
    }
}

/// The total length of a message from its status byte, or `None` when it is variable
/// (SysEx, meta) or the byte is not a status byte.
fn fixed_len(status: u8) -> Option<usize> {
    match status {
        0x80..=0xBF | 0xE0..=0xEF | 0xF2 => Some(3),
        0xC0..=0xDF | 0xF1 | 0xF3 => Some(2),
        0xF4..=0xF6 | 0xF8..=0xFF => Some(1),
        _ => None,
    }
}

/// Reads a Standard MIDI File variable-length quantity (at most four bytes).
/// Returns the value and the number of bytes it used.
pub(crate) fn read_var_len(bytes: &[u8]) -> Option<(u32, usize)> {
    let mut value = 0_u32;
    for (i, &b) in bytes.iter().take(4).enumerate() {
        value = (value << 7) | u32::from(b & 0x7F);
        if b & 0x80 == 0 {
            return Some((value, i + 1));
        }
    }
    None
}

/// Checks that `bytes` hold exactly one complete, well-formed MIDI message.
pub(crate) fn validate(bytes: &[u8]) -> Result<(), MidiError> {
    let ok = match bytes {
        [] => false,
        [0xFF] => true,
        [0xFF, kind, rest @ ..] => {
            *kind < 0x80
                && read_var_len(rest).is_some_and(|(len, used)| rest.len() - used == len as usize)
        }
        [0xF0, mid @ .., 0xF7] => mid.iter().all(|b| *b < 0x80),
        [s, data @ ..] => fixed_len(*s) == Some(bytes.len()) && data.iter().all(|b| *b < 0x80),
    };
    if ok {
        Ok(())
    } else {
        Err(MidiError::Malformed)
    }
}

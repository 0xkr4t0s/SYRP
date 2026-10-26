// SPDX-License-Identifier: MIT OR Apache-2.0
//! The owned [`MidiMessage`] and the borrowed [`MidiMessageRef`] view.

use syrp_rt::RtSafe;

use crate::{MidiData, MidiError, sealed, validate};

/// An owned MIDI 1.0 message with a timestamp, for message-thread code.
///
/// Holds exactly one well-formed message: every constructor validates. The timestamp's unit
/// is the caller's choice (seconds in a sequence, samples in a buffer). Audio-thread code
/// reads borrowed [`MidiMessageRef`] views instead, because building a `MidiMessage` allocates.
/// ```
/// use syrp_midi::{MidiData, MidiMessage};
/// let m = MidiMessage::note_on(1, 60, 100).unwrap().with_timestamp(0.5);
/// assert_eq!((m.bytes(), m.timestamp()), (&[0x90, 60, 100][..], 0.5));
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct MidiMessage {
    bytes: Vec<u8>,
    timestamp: f64,
}

/// Maps a channel numbered 1 to 16 onto the low status nibble.
fn chan(channel: u8) -> Result<u8, MidiError> {
    match channel {
        1..=16 => Ok(channel - 1),
        _ => Err(MidiError::ValueOutOfRange),
    }
}

/// Checks a 7-bit data value.
fn d7(value: u8) -> Result<u8, MidiError> {
    if value < 0x80 {
        Ok(value)
    } else {
        Err(MidiError::ValueOutOfRange)
    }
}

impl MidiMessage {
    fn channel_msg(status: u8, channel: u8, data: &[u8]) -> Result<Self, MidiError> {
        let mut bytes = Vec::with_capacity(1 + data.len());
        bytes.push(status | chan(channel)?);
        for &d in data {
            bytes.push(d7(d)?);
        }
        Ok(MidiMessage {
            bytes,
            timestamp: 0.0,
        })
    }

    /// A note on. Channels are 1 to 16; note and velocity are 0 to 127.
    /// ```
    /// assert_eq!(syrp_midi::MidiMessage::note_on(1, 128, 1), Err(syrp_midi::MidiError::ValueOutOfRange));
    /// ```
    #[doc(alias = "noteOn")]
    pub fn note_on(channel: u8, note: u8, velocity: u8) -> Result<Self, MidiError> {
        Self::channel_msg(0x90, channel, &[note, velocity])
    }

    /// A note off with a release velocity.
    /// ```
    /// use syrp_midi::MidiData;
    /// assert!(syrp_midi::MidiMessage::note_off(1, 60, 64).unwrap().is_note_off());
    /// ```
    #[doc(alias = "noteOff")]
    pub fn note_off(channel: u8, note: u8, velocity: u8) -> Result<Self, MidiError> {
        Self::channel_msg(0x80, channel, &[note, velocity])
    }

    /// A polyphonic key pressure (aftertouch) message.
    /// ```
    /// let _ = syrp_midi::MidiMessage::poly_pressure(1, 60, 30).unwrap();
    /// ```
    #[doc(alias = "aftertouchChange")]
    pub fn poly_pressure(channel: u8, note: u8, pressure: u8) -> Result<Self, MidiError> {
        Self::channel_msg(0xA0, channel, &[note, pressure])
    }

    /// A control change.
    /// ```
    /// let _ = syrp_midi::MidiMessage::controller(1, 1, 64).unwrap();
    /// ```
    #[doc(alias = "controllerEvent")]
    pub fn controller(channel: u8, number: u8, value: u8) -> Result<Self, MidiError> {
        Self::channel_msg(0xB0, channel, &[number, value])
    }

    /// A program change.
    /// ```
    /// let _ = syrp_midi::MidiMessage::program_change(10, 0).unwrap();
    /// ```
    #[doc(alias = "programChange")]
    pub fn program_change(channel: u8, program: u8) -> Result<Self, MidiError> {
        Self::channel_msg(0xC0, channel, &[program])
    }

    /// A channel pressure message.
    /// ```
    /// let _ = syrp_midi::MidiMessage::channel_pressure(1, 100).unwrap();
    /// ```
    #[doc(alias = "channelPressureChange")]
    pub fn channel_pressure(channel: u8, pressure: u8) -> Result<Self, MidiError> {
        Self::channel_msg(0xD0, channel, &[pressure])
    }

    /// A pitch bend with a 14-bit value, 0 to 16383 (8192 is centre).
    /// ```
    /// use syrp_midi::MidiData;
    /// assert_eq!(syrp_midi::MidiMessage::pitch_bend(1, 8192).unwrap().bytes(), &[0xE0, 0x00, 0x40]);
    /// ```
    #[doc(alias = "pitchWheel")]
    pub fn pitch_bend(channel: u8, value: u16) -> Result<Self, MidiError> {
        if value > 0x3FFF {
            return Err(MidiError::ValueOutOfRange);
        }
        let [lsb, msb] = [(value & 0x7F) as u8, (value >> 7) as u8];
        Self::channel_msg(0xE0, channel, &[lsb, msb])
    }

    /// A SysEx message around `data`, which must not include the `0xF0`/`0xF7` framing.
    /// ```
    /// assert!(syrp_midi::MidiMessage::sysex(&[0x80]).is_err());
    /// ```
    #[doc(alias = "createSysExMessage")]
    pub fn sysex(data: &[u8]) -> Result<Self, MidiError> {
        if data.iter().any(|b| *b >= 0x80) {
            return Err(MidiError::ValueOutOfRange);
        }
        let mut bytes = Vec::with_capacity(data.len() + 2);
        bytes.push(0xF0);
        bytes.extend_from_slice(data);
        bytes.push(0xF7);
        Ok(MidiMessage {
            bytes,
            timestamp: 0.0,
        })
    }

    /// A Standard MIDI File meta event of type `meta_type` (0 to 127) carrying `data`
    /// (at most 2^28 - 1 bytes).
    /// ```
    /// use syrp_midi::MidiData; let end = syrp_midi::MidiMessage::meta(0x2F, &[]).unwrap();
    /// assert_eq!(end.bytes(), &[0xFF, 0x2F, 0x00]);
    /// ```
    pub fn meta(meta_type: u8, data: &[u8]) -> Result<Self, MidiError> {
        let len = u32::try_from(data.len()).map_err(|_| MidiError::ValueOutOfRange)?;
        if len >= 1 << 28 {
            return Err(MidiError::ValueOutOfRange);
        }
        let mut bytes = Vec::with_capacity(data.len() + 6);
        bytes.extend_from_slice(&[0xFF, d7(meta_type)?]);
        // Variable-length quantity: 7 bits per byte, most significant first, high bit set on
        // every byte but the last.
        let groups = (1..4).take_while(|i| len >> (7 * i) != 0).count();
        for i in (0..=groups).rev() {
            let more = if i > 0 { 0x80 } else { 0 };
            bytes.push(((len >> (7 * i)) & 0x7F) as u8 | more);
        }
        bytes.extend_from_slice(data);
        Ok(MidiMessage {
            bytes,
            timestamp: 0.0,
        })
    }

    /// Parses exactly one complete message (no running status), with timestamp 0.
    /// Never panics: anything else is [`MidiError::Malformed`].
    /// ```
    /// use syrp_midi::{MidiData, MidiMessage};
    /// assert_eq!(MidiMessage::from_bytes(&[0xB0, 7, 100]).unwrap().controller_value(), Some(100));
    /// assert!(MidiMessage::from_bytes(&[7, 100]).is_err());
    /// ```
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, MidiError> {
        validate(bytes)?;
        Ok(MidiMessage {
            bytes: bytes.to_vec(),
            timestamp: 0.0,
        })
    }

    /// The timestamp.
    /// ```
    /// assert_eq!(syrp_midi::MidiMessage::controller(1, 1, 1).unwrap().timestamp(), 0.0);
    /// ```
    #[doc(alias = "getTimeStamp")]
    #[must_use]
    pub fn timestamp(&self) -> f64 {
        self.timestamp
    }

    /// Sets the timestamp.
    /// ```
    /// let mut m = syrp_midi::MidiMessage::controller(1, 1, 1).unwrap();
    /// m.set_timestamp(2.0);
    /// assert_eq!(m.timestamp(), 2.0);
    /// ```
    #[doc(alias = "setTimeStamp")]
    pub fn set_timestamp(&mut self, timestamp: f64) {
        self.timestamp = timestamp;
    }

    /// Returns the message with its timestamp replaced.
    /// ```
    /// let m = syrp_midi::MidiMessage::controller(1, 1, 1).unwrap().with_timestamp(3.0);
    /// assert_eq!(m.timestamp(), 3.0);
    /// ```
    #[doc(alias = "withTimeStamp")]
    #[must_use]
    pub fn with_timestamp(mut self, timestamp: f64) -> Self {
        self.timestamp = timestamp;
        self
    }

    /// A borrowed view of the bytes, at the given sample offset.
    /// ```
    /// use syrp_midi::MidiData; let m = syrp_midi::MidiMessage::note_on(1, 60, 1).unwrap();
    /// assert_eq!(m.view(5).sample_offset(), 5);
    /// ```
    #[must_use]
    pub fn view(&self, sample_offset: u32) -> MidiMessageRef<'_> {
        MidiMessageRef::from_valid(&self.bytes, sample_offset)
    }
}

impl sealed::Sealed for MidiMessage {}

impl MidiData for MidiMessage {
    fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// A borrowed view of one MIDI message's bytes and its sample offset in a block.
///
/// The packed `MidiBuffer` (next increment) yields these without copying. `Copy`, and
/// usable on the audio thread ([`RtSafe`]).
/// ```
/// use syrp_midi::{MidiData, MidiMessageRef};
/// let r = MidiMessageRef::new(&[0x91, 64, 0], 10).unwrap();
/// assert_eq!((r.channel(), r.is_note_off(), r.sample_offset()), (Some(2), true, 10));
/// ```
#[doc(alias = "MidiMessageMetadata")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MidiMessageRef<'a> {
    bytes: &'a [u8],
    sample_offset: u32,
}

impl<'a> MidiMessageRef<'a> {
    /// A view of `bytes`, which must be exactly one well-formed message. Never panics.
    /// ```
    /// assert!(syrp_midi::MidiMessageRef::new(&[0xF0, 1], 0).is_err());
    /// ```
    pub fn new(bytes: &'a [u8], sample_offset: u32) -> Result<Self, MidiError> {
        validate(bytes)?;
        Ok(Self::from_valid(bytes, sample_offset))
    }

    pub(crate) fn from_valid(bytes: &'a [u8], sample_offset: u32) -> Self {
        MidiMessageRef {
            bytes,
            sample_offset,
        }
    }

    /// The position of the event within its block, in samples.
    /// ```
    /// assert_eq!(syrp_midi::MidiMessageRef::new(&[0xF8], 3).unwrap().sample_offset(), 3);
    /// ```
    #[doc(alias = "samplePosition")]
    #[must_use]
    pub fn sample_offset(self) -> u32 {
        self.sample_offset
    }

    /// The bytes, borrowed for the view's whole lifetime (unlike [`MidiData::bytes`]).
    /// ```
    /// let b = syrp_midi::MidiMessageRef::new(&[0xF8], 0).unwrap().as_slice();
    /// assert_eq!(b, &[0xF8]);
    /// ```
    #[must_use]
    pub fn as_slice(self) -> &'a [u8] {
        self.bytes
    }

    /// The SysEx payload, borrowed for the view's whole lifetime (unlike
    /// [`MidiData::sysex_data`], which borrows the view), so it can escape a `filter_map`.
    /// ```
    /// use syrp_midi::MidiMessageRef;
    /// let bytes = [0xF0, 0x7E, 0x01, 0xF7];
    /// let data = MidiMessageRef::new(&bytes, 0).ok().and_then(|r| r.sysex_data());
    /// assert_eq!(data, Some(&[0x7E, 0x01][..]));
    /// ```
    #[doc(alias = "getSysExData")]
    #[must_use]
    pub fn sysex_data(self) -> Option<&'a [u8]> {
        crate::sysex_payload(self.bytes)
    }

    /// The meta event data, borrowed for the view's whole lifetime (unlike
    /// [`MidiData::meta_data`]).
    /// ```
    /// use syrp_midi::MidiMessageRef;
    /// let bytes = [0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20];
    /// let data = MidiMessageRef::new(&bytes, 0).ok().and_then(|r| r.meta_data());
    /// assert_eq!(data, Some(&[0x07, 0xA1, 0x20][..]));
    /// ```
    #[doc(alias = "getMetaEventData")]
    #[must_use]
    pub fn meta_data(self) -> Option<&'a [u8]> {
        crate::meta_payload(self.bytes)
    }

    /// Copies the view into an owned message whose timestamp is the sample offset.
    /// Allocates: message thread only.
    /// ```
    /// let m = syrp_midi::MidiMessageRef::new(&[0xF8], 7).unwrap().to_message();
    /// assert_eq!(m.timestamp(), 7.0);
    /// ```
    #[doc(alias = "getMessage")]
    #[must_use]
    pub fn to_message(self) -> MidiMessage {
        MidiMessage {
            bytes: self.bytes.to_vec(),
            timestamp: f64::from(self.sample_offset),
        }
    }
}

impl sealed::Sealed for MidiMessageRef<'_> {}

impl MidiData for MidiMessageRef<'_> {
    fn bytes(&self) -> &[u8] {
        self.bytes
    }
}

// SAFETY: a shared slice reference and a `u32`; `Copy`, no destructor, and every method only
// reads the borrowed bytes (except `to_message`, documented as message-thread only).
unsafe impl RtSafe for MidiMessageRef<'_> {}

// SPDX-License-Identifier: MIT OR Apache-2.0
//! MIDI 2.0 Universal MIDI Packets (UMP): [`UmpPacket`], [`UmpMessageType`], [`SysExStatus`].
//!
//! Field layout follows the MIDI Association's published UMP and MIDI 2.0 protocol
//! specification. Word 0 carries the message type in bits 31..28 and, for grouped types, the
//! group in bits 27..24. Bytes are numbered from the most significant byte of word 0.

use syrp_rt::RtSafe;

use crate::MidiError;

/// The message type (MT) of a UMP, from the top four bits of its first word.
/// ```
/// use syrp_midi::{UmpMessageType, UmpPacket};
/// assert_eq!(UmpPacket::noop().message_type(), UmpMessageType::Utility);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum UmpMessageType {
    /// MT 0x0, 32 bits: NOOP, jitter-reduction clock and timestamp, delta clockstamps.
    Utility,
    /// MT 0x1, 32 bits: system common and system real-time.
    System,
    /// MT 0x2, 32 bits: MIDI 1.0 channel voice.
    Midi1ChannelVoice,
    /// MT 0x3, 64 bits: data messages carrying 7-bit SysEx.
    Data64,
    /// MT 0x4, 64 bits: MIDI 2.0 channel voice.
    Midi2ChannelVoice,
    /// MT 0x5, 128 bits: 8-bit SysEx and mixed data sets.
    Data128,
    /// MT 0xD, 128 bits: flex data (tempo, time signature, text, ...).
    FlexData,
    /// MT 0xF, 128 bits: UMP stream (endpoint and function block discovery, ...).
    Stream,
    /// A reserved message type (0x6 to 0xC, 0xE), carried as is so receivers can skip it.
    Reserved(u8),
}

impl UmpMessageType {
    /// The type for a raw four-bit MT value (only the low four bits are used).
    /// ```
    /// use syrp_midi::UmpMessageType;
    /// assert_eq!(UmpMessageType::from_nibble(0x4), UmpMessageType::Midi2ChannelVoice);
    /// ```
    #[must_use]
    pub fn from_nibble(mt: u8) -> Self {
        match mt & 0xF {
            0x0 => Self::Utility,
            0x1 => Self::System,
            0x2 => Self::Midi1ChannelVoice,
            0x3 => Self::Data64,
            0x4 => Self::Midi2ChannelVoice,
            0x5 => Self::Data128,
            0xD => Self::FlexData,
            0xF => Self::Stream,
            n => Self::Reserved(n),
        }
    }

    /// The packet size in 32-bit words for this type (1, 2, 3 or 4).
    /// ```
    /// use syrp_midi::UmpMessageType;
    /// assert_eq!(UmpMessageType::Data128.word_count(), 4);
    /// assert_eq!(UmpMessageType::Reserved(0xB).word_count(), 3);
    /// ```
    #[must_use]
    pub fn word_count(self) -> usize {
        match self {
            Self::Utility | Self::System | Self::Midi1ChannelVoice => 1,
            Self::Data64 | Self::Midi2ChannelVoice => 2,
            Self::Data128 | Self::FlexData | Self::Stream => 4,
            Self::Reserved(n) => match n {
                0x6 | 0x7 => 1,
                0x8..=0xA => 2,
                0xB | 0xC => 3,
                _ => 4,
            },
        }
    }
}

/// Where a SysEx packet sits in its message (Data 64 and Data 128 status values 0 to 3).
/// ```
/// use syrp_midi::{SysExStatus, UmpPacket};
/// let p = UmpPacket::sysex7(0, SysExStatus::Start, &[0x7E, 0x7F]).unwrap();
/// assert_eq!(p.sysex_status(), Some(SysExStatus::Start));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SysExStatus {
    /// The whole message fits in this packet.
    Complete,
    /// The first packet of several.
    Start,
    /// A middle packet.
    Continue,
    /// The last packet.
    End,
}

impl SysExStatus {
    fn from_nibble(n: u8) -> Option<Self> {
        [Self::Complete, Self::Start, Self::Continue, Self::End]
            .get(usize::from(n))
            .copied()
    }
}

/// One Universal MIDI Packet of 1 to 4 words, always well formed for its message type.
///
/// A plain `Copy` value. Serialise with [`words`](Self::words); parse with
/// [`read`](Self::read), which never panics. Groups are numbered 0 to 15 (the field value) and
/// channels 1 to 16, as elsewhere in this crate. Equality and hashing compare all words,
/// including unused and reserved bits (constructors zero them; `read` keeps them as given).
/// ```
/// use syrp_midi::UmpPacket;
/// let p = UmpPacket::midi1_channel_voice(0, 0x90, 60, 100).unwrap();
/// assert_eq!(p.words(), &[0x2090_3C64]);
/// let (q, rest) = UmpPacket::read(&[0x2090_3C64, 0]).unwrap();
/// assert_eq!((q, rest.len()), (p, 1));
/// ```
#[doc(alias = "UMP")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UmpPacket {
    words: [u32; 4],
}

/// Checks that `value` fits in `bits` bits.
fn bits(value: u32, bits: u32) -> Result<u32, MidiError> {
    if value >> bits == 0 {
        Ok(value)
    } else {
        Err(MidiError::ValueOutOfRange)
    }
}

/// Maps a channel numbered 1 to 16 onto the four-bit field.
fn chan(channel: u8) -> Result<u32, MidiError> {
    match channel {
        1..=16 => Ok(u32::from(channel - 1)),
        _ => Err(MidiError::ValueOutOfRange),
    }
}

impl UmpPacket {
    /// Builds word 0 from MT, group and the low 24 bits, then validates the whole packet.
    fn build(mt: u32, group: u8, low24: u32, rest: [u32; 3]) -> Result<Self, MidiError> {
        let w0 = (mt << 28) | (bits(group.into(), 4)? << 24) | low24;
        let p = UmpPacket {
            words: [w0, rest[0], rest[1], rest[2]],
        };
        p.check().map_err(|_| MidiError::ValueOutOfRange)?;
        Ok(p)
    }

    fn byte(&self, index: usize) -> u8 {
        (self.words[index / 4] >> (24 - 8 * (index % 4))) as u8
    }

    /// Packs `data` into the bytes from `first` on, as SysEx packets carry them.
    fn with_bytes(mut self, first: usize, data: &[u8]) -> Self {
        for (i, b) in data.iter().enumerate() {
            let at = first + i;
            self.words[at / 4] |= u32::from(*b) << (24 - 8 * (at % 4));
        }
        self
    }

    /// A Utility NOOP (MT 0x0, status 0).
    /// ```
    /// assert_eq!(syrp_midi::UmpPacket::noop().words(), &[0]);
    /// ```
    #[must_use]
    pub fn noop() -> Self {
        UmpPacket { words: [0; 4] }
    }

    /// A Utility message: `status` 0 to 15 (1 JR clock, 2 JR timestamp, 3 delta clockstamp
    /// ticks per quarter note, 4 delta clockstamp) and up to 20 bits of `data`.
    /// ```
    /// let jr = syrp_midi::UmpPacket::utility(1, 0x1234).unwrap();
    /// assert_eq!((jr.words(), jr.status()), (&[0x0010_1234][..], 1));
    /// ```
    pub fn utility(status: u8, data: u32) -> Result<Self, MidiError> {
        let low = (bits(status.into(), 4)? << 20) | bits(data, 20)?;
        Self::build(0x0, 0, low, [0; 3])
    }

    /// A System message (MT 0x1): `status` 0xF1 to 0xFF except 0xF7, two 7-bit data bytes.
    /// ```
    /// let clock = syrp_midi::UmpPacket::system(2, 0xF8, 0, 0).unwrap();
    /// assert_eq!(clock.words(), &[0x12F8_0000]);
    /// ```
    pub fn system(group: u8, status: u8, data1: u8, data2: u8) -> Result<Self, MidiError> {
        let low = u32::from_be_bytes([0, status, data1, data2]);
        Self::build(0x1, group, low, [0; 3])
    }

    /// A MIDI 1.0 channel voice message (MT 0x2): a status byte 0x80 to 0xEF (channel in its
    /// low nibble) and two 7-bit data bytes (0 when the message has only one).
    /// ```
    /// let pc = syrp_midi::UmpPacket::midi1_channel_voice(1, 0xC3, 5, 0).unwrap();
    /// assert_eq!((pc.words(), pc.channel()), (&[0x21C3_0500][..], Some(4)));
    /// ```
    pub fn midi1_channel_voice(group: u8, status: u8, d1: u8, d2: u8) -> Result<Self, MidiError> {
        let low = u32::from_be_bytes([0, status, d1, d2]);
        Self::build(0x2, group, low, [0; 3])
    }

    /// A Data 64 packet (MT 0x3) carrying up to six 7-bit SysEx bytes, without `0xF0`/`0xF7`.
    /// ```
    /// use syrp_midi::{SysExStatus, UmpPacket};
    /// let p = UmpPacket::sysex7(0, SysExStatus::Complete, &[1, 2, 3]).unwrap();
    /// assert_eq!(p.words(), &[0x3003_0102, 0x0300_0000]);
    /// ```
    pub fn sysex7(group: u8, status: SysExStatus, data: &[u8]) -> Result<Self, MidiError> {
        if data.len() > 6 || data.iter().any(|b| *b >= 0x80) {
            return Err(MidiError::ValueOutOfRange);
        }
        let low = ((status as u32) << 20) | ((data.len() as u32) << 16);
        Ok(Self::build(0x3, group, low, [0; 3])?.with_bytes(2, data))
    }

    /// A MIDI 2.0 channel voice message (MT 0x4): `opcode` 0 to 15 (for example 0x9 note on,
    /// 0xB control change), channel 1 to 16, the two index bytes of word 0, and word 1.
    /// ```
    /// let cc = syrp_midi::UmpPacket::midi2_channel_voice(0, 0xB, 1, 0x0700, u32::MAX).unwrap();
    /// assert_eq!((cc.opcode(), cc.index(), cc.value()), (Some(0xB), Some(0x0700), Some(u32::MAX)));
    /// ```
    pub fn midi2_channel_voice(
        group: u8,
        opcode: u8,
        channel: u8,
        index: u16,
        data: u32,
    ) -> Result<Self, MidiError> {
        let low = (bits(opcode.into(), 4)? << 20) | (chan(channel)? << 16) | u32::from(index);
        Self::build(0x4, group, low, [data, 0, 0])
    }

    /// A MIDI 2.0 note on: 7-bit note, 16-bit velocity, attribute type and 16-bit attribute.
    /// ```
    /// let on = syrp_midi::UmpPacket::midi2_note_on(0, 1, 60, 0xFFFF, 0, 0).unwrap();
    /// assert_eq!(on.words(), &[0x4090_3C00, 0xFFFF_0000]);
    /// ```
    #[doc(alias = "noteOn")]
    pub fn midi2_note_on(
        group: u8,
        channel: u8,
        note: u8,
        velocity: u16,
        attribute_type: u8,
        attribute: u16,
    ) -> Result<Self, MidiError> {
        let index = (u16::from(bits(note.into(), 7)? as u8) << 8) | u16::from(attribute_type);
        let data = (u32::from(velocity) << 16) | u32::from(attribute);
        Self::midi2_channel_voice(group, 0x9, channel, index, data)
    }

    /// A MIDI 2.0 note off, laid out as [`midi2_note_on`](Self::midi2_note_on).
    /// ```
    /// let off = syrp_midi::UmpPacket::midi2_note_off(0, 1, 60, 0, 0, 0).unwrap();
    /// assert_eq!(off.opcode(), Some(0x8));
    /// ```
    #[doc(alias = "noteOff")]
    pub fn midi2_note_off(
        group: u8,
        channel: u8,
        note: u8,
        velocity: u16,
        attribute_type: u8,
        attribute: u16,
    ) -> Result<Self, MidiError> {
        let on = Self::midi2_note_on(group, channel, note, velocity, attribute_type, attribute)?;
        Ok(UmpPacket {
            words: [on.words[0] ^ (0x1 << 20), on.words[1], 0, 0],
        })
    }

    /// A Data 128 SysEx8 packet (MT 0x5): a stream id and up to 13 bytes of 8-bit data.
    /// ```
    /// use syrp_midi::{SysExStatus, UmpPacket};
    /// let p = UmpPacket::sysex8(0, SysExStatus::End, 7, &[0xFF]).unwrap();
    /// assert_eq!((p.stream_id(), p.sysex_bytes().unwrap().len()), (Some(7), 1));
    /// ```
    pub fn sysex8(
        group: u8,
        status: SysExStatus,
        stream_id: u8,
        data: &[u8],
    ) -> Result<Self, MidiError> {
        if data.len() > 13 {
            return Err(MidiError::ValueOutOfRange);
        }
        // The byte count includes the stream id.
        let low =
            ((status as u32) << 20) | ((data.len() as u32 + 1) << 16) | (u32::from(stream_id) << 8);
        Ok(Self::build(0x5, group, low, [0; 3])?.with_bytes(3, data))
    }

    /// A Flex Data packet (MT 0xD): `form` and `address` (two bits each), channel 1 to 16,
    /// status bank, status and the three payload words.
    /// ```
    /// // Set Tempo (bank 0, status 0) in 10-nanosecond units.
    /// let tempo = syrp_midi::UmpPacket::flex_data(0, 0, 1, 1, 0, 0, [50_000_000, 0, 0]).unwrap();
    /// assert_eq!(tempo.words()[0], 0xD010_0000);
    /// ```
    pub fn flex_data(
        group: u8,
        form: u8,
        address: u8,
        channel: u8,
        bank: u8,
        status: u8,
        data: [u32; 3],
    ) -> Result<Self, MidiError> {
        let low = (bits(form.into(), 2)? << 22)
            | (bits(address.into(), 2)? << 20)
            | (chan(channel)? << 16)
            | u32::from(u16::from_be_bytes([bank, status]));
        Self::build(0xD, group, low, data)
    }

    /// A UMP Stream packet (MT 0xF): `format` (two bits), a 10-bit `status`, the low 16 bits
    /// of word 0 and three more words.
    /// ```
    /// // Endpoint Discovery (status 0x000), UMP version 1.1, filter 0x1F.
    /// let p = syrp_midi::UmpPacket::stream(0, 0x000, 0x0101, [0x1F, 0, 0]).unwrap();
    /// assert_eq!(p.words(), &[0xF000_0101, 0x1F, 0, 0]);
    /// ```
    pub fn stream(format: u8, status: u16, data0: u16, data: [u32; 3]) -> Result<Self, MidiError> {
        let low = (bits(status.into(), 10)? << 16) | u32::from(data0);
        let w0 = (0xF << 28) | (bits(format.into(), 2)? << 26) | low;
        Ok(UmpPacket {
            words: [w0, data[0], data[1], data[2]],
        })
    }

    /// Parses the first packet of `words` and returns it with the words after it.
    ///
    /// Fails with [`MidiError::MalformedUmp`] when `words` is shorter than the packet or a
    /// field is invalid for its message type. Never panics.
    /// ```
    /// use syrp_midi::{MidiError, UmpPacket};
    /// assert_eq!(UmpPacket::read(&[0x4090_3C00]), Err(MidiError::MalformedUmp)); // needs 2 words
    /// ```
    pub fn read(words: &[u32]) -> Result<(Self, &[u32]), MidiError> {
        let w0 = *words.first().ok_or(MidiError::MalformedUmp)?;
        let n = UmpMessageType::from_nibble((w0 >> 28) as u8).word_count();
        let (head, rest) = words.split_at_checked(n).ok_or(MidiError::MalformedUmp)?;
        let mut p = UmpPacket { words: [0; 4] };
        p.words[..n].copy_from_slice(head);
        p.check()?;
        Ok((p, rest))
    }

    /// Checks the fields a message type constrains.
    fn check(&self) -> Result<(), MidiError> {
        let [_, s, d1, d2] = self.words[0].to_be_bytes();
        let n = (s & 0xF) as usize;
        let ok = match self.message_type() {
            UmpMessageType::System => s >= 0xF1 && s != 0xF7 && d1 < 0x80 && d2 < 0x80,
            UmpMessageType::Midi1ChannelVoice => {
                (0x80..0xF0).contains(&s) && d1 < 0x80 && d2 < 0x80
            }
            UmpMessageType::Data64 => {
                s >> 4 <= 3 && n <= 6 && (0..n).all(|i| self.byte(2 + i) < 0x80)
            }
            UmpMessageType::Data128 => match s >> 4 {
                0..=3 => (1..=14).contains(&n),
                st => st == 8 || st == 9,
            },
            _ => true,
        };
        if ok {
            Ok(())
        } else {
            Err(MidiError::MalformedUmp)
        }
    }

    /// The packet's words, 1 to 4 of them.
    /// ```
    /// assert_eq!(syrp_midi::UmpPacket::noop().words().len(), 1);
    /// ```
    #[must_use]
    pub fn words(&self) -> &[u32] {
        &self.words[..self.message_type().word_count()]
    }

    /// The message type.
    /// ```
    /// use syrp_midi::{UmpMessageType, UmpPacket};
    /// assert_eq!(UmpPacket::read(&[0x6000_0000]).unwrap().0.message_type(), UmpMessageType::Reserved(6));
    /// ```
    #[must_use]
    pub fn message_type(&self) -> UmpMessageType {
        UmpMessageType::from_nibble((self.words[0] >> 28) as u8)
    }

    /// The group (0 to 15), or `None` for the groupless Utility, Stream and reserved types.
    /// ```
    /// assert_eq!(syrp_midi::UmpPacket::system(9, 0xFE, 0, 0).unwrap().group(), Some(9));
    /// ```
    #[doc(alias = "getGroup")]
    #[must_use]
    pub fn group(&self) -> Option<u8> {
        use UmpMessageType as T;
        match self.message_type() {
            T::Utility | T::Stream | T::Reserved(_) => None,
            _ => Some(((self.words[0] >> 24) & 0xF) as u8),
        }
    }

    /// The channel (1 to 16) of a channel voice or flex data packet.
    /// ```
    /// assert_eq!(syrp_midi::UmpPacket::midi2_note_on(0, 16, 60, 1, 0, 0).unwrap().channel(), Some(16));
    /// ```
    #[doc(alias = "getChannel")]
    #[must_use]
    pub fn channel(&self) -> Option<u8> {
        use UmpMessageType as T;
        match self.message_type() {
            T::Midi1ChannelVoice | T::Midi2ChannelVoice | T::FlexData => {
                Some(((self.words[0] >> 16) & 0xF) as u8 + 1)
            }
            _ => None,
        }
    }

    /// The status field, whose meaning depends on the message type: the Utility status nibble,
    /// the System or MIDI 1.0 status byte, the Data 64/128 status nibble, the MIDI 2.0 status
    /// byte (opcode and channel), Flex Data bank and status (`bank << 8 | status`), the 10-bit
    /// Stream status, or 0 for reserved types.
    /// ```
    /// assert_eq!(syrp_midi::UmpPacket::system(0, 0xF2, 1, 2).unwrap().status(), 0xF2);
    /// ```
    #[must_use]
    pub fn status(&self) -> u16 {
        use UmpMessageType as T;
        let w0 = self.words[0];
        let v = match self.message_type() {
            T::Utility | T::Data64 | T::Data128 => (w0 >> 20) & 0xF,
            T::System | T::Midi1ChannelVoice | T::Midi2ChannelVoice => (w0 >> 16) & 0xFF,
            T::FlexData => w0 & 0xFFFF,
            T::Stream => (w0 >> 16) & 0x3FF,
            T::Reserved(_) => 0,
        };
        v as u16
    }

    /// The opcode (status high nibble, 0x8 to 0xE for MIDI 1.0, 0 to 15 for MIDI 2.0) of a
    /// channel voice packet.
    /// ```
    /// assert_eq!(syrp_midi::UmpPacket::midi1_channel_voice(0, 0xE0, 0, 64).unwrap().opcode(), Some(0xE));
    /// ```
    #[must_use]
    pub fn opcode(&self) -> Option<u8> {
        self.channel()
            .filter(|_| self.message_type() != UmpMessageType::FlexData)?;
        Some((self.status() >> 4) as u8)
    }

    /// The two index bytes of a MIDI 2.0 channel voice packet (note and attribute type, or
    /// bank and index, by opcode).
    /// ```
    /// assert_eq!(syrp_midi::UmpPacket::midi2_note_on(0, 1, 60, 1, 3, 0).unwrap().index(), Some(0x3C03));
    /// ```
    #[must_use]
    pub fn index(&self) -> Option<u16> {
        (self.message_type() == UmpMessageType::Midi2ChannelVoice).then_some(self.words[0] as u16)
    }

    /// The 32-bit data word of a MIDI 2.0 channel voice packet (velocity and attribute for
    /// notes, the value for controllers, pressure and pitch bend).
    /// ```
    /// assert_eq!(syrp_midi::UmpPacket::midi2_note_on(0, 1, 60, 0x8000, 0, 0).unwrap().value(), Some(0x8000_0000));
    /// ```
    #[must_use]
    pub fn value(&self) -> Option<u32> {
        (self.message_type() == UmpMessageType::Midi2ChannelVoice).then_some(self.words[1])
    }

    /// The SysEx status of a Data 64 or SysEx8 Data 128 packet.
    /// ```
    /// use syrp_midi::{SysExStatus, UmpPacket};
    /// assert_eq!(UmpPacket::noop().sysex_status(), None);
    /// ```
    #[must_use]
    pub fn sysex_status(&self) -> Option<SysExStatus> {
        match self.message_type() {
            UmpMessageType::Data64 | UmpMessageType::Data128 => {
                SysExStatus::from_nibble(self.status() as u8)
            }
            _ => None,
        }
    }

    /// The stream id of a SysEx8 packet.
    /// ```
    /// use syrp_midi::{SysExStatus, UmpPacket};
    /// assert_eq!(UmpPacket::sysex8(0, SysExStatus::Complete, 3, &[]).unwrap().stream_id(), Some(3));
    /// ```
    #[must_use]
    pub fn stream_id(&self) -> Option<u8> {
        (self.message_type() == UmpMessageType::Data128 && self.sysex_status().is_some())
            .then(|| self.byte(2))
    }

    /// The SysEx data bytes of a Data 64 or SysEx8 packet (without the stream id), by value.
    /// ```
    /// use syrp_midi::{SysExStatus, UmpPacket};
    /// let p = UmpPacket::sysex7(0, SysExStatus::Continue, &[4, 5]).unwrap();
    /// assert_eq!(p.sysex_bytes().unwrap().collect::<Vec<u8>>(), [4, 5]);
    /// ```
    #[must_use]
    pub fn sysex_bytes(self) -> Option<impl ExactSizeIterator<Item = u8> + use<>> {
        self.sysex_status()?;
        let n = (self.words[0] >> 16 & 0xF) as usize;
        let (first, count) = match self.message_type() {
            UmpMessageType::Data64 => (2, n),
            _ => (3, n.saturating_sub(1)),
        };
        Some((first..first + count).map(move |i| self.byte(i)))
    }
}

// SAFETY: four `u32` words inline; `Copy`, no destructor, and no method allocates.
unsafe impl RtSafe for UmpPacket {}

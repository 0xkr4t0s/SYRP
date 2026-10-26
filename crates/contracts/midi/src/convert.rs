// SPDX-License-Identifier: MIT OR Apache-2.0
//! Conversion between MIDI 1.0 byte streams and Universal MIDI Packets (MT 0x1, 0x2, 0x3).
//!
//! Follows the MIDI 1.0 byte-stream rules (running status for channel messages, system
//! common messages cancel it, real-time bytes may appear anywhere, any status byte other than
//! real-time ends a SysEx) and the UMP specification's mapping of MIDI 1.0 messages onto
//! System, MIDI 1.0 Channel Voice and Data 64 packets.

use std::iter::Take;

use syrp_rt::RtSafe;

use crate::{SysExStatus, UmpMessageType, UmpPacket, fixed_len};

/// Turns a MIDI 1.0 byte stream into Universal MIDI Packets for one group.
///
/// Feed bytes as they arrive, in any chunking; each complete message is passed to the `emit`
/// callback as a packet. Running status is expanded, real-time bytes are emitted at once
/// (even inside another message or a SysEx), and SysEx is split into Data 64 packets of up to
/// six bytes. Bytes the stream rules make meaningless (a data byte with no status, a stray
/// `0xF7`, the undefined statuses `0xF4`, `0xF5`, `0xF9`, `0xFD`) are dropped, never a panic.
/// Holds no heap memory: every method is RT-safe.
/// ```
/// use syrp_midi::{BytesToUmp, UmpPacket};
/// let mut dec = BytesToUmp::new(0).unwrap();
/// let mut out = Vec::new();
/// // A note on, then a second one in running status.
/// dec.feed(&[0x90, 60, 100, 62, 100], |p| out.push(p));
/// assert_eq!(out[1], UmpPacket::midi1_channel_voice(0, 0x90, 62, 100).unwrap());
/// ```
#[doc(alias = "BytestreamToUMP")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BytesToUmp {
    group: u8,
    /// The status whose data bytes are being collected; 0 when none.
    status: u8,
    data: [u8; 6],
    len: u8,
    in_sysex: bool,
    /// A Start packet has gone out for the current SysEx.
    sysex_started: bool,
}

impl BytesToUmp {
    /// A decoder whose packets carry `group` (0 to 15). `None` for a larger group.
    /// ```
    /// assert!(syrp_midi::BytesToUmp::new(16).is_none());
    /// ```
    #[must_use]
    pub fn new(group: u8) -> Option<Self> {
        (group < 16).then_some(BytesToUmp {
            group,
            status: 0,
            data: [0; 6],
            len: 0,
            in_sysex: false,
            sysex_started: false,
        })
    }

    /// Forgets any partial message, SysEx and running status (after a cable reconnect, say).
    /// ```
    /// let mut dec = syrp_midi::BytesToUmp::new(0).unwrap();
    /// dec.feed(&[0x90, 60], |_| {});
    /// dec.reset();
    /// let mut n = 0;
    /// dec.feed(&[100], |_| n += 1); // no status any more: dropped
    /// assert_eq!(n, 0);
    /// ```
    pub fn reset(&mut self) {
        let group = self.group;
        *self = BytesToUmp {
            group,
            status: 0,
            data: [0; 6],
            len: 0,
            in_sysex: false,
            sysex_started: false,
        };
    }

    /// Feeds every byte of `bytes` in order. See [`push`](Self::push).
    /// ```
    /// let mut dec = syrp_midi::BytesToUmp::new(0).unwrap();
    /// let mut n = 0;
    /// dec.feed(&[0xF0, 1, 2, 0xF7, 0xF8], |_| n += 1);
    /// assert_eq!(n, 2);
    /// ```
    pub fn feed(&mut self, bytes: &[u8], mut emit: impl FnMut(UmpPacket)) {
        for &b in bytes {
            self.push(b, &mut emit);
        }
    }

    /// Feeds one byte, calling `emit` for each packet it completes (at most two: a SysEx
    /// cut short by a new status, then a one-byte message).
    /// ```
    /// let mut dec = syrp_midi::BytesToUmp::new(3).unwrap();
    /// let mut last = None;
    /// for b in [0xC0, 5] { dec.push(b, |p| last = Some(p)); }
    /// assert_eq!(last.unwrap().words(), &[0x23C0_0500]);
    /// ```
    pub fn push(&mut self, byte: u8, mut emit: impl FnMut(UmpPacket)) {
        if byte >= 0xF8 {
            if byte != 0xF9 && byte != 0xFD {
                self.emit_system(byte, [0, 0], &mut emit);
            }
            return;
        }
        if self.in_sysex {
            if byte < 0x80 {
                if usize::from(self.len) == self.data.len() {
                    let status = if self.sysex_started {
                        SysExStatus::Continue
                    } else {
                        SysExStatus::Start
                    };
                    self.emit_sysex(status, &mut emit);
                    self.sysex_started = true;
                }
                self.data[usize::from(self.len)] = byte;
                self.len += 1;
                return;
            }
            let status = if self.sysex_started {
                SysExStatus::End
            } else {
                SysExStatus::Complete
            };
            self.emit_sysex(status, &mut emit);
            self.in_sysex = false;
            if byte == 0xF7 {
                return;
            }
        }
        if byte >= 0x80 {
            self.len = 0;
            self.status = 0;
            match byte {
                0xF0 => (self.in_sysex, self.sysex_started) = (true, false),
                0x80..=0xEF | 0xF1..=0xF3 => self.status = byte,
                0xF6 => self.emit_system(byte, [0, 0], &mut emit),
                _ => {} // 0xF4, 0xF5 undefined; 0xF7 stray
            }
            return;
        }
        if self.status == 0 {
            return; // a data byte with no status to apply it to
        }
        self.data[usize::from(self.len)] = byte;
        self.len += 1;
        let needed = fixed_len(self.status).unwrap_or(1) - 1;
        if usize::from(self.len) < needed {
            return;
        }
        let (s, d) = (
            self.status,
            [self.data[0], if needed == 2 { self.data[1] } else { 0 }],
        );
        self.len = 0;
        if s < 0xF0 {
            // Running status: keep `status` for the next data bytes.
            if let Ok(p) = UmpPacket::midi1_channel_voice(self.group, s, d[0], d[1]) {
                emit(p);
            }
        } else {
            self.status = 0; // system common cancels running status
            self.emit_system(s, d, &mut emit);
        }
    }

    fn emit_system(&self, status: u8, d: [u8; 2], emit: &mut impl FnMut(UmpPacket)) {
        if let Ok(p) = UmpPacket::system(self.group, status, d[0], d[1]) {
            emit(p);
        }
    }

    fn emit_sysex(&mut self, status: SysExStatus, emit: &mut impl FnMut(UmpPacket)) {
        let data = &self.data[..usize::from(self.len)];
        if let Ok(p) = UmpPacket::sysex7(self.group, status, data) {
            emit(p);
        }
        self.len = 0;
    }
}

// SAFETY: plain `Copy` data with no destructor; no method allocates, locks or blocks.
unsafe impl RtSafe for BytesToUmp {}

/// The MIDI 1.0 bytes of one packet, from [`UmpPacket::midi1_bytes`].
type Midi1Bytes = Take<std::array::IntoIter<u8, 8>>;

impl UmpPacket {
    /// The MIDI 1.0 byte-stream form of a System, MIDI 1.0 Channel Voice or Data 64 packet,
    /// by value; `None` for every other message type.
    ///
    /// SysEx packets map to `0xF0` (Complete and Start), their data, and `0xF7` (Complete and
    /// End), so concatenating the bytes of a packet sequence rebuilds the original SysEx.
    /// Stateless: an End without its Start gives data bytes and a lone `0xF7`.
    /// ```
    /// use syrp_midi::{SysExStatus, UmpPacket};
    /// let p = UmpPacket::sysex7(0, SysExStatus::Complete, &[1, 2]).unwrap();
    /// assert_eq!(p.midi1_bytes().unwrap().collect::<Vec<u8>>(), [0xF0, 1, 2, 0xF7]);
    /// let pc = UmpPacket::midi1_channel_voice(0, 0xC2, 7, 0).unwrap();
    /// assert_eq!(pc.midi1_bytes().unwrap().collect::<Vec<u8>>(), [0xC2, 7]);
    /// ```
    #[must_use]
    pub fn midi1_bytes(self) -> Option<impl ExactSizeIterator<Item = u8> + use<>> {
        let [_, s, d1, d2] = self.words()[0].to_be_bytes();
        let mut out = [0_u8; 8];
        let len = match self.message_type() {
            UmpMessageType::System | UmpMessageType::Midi1ChannelVoice => {
                out[..3].copy_from_slice(&[s, d1, d2]);
                fixed_len(s)?
            }
            UmpMessageType::Data64 => {
                let status = self.sysex_status()?;
                let opens = matches!(status, SysExStatus::Complete | SysExStatus::Start);
                let closes = matches!(status, SysExStatus::Complete | SysExStatus::End);
                let mut n = 0;
                let bytes = opens.then_some(0xF0).into_iter().chain(self.sysex_bytes()?);
                for b in bytes.chain(closes.then_some(0xF7)) {
                    out[n] = b;
                    n += 1;
                }
                n
            }
            _ => return None,
        };
        let it: Midi1Bytes = out.into_iter().take(len);
        Some(it)
    }
}

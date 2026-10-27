// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Registered and non-registered parameter numbers (RPN and NRPN): [`MidiRpnDetector`]
//! assembles them from control changes, and [`MidiRpnGenerator`] writes them as control
//! changes.
//!
//! The MIDI 1.0 specification selects a parameter with controllers 101 and 100 (RPN MSB and
//! LSB) or 99 and 98 (NRPN MSB and LSB), then sets its value with data entry: controller 6
//! (value MSB) and controller 38 (value LSB). RPN 127/127 is the null parameter, which
//! deselects.
//!
//! ```
//! use syrp_midi::MidiData;
//! use syrp_midi_utils::rpn::{MidiRpnDetector, MidiRpnGenerator};
//! let mut detector = MidiRpnDetector::new();
//! let mut found = None;
//! for m in MidiRpnGenerator::generate(1, 0, 2 << 7, false, true)? {
//!     let (c, v) = (m.controller_number().unwrap(), m.controller_value().unwrap());
//!     found = found.or(detector.try_parse(1, c, v));
//! }
//! let rpn = found.unwrap();
//! assert_eq!((rpn.parameter_number(), rpn.value(), rpn.is_14_bit()), (0, 256, true));
//! # Ok::<(), syrp_midi::MidiError>(())
//! ```

use syrp_midi::{MidiError, MidiMessage};

/// Controller numbers of the RPN and NRPN protocol.
const NRPN_LSB: u8 = 98;
const NRPN_MSB: u8 = 99;
const RPN_LSB: u8 = 100;
const RPN_MSB: u8 = 101;
const DATA_MSB: u8 = 6;
const DATA_LSB: u8 = 38;

/// One complete RPN or NRPN value change, in the role of JUCE's `MidiRPNMessage`.
/// ```
/// let m = syrp_midi_utils::rpn::MidiRpnMessage::new(1, 6, 48, false, false)?;
/// assert_eq!((m.channel(), m.parameter_number(), m.value(), m.is_nrpn()), (1, 6, 48, false));
/// # Ok::<(), syrp_midi::MidiError>(())
/// ```
#[doc(alias = "MidiRPNMessage")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MidiRpnMessage {
    channel: u8,
    parameter_number: u16,
    value: u16,
    is_nrpn: bool,
    is_14_bit: bool,
}

impl MidiRpnMessage {
    /// A message for `channel` (1 to 16) and `parameter_number` (0 to 16383), with a `value`
    /// of 0 to 16383 when `is_14_bit`, else 0 to 127. Anything else is
    /// [`MidiError::ValueOutOfRange`].
    /// ```
    /// use syrp_midi_utils::rpn::MidiRpnMessage;
    /// assert!(MidiRpnMessage::new(1, 0, 128, false, false).is_err());
    /// assert!(MidiRpnMessage::new(1, 0, 128, false, true).is_ok());
    /// ```
    pub fn new(
        channel: u8,
        parameter_number: u16,
        value: u16,
        is_nrpn: bool,
        is_14_bit: bool,
    ) -> Result<Self, MidiError> {
        let max_value = if is_14_bit { 0x3FFF } else { 0x7F };
        if !(1..=16).contains(&channel) || parameter_number > 0x3FFF || value > max_value {
            return Err(MidiError::ValueOutOfRange);
        }
        Ok(MidiRpnMessage {
            channel,
            parameter_number,
            value,
            is_nrpn,
            is_14_bit,
        })
    }

    /// The channel, 1 to 16.
    /// ```
    /// # let m = syrp_midi_utils::rpn::MidiRpnMessage::new(9, 0, 0, false, false).unwrap();
    /// assert_eq!(m.channel(), 9);
    /// ```
    #[must_use]
    pub fn channel(self) -> u8 {
        self.channel
    }

    /// The 14-bit parameter number (MSB times 128 plus LSB).
    /// ```
    /// # let m = syrp_midi_utils::rpn::MidiRpnMessage::new(1, 300, 0, true, false).unwrap();
    /// assert_eq!(m.parameter_number(), 300);
    /// ```
    #[doc(alias = "parameterNumber")]
    #[must_use]
    pub fn parameter_number(self) -> u16 {
        self.parameter_number
    }

    /// The value: 7-bit (the data entry MSB) or 14-bit (MSB times 128 plus LSB).
    /// ```
    /// # let m = syrp_midi_utils::rpn::MidiRpnMessage::new(1, 0, 9000, false, true).unwrap();
    /// assert_eq!(m.value(), 9000);
    /// ```
    #[must_use]
    pub fn value(self) -> u16 {
        self.value
    }

    /// True for an NRPN, false for an RPN.
    /// ```
    /// # let m = syrp_midi_utils::rpn::MidiRpnMessage::new(1, 0, 0, true, false).unwrap();
    /// assert!(m.is_nrpn());
    /// ```
    #[doc(alias = "isNRPN")]
    #[must_use]
    pub fn is_nrpn(self) -> bool {
        self.is_nrpn
    }

    /// True when the value is 14-bit.
    /// ```
    /// # let m = syrp_midi_utils::rpn::MidiRpnMessage::new(1, 0, 0, false, true).unwrap();
    /// assert!(m.is_14_bit());
    /// ```
    #[doc(alias = "is14BitValue")]
    #[must_use]
    pub fn is_14_bit(self) -> bool {
        self.is_14_bit
    }
}

/// The parsing state of one channel.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ChannelState {
    parameter_msb: Option<u8>,
    parameter_lsb: Option<u8>,
    is_nrpn: bool,
    value_lsb: Option<u8>,
}

impl ChannelState {
    /// The selected parameter, unless it is incomplete or the null parameter (127/127).
    fn parameter(self) -> Option<u16> {
        match (self.parameter_msb?, self.parameter_lsb?) {
            (0x7F, 0x7F) => None,
            (msb, lsb) => Some(u16::from(msb) << 7 | u16::from(lsb)),
        }
    }
}

/// Assembles RPN and NRPN messages from a stream of control changes, in the role of JUCE's
/// `MidiRPNDetector`. Each of the 16 channels is parsed independently.
///
/// Rules (from the `MidiRPNDetector` documentation and the MIDI 1.0 specification):
/// - Both parameter number bytes (in either order) must arrive before the value. Selecting a
///   parameter byte of the other kind (RPN after NRPN or back) forgets the other byte.
/// - A value LSB (controller 38) applies to the value MSB (controller 6) that follows it; the
///   message is reported when the MSB arrives, as 14-bit if an LSB came first, otherwise as
///   7-bit. The LSB is used once (Syrp's choice), and selecting a parameter forgets it.
/// - The parameter stays selected for further data entry until another is chosen. The null
///   parameter 127/127 (Syrp's choice: for NRPN too) deselects, so data entry is ignored.
/// - Data increment and decrement (controllers 96, 97) and other controllers are ignored.
///
/// Allocation-free and `Copy`-sized: usable on the audio thread.
/// ```
/// let mut d = syrp_midi_utils::rpn::MidiRpnDetector::new();
/// assert_eq!(d.try_parse(2, 101, 0), None);
/// assert_eq!(d.try_parse(2, 100, 0), None);
/// let m = d.try_parse(2, 6, 12).unwrap();
/// assert_eq!((m.channel(), m.parameter_number(), m.value(), m.is_14_bit()), (2, 0, 12, false));
/// ```
#[doc(alias = "MidiRPNDetector")]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MidiRpnDetector {
    channels: [ChannelState; 16],
}

impl MidiRpnDetector {
    /// A detector with nothing selected on any channel.
    /// ```
    /// assert_eq!(syrp_midi_utils::rpn::MidiRpnDetector::new().try_parse(1, 6, 1), None);
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Forgets every channel's selection and pending value.
    /// ```
    /// let mut d = syrp_midi_utils::rpn::MidiRpnDetector::new();
    /// d.try_parse(1, 101, 0);
    /// d.try_parse(1, 100, 0);
    /// d.reset();
    /// assert_eq!(d.try_parse(1, 6, 1), None);
    /// ```
    pub fn reset(&mut self) {
        self.channels = Default::default();
    }

    /// Feeds one control change (`channel` 1 to 16, `controller` and `value` 0 to 127) and
    /// returns the message it completes, if any. Out-of-range arguments are ignored.
    /// ```
    /// let mut d = syrp_midi_utils::rpn::MidiRpnDetector::new();
    /// for (c, v) in [(99, 1), (98, 2), (38, 3)] {
    ///     assert_eq!(d.try_parse(1, c, v), None);
    /// }
    /// let m = d.try_parse(1, 6, 4).unwrap();
    /// assert_eq!((m.is_nrpn(), m.parameter_number(), m.value()), (true, 130, 4 * 128 + 3));
    /// ```
    #[doc(alias = "tryParse")]
    #[doc(alias = "parseControllerMessage")]
    pub fn try_parse(&mut self, channel: u8, controller: u8, value: u8) -> Option<MidiRpnMessage> {
        if value > 0x7F {
            return None;
        }
        let state = self
            .channels
            .get_mut(usize::from(channel.checked_sub(1)?))?;
        match controller {
            NRPN_LSB | NRPN_MSB | RPN_LSB | RPN_MSB => {
                let is_nrpn = controller < RPN_LSB;
                if is_nrpn != state.is_nrpn {
                    *state = ChannelState {
                        is_nrpn,
                        ..ChannelState::default()
                    };
                }
                if controller == NRPN_MSB || controller == RPN_MSB {
                    state.parameter_msb = Some(value);
                } else {
                    state.parameter_lsb = Some(value);
                }
                state.value_lsb = None;
                None
            }
            DATA_LSB => {
                state.parameter()?;
                state.value_lsb = Some(value);
                None
            }
            DATA_MSB => {
                let parameter_number = state.parameter()?;
                let lsb = state.value_lsb.take();
                let value = lsb.map_or(u16::from(value), |lsb| {
                    u16::from(value) << 7 | u16::from(lsb)
                });
                Some(MidiRpnMessage {
                    channel,
                    parameter_number,
                    value,
                    is_nrpn: state.is_nrpn,
                    is_14_bit: lsb.is_some(),
                })
            }
            _ => None,
        }
    }
}

/// Writes RPN and NRPN messages as control changes, in the role of JUCE's
/// `MidiRPNGenerator`.
/// ```
/// use syrp_midi::MidiData;
/// let msgs = syrp_midi_utils::rpn::MidiRpnGenerator::generate(1, 0, 2, false, false)?;
/// let ccs: Vec<_> = msgs.iter().map(|m| m.bytes().to_vec()).collect();
/// assert_eq!(ccs, [[0xB0, 101, 0], [0xB0, 100, 0], [0xB0, 6, 2], [0xB0, 38, 0]]);
/// # Ok::<(), syrp_midi::MidiError>(())
/// ```
#[doc(alias = "MidiRPNGenerator")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MidiRpnGenerator;

impl MidiRpnGenerator {
    /// The four control changes that set `parameter_number` (0 to 16383) of `channel` (1 to
    /// 16) to `value`: the parameter MSB and LSB (101 and 100, or 99 and 98 for an NRPN), then
    /// the value. A 14-bit `value` (0 to 16383) is sent LSB first (38, then 6) so that
    /// [`MidiRpnDetector`] reads it as 14-bit. A 7-bit `value` (0 to 127) is sent as 6, then
    /// 38 with 0: a receiver that combines them gets the same coarse value, and the detector
    /// reports the 7-bit message at controller 6 (Syrp's choice of the fourth message).
    /// Anything out of range is [`MidiError::ValueOutOfRange`]. Parameter 16383 (127/127) is
    /// the null parameter: its messages deselect rather than set a value.
    /// ```
    /// use syrp_midi::MidiData;
    /// let msgs = syrp_midi_utils::rpn::MidiRpnGenerator::generate(16, 0x3FFF, 0x3FFF, true, true)?;
    /// assert_eq!(msgs[0].bytes(), &[0xBF, 99, 0x7F]);
    /// assert_eq!(msgs[2].bytes(), &[0xBF, 38, 0x7F]);
    /// # Ok::<(), syrp_midi::MidiError>(())
    /// ```
    pub fn generate(
        channel: u8,
        parameter_number: u16,
        value: u16,
        is_nrpn: bool,
        use_14_bit_value: bool,
    ) -> Result<[MidiMessage; 4], MidiError> {
        let message =
            MidiRpnMessage::new(channel, parameter_number, value, is_nrpn, use_14_bit_value)?;
        Ok(Self::generate_message(message))
    }

    /// The four control changes for `message`, as [`generate`](Self::generate) writes them.
    /// ```
    /// use syrp_midi_utils::rpn::{MidiRpnGenerator, MidiRpnMessage};
    /// let m = MidiRpnMessage::new(1, 0, 2, false, false)?;
    /// assert_eq!(MidiRpnGenerator::generate_message(m), MidiRpnGenerator::generate(1, 0, 2, false, false)?);
    /// # Ok::<(), syrp_midi::MidiError>(())
    /// ```
    #[must_use]
    pub fn generate_message(message: MidiRpnMessage) -> [MidiMessage; 4] {
        let MidiRpnMessage {
            channel,
            parameter_number,
            value,
            is_nrpn,
            is_14_bit,
        } = message;
        let (msb_cc, lsb_cc) = if is_nrpn {
            (NRPN_MSB, NRPN_LSB)
        } else {
            (RPN_MSB, RPN_LSB)
        };
        let high = |v: u16| (v >> 7) as u8;
        let low = |v: u16| (v & 0x7F) as u8;
        // Every field was range-checked by `MidiRpnMessage::new`, so each byte is 7-bit.
        let cc = |number, v| {
            MidiMessage::controller(channel, number, v).expect("checked 7-bit controller")
        };
        let (third, fourth) = if is_14_bit {
            (cc(DATA_LSB, low(value)), cc(DATA_MSB, high(value)))
        } else {
            (cc(DATA_MSB, low(value)), cc(DATA_LSB, 0))
        };
        [
            cc(msb_cc, high(parameter_number)),
            cc(lsb_cc, low(parameter_number)),
            third,
            fourth,
        ]
    }
}

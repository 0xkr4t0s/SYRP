// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Tests for work/specs/WP-2.1-07.md, increment 4: B4 (RPN detector and generator).

use proptest::prelude::*;
use syrp_midi::{MidiData, MidiError, MidiMessage};
use syrp_midi_utils::rpn::{MidiRpnDetector, MidiRpnGenerator, MidiRpnMessage};

#[global_allocator]
static ALLOC: syrp_rt::GuardedAllocator = syrp_rt::GuardedAllocator;

/// Feeds control changes and collects every completed message.
fn feed(d: &mut MidiRpnDetector, channel: u8, ccs: &[(u8, u8)]) -> Vec<MidiRpnMessage> {
    ccs.iter()
        .filter_map(|(c, v)| d.try_parse(channel, *c, *v))
        .collect()
}

fn feed_messages(d: &mut MidiRpnDetector, messages: &[MidiMessage]) -> Vec<MidiRpnMessage> {
    messages
        .iter()
        .filter_map(|m| d.try_parse(m.channel()?, m.controller_number()?, m.controller_value()?))
        .collect()
}

#[test]
fn b4_parameter_bytes_in_either_order_then_value() {
    let mut d = MidiRpnDetector::new();
    let got = feed(&mut d, 3, &[(100, 5), (101, 1), (6, 64)]);
    assert_eq!(
        got,
        [MidiRpnMessage::new(3, 133, 64, false, false).unwrap()]
    );
    // The parameter stays selected for more data entry.
    assert_eq!(feed(&mut d, 3, &[(6, 65)])[0].value(), 65);
}

#[test]
fn b4_value_lsb_before_msb_is_14_bit_and_used_once() {
    let mut d = MidiRpnDetector::new();
    let got = feed(&mut d, 1, &[(99, 0), (98, 7), (38, 1), (6, 2), (6, 3)]);
    assert_eq!(
        got[0],
        MidiRpnMessage::new(1, 7, 2 * 128 + 1, true, true).unwrap()
    );
    assert_eq!(got[1], MidiRpnMessage::new(1, 7, 3, true, false).unwrap());
    // An LSB after the MSB does not complete a message; it waits for the next MSB.
    assert!(feed(&mut d, 1, &[(38, 9)]).is_empty());
}

#[test]
fn b4_incomplete_or_null_parameters_ignore_data_entry() {
    let mut d = MidiRpnDetector::new();
    assert!(feed(&mut d, 1, &[(6, 1), (101, 0), (38, 1), (6, 1)]).is_empty());
    assert!(!feed(&mut d, 1, &[(100, 0), (6, 1)]).is_empty());
    assert!(feed(&mut d, 1, &[(101, 127), (100, 127), (38, 1), (6, 1)]).is_empty());
    assert!(feed(&mut d, 1, &[(99, 127), (98, 127), (6, 1)]).is_empty());
    // So a generated message for the null parameter reads back as nothing.
    let null = MidiRpnGenerator::generate(1, 0x3FFF, 5, false, false).unwrap();
    assert!(feed_messages(&mut d, &null).is_empty());
}

#[test]
fn b4_switching_between_rpn_and_nrpn_forgets_the_other_byte() {
    let mut d = MidiRpnDetector::new();
    assert!(feed(&mut d, 1, &[(101, 0), (98, 4), (6, 1)]).is_empty());
    let got = feed(&mut d, 1, &[(99, 0), (6, 1)]);
    assert_eq!((got[0].is_nrpn(), got[0].parameter_number()), (true, 4));
}

#[test]
fn b4_channels_are_independent_and_reset_clears_them() {
    let mut d = MidiRpnDetector::new();
    feed(&mut d, 1, &[(101, 0), (100, 0)]);
    assert!(feed(&mut d, 2, &[(6, 1)]).is_empty());
    assert_eq!(feed(&mut d, 1, &[(6, 1)]).len(), 1);
    d.reset();
    assert!(feed(&mut d, 1, &[(6, 1)]).is_empty());
}

#[test]
fn b4_out_of_range_input_is_ignored_and_other_controllers_too() {
    let mut d = MidiRpnDetector::new();
    feed(&mut d, 1, &[(101, 0), (100, 0)]);
    assert_eq!(d.try_parse(0, 6, 1), None);
    assert_eq!(d.try_parse(17, 6, 1), None);
    assert_eq!(d.try_parse(1, 6, 128), None);
    assert!(feed(&mut d, 1, &[(96, 1), (97, 1), (7, 100)]).is_empty());
}

#[test]
fn b4_generator_writes_the_documented_controller_order() {
    let bytes = |ms: [MidiMessage; 4]| ms.iter().map(|m| m.bytes().to_vec()).collect::<Vec<_>>();
    let fourteen = MidiRpnGenerator::generate(2, 0x81, 0x3F80 | 5, false, true).unwrap();
    assert_eq!(
        bytes(fourteen),
        [
            [0xB1, 101, 1],
            [0xB1, 100, 1],
            [0xB1, 38, 5],
            [0xB1, 6, 0x7F]
        ]
    );
    let seven = MidiRpnGenerator::generate(1, 2, 127, true, false).unwrap();
    assert_eq!(
        bytes(seven),
        [[0xB0, 99, 0], [0xB0, 98, 2], [0xB0, 6, 127], [0xB0, 38, 0]]
    );
    for (c, p, v, wide) in [
        (0, 0, 0, false),
        (17, 0, 0, false),
        (1, 0x4000, 0, true),
        (1, 0, 128, false),
        (1, 0, 0x4000, true),
    ] {
        assert_eq!(
            MidiRpnGenerator::generate(c, p, v, false, wide),
            Err(MidiError::ValueOutOfRange)
        );
    }
}

#[test]
fn b4_detector_never_allocates() {
    let mut d = MidiRpnDetector::new();
    let found = syrp_rt::no_alloc(|| {
        let mut n = 0;
        for (c, v) in [(101, 0), (100, 0), (38, 1), (6, 2), (6, 3)] {
            n += usize::from(d.try_parse(1, c, v).is_some());
        }
        n
    });
    assert_eq!(found, 2);
}

/// Any RPN or NRPN except the null parameter 16383 (127/127), which the detector ignores.
fn rpn() -> impl Strategy<Value = MidiRpnMessage> {
    (1_u8..=16, 0_u16..0x3FFF, any::<bool>(), any::<bool>()).prop_flat_map(|(c, p, nrpn, wide)| {
        let max: u16 = if wide { 0x3FFF } else { 0x7F };
        (0_u16..=max).prop_map(move |v| MidiRpnMessage::new(c, p, v, nrpn, wide).unwrap())
    })
}

proptest! {
    #[test]
    fn b4_generated_messages_parse_back_to_the_same_rpn(msgs in prop::collection::vec(rpn(), 1..8)) {
        let mut d = MidiRpnDetector::new();
        for m in msgs {
            let got = feed_messages(&mut d, &MidiRpnGenerator::generate_message(m));
            prop_assert_eq!(got, vec![m]);
        }
    }

    #[test]
    fn b4_random_controller_streams_never_panic(ccs in prop::collection::vec((0_u8..20, any::<u8>(), any::<u8>()), 0..200)) {
        let mut d = MidiRpnDetector::new();
        for (ch, c, v) in ccs {
            if let Some(m) = d.try_parse(ch, c, v) {
                prop_assert!(MidiRpnMessage::new(m.channel(), m.parameter_number(), m.value(), m.is_nrpn(), m.is_14_bit()).is_ok());
            }
        }
    }
}

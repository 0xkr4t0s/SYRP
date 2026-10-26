// SPDX-License-Identifier: MIT OR Apache-2.0
//! Behaviour tests for byte stream <-> UMP conversion (WP-0.2-03 increment 4: B4, and B5).

use proptest::prelude::*;
use syrp_midi::SysExStatus::{Complete, Continue, End, Start};
use syrp_midi::{BytesToUmp, MidiData, MidiMessage, UmpMessageType, UmpPacket as P};
use syrp_rt::{GuardedAllocator, RtSafe, no_alloc};

#[global_allocator]
static ALLOC: GuardedAllocator = GuardedAllocator;

fn decode(group: u8, bytes: &[u8]) -> Vec<P> {
    let mut out = Vec::new();
    BytesToUmp::new(group).unwrap().feed(bytes, |p| out.push(p));
    out
}

fn to_bytes(packets: &[P]) -> Vec<u8> {
    packets
        .iter()
        .flat_map(|p| p.midi1_bytes().unwrap())
        .collect()
}

fn cv(status: u8, d1: u8, d2: u8) -> P {
    P::midi1_channel_voice(0, status, d1, d2).unwrap()
}

#[test]
fn b4_channel_messages_and_running_status() {
    let got = decode(0, &[0x90, 60, 100, 62, 0, 0xC1, 5, 6, 0xE0, 0, 0x40]);
    let want = [
        cv(0x90, 60, 100),
        cv(0x90, 62, 0),
        cv(0xC1, 5, 0),
        cv(0xC1, 6, 0),
        cv(0xE0, 0, 0x40),
    ];
    assert_eq!(got, want);
    // Back to bytes, running status expanded.
    let bytes = [0x90, 60, 100, 0x90, 62, 0, 0xC1, 5, 0xC1, 6, 0xE0, 0, 0x40];
    assert_eq!(to_bytes(&got), bytes);
    assert_eq!(decode(7, &[0xB2, 7, 99])[0].words(), &[0x27B2_0763]);
}

#[test]
fn b4_realtime_bytes_interleave_anywhere() {
    let clock = P::system(0, 0xF8, 0, 0).unwrap();
    let got = decode(0, &[0x90, 0xF8, 60, 0xFE, 100, 0xF8, 61, 1]);
    let sense = P::system(0, 0xFE, 0, 0).unwrap();
    assert_eq!(
        got,
        [clock, sense, cv(0x90, 60, 100), clock, cv(0x90, 61, 1)]
    );
    // Inside a SysEx too, without ending it.
    let got = decode(0, &[0xF0, 1, 0xF8, 2, 0xF7]);
    assert_eq!(got, [clock, P::sysex7(0, Complete, &[1, 2]).unwrap()]);
    // Undefined real-time bytes are dropped.
    assert!(decode(0, &[0xF9, 0xFD]).is_empty());
}

#[test]
fn b4_system_common_cancels_running_status_and_strays_are_dropped() {
    let got = decode(
        0,
        &[
            0x90, 60, 1, 0xF2, 0x10, 0x20, 61, 1, 0xF1, 0x33, 0xF6, 0xF3, 4,
        ],
    );
    let sys = |s, a, b| P::system(0, s, a, b).unwrap();
    let want = [
        cv(0x90, 60, 1),
        sys(0xF2, 0x10, 0x20),
        sys(0xF1, 0x33, 0),
        sys(0xF6, 0, 0),
        sys(0xF3, 4, 0),
    ];
    assert_eq!(got, want);
    assert_eq!(
        to_bytes(&got),
        [0x90, 60, 1, 0xF2, 0x10, 0x20, 0xF1, 0x33, 0xF6, 0xF3, 4]
    );
    // Data with no status, a stray end of SysEx, undefined system common: all dropped.
    assert!(decode(0, &[1, 2, 0xF7, 0xF4, 3, 0xF5]).is_empty());
    // A new status abandons a partial message.
    assert_eq!(decode(0, &[0x90, 60, 0x80, 60, 0]), [cv(0x80, 60, 0)]);
}

#[test]
fn b4_sysex_is_split_into_data64_packets_and_reassembled() {
    let s7 = |st, d: &[u8]| P::sysex7(0, st, d).unwrap();
    let msg = |n: u8| -> Vec<u8> { [0xF0].into_iter().chain(1..=n).chain([0xF7]).collect() };
    assert_eq!(decode(0, &msg(0)), [s7(Complete, &[])]);
    assert_eq!(decode(0, &msg(6)), [s7(Complete, &[1, 2, 3, 4, 5, 6])]);
    assert_eq!(
        decode(0, &msg(12)),
        [
            s7(Start, &[1, 2, 3, 4, 5, 6]),
            s7(End, &[7, 8, 9, 10, 11, 12])
        ]
    );
    let got = decode(0, &msg(14));
    let statuses: Vec<_> = got
        .iter()
        .map(|p| (p.sysex_status(), p.sysex_bytes().unwrap().len()))
        .collect();
    assert_eq!(
        statuses,
        [(Some(Start), 6), (Some(Continue), 6), (Some(End), 2)]
    );
    for n in [0, 5, 6, 7, 13, 40] {
        assert_eq!(to_bytes(&decode(0, &msg(n))), msg(n), "{n} bytes");
    }
    // A status byte ends an unterminated SysEx, then starts its own message.
    let got = decode(0, &[0xF0, 1, 0xF6, 0xF0, 2, 0x90, 60, 1]);
    let tune = P::system(0, 0xF6, 0, 0).unwrap();
    assert_eq!(
        got,
        [
            s7(Complete, &[1]),
            tune,
            s7(Complete, &[2]),
            cv(0x90, 60, 1)
        ]
    );
}

#[test]
fn b4_only_byte_stream_types_have_midi1_bytes_and_decoding_never_allocates() {
    assert!(P::noop().midi1_bytes().is_none());
    assert!(
        P::midi2_note_on(0, 1, 60, 1, 0, 0)
            .unwrap()
            .midi1_bytes()
            .is_none()
    );
    assert!(
        P::sysex8(0, Complete, 0, &[1])
            .unwrap()
            .midi1_bytes()
            .is_none()
    );
    let reset = P::system(0, 0xFF, 0, 0).unwrap();
    assert_eq!(reset.midi1_bytes().unwrap().collect::<Vec<_>>(), [0xFF]);
    let cont = P::sysex7(0, Continue, &[9]).unwrap();
    assert_eq!(cont.midi1_bytes().unwrap().collect::<Vec<_>>(), [9]);

    fn rt_safe<T: RtSafe>(_: &T) {}
    let mut dec = BytesToUmp::new(1).unwrap();
    rt_safe(&dec);
    let mut n = 0;
    no_alloc(|| {
        dec.feed(
            &[0xF0, 1, 2, 3, 4, 5, 6, 7, 0xF7, 0x91, 60, 1, 61, 1],
            |p| {
                n += p.midi1_bytes().map_or(0, |b| b.len());
            },
        );
    });
    assert_eq!(n, 15); // F0 + 6, then 1 + F7, then two expanded 3-byte note ons
    assert!(BytesToUmp::new(16).is_none());
}

/// Any well-formed MIDI 1.0 message, as bytes.
fn message() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![
        (0x80_u8..0xF0, 0_u8..128, 0_u8..128).prop_map(|(s, a, b)| {
            MidiMessage::from_bytes(&[s, a, b])
                .or_else(|_| MidiMessage::from_bytes(&[s, a]))
                .unwrap()
                .bytes()
                .to_vec()
        }),
        proptest::collection::vec(0_u8..128, 0..20)
            .prop_map(|d| MidiMessage::sysex(&d).unwrap().bytes().to_vec()),
        prop_oneof![Just(0xF6_u8), Just(0xF8), Just(0xFA), Just(0xFE)].prop_map(|s| vec![s]),
        (0_u8..128, 0_u8..128).prop_map(|(a, b)| vec![0xF2, a, b]),
    ]
}

proptest! {
    #[test]
    fn b5_decoder_never_panics_and_emits_valid_packets(
        bytes in proptest::collection::vec(any::<u8>(), 0..64),
        g in 0_u8..16,
    ) {
        for p in decode(g, &bytes) {
            prop_assert_eq!(P::read(p.words()).unwrap().0, p);
            prop_assert_eq!(p.group(), Some(g));
            prop_assert!(p.midi1_bytes().is_some());
        }
    }

    #[test]
    fn b5_messages_round_trip_through_ump(msgs in proptest::collection::vec(message(), 0..12)) {
        let bytes: Vec<u8> = msgs.concat();
        let packets = decode(0, &bytes);
        prop_assert!(packets.iter().all(|p| p.message_type() != UmpMessageType::Utility));
        prop_assert_eq!(to_bytes(&packets), bytes);
    }

    #[test]
    fn b5_midi1_bytes_never_panics_on_random_packets(words in proptest::collection::vec(any::<u32>(), 1..5)) {
        if let Ok((p, _)) = P::read(&words)
            && let Some(b) = p.midi1_bytes()
        {
            let v: Vec<u8> = b.collect();
            prop_assert!(!v.is_empty() && v.len() <= 8);
        }
    }
}

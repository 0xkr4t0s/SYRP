// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Tests for work/specs/WP-2.1-07.md, increment 2: B2 (`MidiMessageSequence`).

use std::collections::BTreeSet;

use proptest::prelude::*;
use syrp_error::Diagnostic;
use syrp_midi::{MidiData, MidiMessage};
use syrp_midi_utils::{MidiMessageSequence, SequenceError};

fn on(ch: u8, note: u8, t: f64) -> MidiMessage {
    MidiMessage::note_on(ch, note, 100)
        .unwrap()
        .with_timestamp(t)
}
fn off(ch: u8, note: u8, t: f64) -> MidiMessage {
    MidiMessage::note_off(ch, note, 0)
        .unwrap()
        .with_timestamp(t)
}
fn cc(ch: u8, n: u8, v: u8, t: f64) -> MidiMessage {
    MidiMessage::controller(ch, n, v).unwrap().with_timestamp(t)
}
fn seq(messages: &[MidiMessage]) -> MidiMessageSequence {
    let mut s = MidiMessageSequence::new();
    for m in messages {
        s.add_event(m.clone(), 0.0).unwrap();
    }
    s
}
fn times(s: &MidiMessageSequence) -> Vec<f64> {
    s.iter().map(|e| e.time()).collect()
}
fn pairs(s: &MidiMessageSequence) -> Vec<Option<usize>> {
    s.iter().map(|e| e.matching_note_off()).collect()
}

#[test]
fn b2_add_event_keeps_time_order_and_insertion_order_at_equal_times() {
    let mut s = seq(&[cc(1, 1, 0, 2.0), cc(1, 2, 0, 1.0), cc(1, 3, 0, 1.0)]);
    assert_eq!(s.add_event(cc(1, 4, 0, 0.0), 1.0).unwrap(), 2);
    assert_eq!(times(&s), [1.0, 1.0, 1.0, 2.0]);
    let numbers: Vec<_> = s
        .iter()
        .map(|e| e.message().controller_number().unwrap())
        .collect();
    assert_eq!(numbers, [2, 3, 4, 1]);
}

#[test]
fn b2_add_event_shifts_matched_indices() {
    let mut s = seq(&[on(1, 60, 0.0), off(1, 60, 2.0)]);
    s.update_matched_pairs();
    s.add_event(cc(1, 7, 1, 1.0), 0.0).unwrap();
    s.add_event(cc(1, 7, 1, -1.0), 0.0).unwrap();
    assert_eq!(pairs(&s), [None, Some(3), None, None]);
    assert!(s.event(3).unwrap().message().is_note_off());
    assert_eq!(s.time_of_matching_key_up(1), Some(2.0));
}

#[test]
fn b2_delete_event_remaps_pairs_and_optionally_deletes_note_up() {
    let mut s = seq(&[
        cc(1, 1, 0, 0.0),
        on(1, 60, 1.0),
        on(1, 62, 1.5),
        off(1, 60, 2.0),
        off(1, 62, 3.0),
    ]);
    s.update_matched_pairs();
    assert_eq!(pairs(&s), [None, Some(3), Some(4), None, None]);
    s.delete_event(0, true).unwrap();
    assert_eq!(pairs(&s), [Some(2), Some(3), None, None]);
    s.delete_event(0, true).unwrap();
    assert_eq!(pairs(&s), [Some(1), None]);
    s.delete_event(1, false).unwrap();
    assert_eq!(pairs(&s), [None]);
    let err = s.delete_event(1, false).unwrap_err();
    assert_eq!(
        (err, err.code().as_str()),
        (SequenceError::IndexOutOfRange, "SYRP-MIDI-0005")
    );
}

#[test]
fn b2_update_matched_pairs_by_channel_and_note() {
    let mut s = seq(&[
        on(1, 60, 0.0),
        on(2, 60, 0.0),
        MidiMessage::note_on(2, 60, 0).unwrap().with_timestamp(1.0),
        off(1, 61, 1.0),
        off(1, 60, 2.0),
        on(1, 64, 3.0),
    ]);
    s.update_matched_pairs();
    assert_eq!(pairs(&s), [Some(4), Some(2), None, None, None, None]);
    assert_eq!(s.index_of_matching_key_up(5), None);
}

#[test]
fn b2_update_matched_pairs_overlapping_notes_pair_first_in_first_out_syrp_choice() {
    let mut s = seq(&[
        on(1, 60, 0.0),
        on(1, 60, 1.0),
        off(1, 60, 2.0),
        off(1, 60, 3.0),
    ]);
    s.update_matched_pairs();
    assert_eq!(pairs(&s), [Some(2), Some(3), None, None]);
}

#[test]
fn b2_times_and_next_index_at_time() {
    let s = seq(&[
        cc(1, 1, 0, 1.0),
        cc(1, 1, 0, 2.0),
        cc(1, 1, 0, 2.0),
        cc(1, 1, 0, 4.0),
    ]);
    assert_eq!(
        (s.start_time(), s.end_time(), s.event_time(1)),
        (Some(1.0), Some(4.0), Some(2.0))
    );
    let next: Vec<_> = [0.0, 1.0, 1.5, 2.0, 4.0, 5.0]
        .iter()
        .map(|t| s.next_index_at_time(*t))
        .collect();
    assert_eq!(next, [0, 0, 1, 1, 3, 4]);
    assert_eq!(MidiMessageSequence::new().next_index_at_time(0.0), 0);
}

#[test]
fn b2_add_sequence_merges_in_time_order_and_keeps_pairs() {
    let mut a = seq(&[cc(1, 1, 0, 1.0), cc(1, 1, 0, 5.0)]);
    let mut b = seq(&[on(1, 60, 0.0), off(1, 60, 4.0)]);
    b.update_matched_pairs();
    a.add_sequence(&b, 1.0).unwrap();
    assert_eq!(times(&a), [1.0, 1.0, 5.0, 5.0]);
    // At equal times the existing event comes first.
    assert!(a.event(0).unwrap().message().controller_number().is_some());
    assert_eq!(pairs(&a), [None, Some(3), None, None]);
}

#[test]
fn b2_add_sequence_range_filters_on_adjusted_time() {
    let mut b = seq(&[
        on(1, 60, 0.0),
        off(1, 60, 1.0),
        on(1, 61, 1.0),
        off(1, 61, 2.0),
    ]);
    b.update_matched_pairs();
    let mut a = MidiMessageSequence::new();
    a.add_sequence_range(&b, 10.0, 10.0, 12.0).unwrap();
    assert_eq!(times(&a), [10.0, 11.0, 11.0]);
    // The note off at 12.0 is outside the range, so its note on keeps no match.
    assert_eq!(pairs(&a), [Some(1), None, None]);
}

#[test]
fn b2_sort_is_stable_and_remaps_pairs() {
    let mut s = seq(&[
        on(1, 60, 0.0),
        cc(1, 1, 0, 1.0),
        cc(1, 2, 0, 1.0),
        off(1, 60, 2.0),
    ]);
    s.update_matched_pairs();
    s.message_mut(3).unwrap().set_timestamp(0.5);
    s.message_mut(0).unwrap().set_timestamp(1.0);
    s.sort();
    let numbers: Vec<_> = s.iter().map(|e| e.message().controller_number()).collect();
    assert_eq!(numbers, [None, None, Some(1), Some(2)]);
    assert!(s.event(0).unwrap().message().is_note_off());
    assert_eq!(pairs(&s), [None, Some(0), None, None]);
}

#[test]
fn b2_extract_and_delete_channel_and_sysex_messages() {
    let tempo = syrp_midi_utils::message::tempo_meta_event(500_000).unwrap();
    let sx = MidiMessage::sysex(&[0x7E, 1]).unwrap().with_timestamp(0.5);
    let mut s = seq(&[on(2, 60, 0.0), tempo, sx, cc(3, 1, 1, 1.0), off(2, 60, 2.0)]);
    s.update_matched_pairs();
    let mut two = MidiMessageSequence::new();
    s.extract_midi_channel_messages(2, &mut two, false).unwrap();
    assert_eq!((two.len(), pairs(&two)), (2, vec![Some(1), None]));
    let mut with_meta = MidiMessageSequence::new();
    s.extract_midi_channel_messages(2, &mut with_meta, true)
        .unwrap();
    assert_eq!(with_meta.len(), 3);
    let mut sysex = MidiMessageSequence::new();
    s.extract_sys_ex_messages(&mut sysex).unwrap();
    assert_eq!(
        sysex.event(0).unwrap().message().sysex_data(),
        Some(&[0x7E, 1][..])
    );
    assert_eq!(
        s.extract_midi_channel_messages(0, &mut two, false),
        Err(SequenceError::ValueOutOfRange)
    );
    s.delete_sys_ex_messages();
    s.delete_midi_channel_messages(3).unwrap();
    assert_eq!(s.len(), 3);
    assert_eq!(pairs(&s), [Some(2), None, None]);
    s.delete_midi_channel_messages(2).unwrap();
    assert_eq!(s.len(), 1);
    assert_eq!(
        s.delete_midi_channel_messages(17),
        Err(SequenceError::ValueOutOfRange)
    );
}

#[test]
fn b2_controller_updates_rebuild_state_before_time_syrp_choice() {
    let pc = |p: u8, t: f64| MidiMessage::program_change(1, p).unwrap().with_timestamp(t);
    let bend = |v: u16, t: f64| MidiMessage::pitch_bend(1, v).unwrap().with_timestamp(t);
    let s = seq(&[
        cc(1, 0, 1, 0.0),
        pc(5, 0.1),
        cc(1, 0, 2, 0.2),
        cc(1, 32, 3, 0.3),
        pc(6, 0.4),
        cc(1, 0, 4, 0.5),
        cc(1, 7, 10, 0.6),
        cc(1, 101, 0, 0.7),
        cc(1, 100, 0, 0.8),
        cc(1, 6, 12, 0.9),
        cc(1, 7, 20, 1.0),
        cc(1, 1, 30, 1.1),
        bend(100, 1.2),
        bend(200, 1.3),
        cc(1, 123, 0, 1.4),
        cc(2, 7, 99, 1.5),
        cc(1, 7, 127, 9.0),
    ]);
    let out = s.create_controller_updates_for_time(1, 9.0).unwrap();
    let bytes: Vec<&[u8]> = out.iter().map(MidiData::bytes).collect();
    let expected: [&[u8]; 10] = [
        &[0xB0, 0, 2],
        &[0xB0, 32, 3],
        &[0xC0, 6],
        &[0xB0, 0, 4],
        &[0xB0, 1, 30],
        &[0xB0, 7, 20],
        &[0xB0, 101, 0],
        &[0xB0, 100, 0],
        &[0xB0, 6, 12],
        &[0xE0, 200 & 0x7F, 200 >> 7],
    ];
    assert_eq!(bytes, expected);
    assert!(out.iter().all(|m| m.timestamp() == 9.0));
    assert!(
        s.create_controller_updates_for_time(1, 0.0)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        s.create_controller_updates_for_time(17, 0.0),
        Err(SequenceError::ValueOutOfRange)
    );
}

/// Edits applied to a sequence of identifiable notes.
#[derive(Debug, Clone)]
enum Op {
    Add(u8, u8),
    Delete(usize, bool),
    Retime(usize, u8),
    Merge(u8),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (0_u8..10, 0_u8..128).prop_map(|(t, v)| Op::Add(t, v)),
        (0_usize..80, any::<bool>()).prop_map(|(i, d)| Op::Delete(i, d)),
        (0_usize..80, 0_u8..10).prop_map(|(i, t)| Op::Retime(i, t)),
        (0_u8..10).prop_map(Op::Merge),
    ]
}

/// A note event's identity: (is note on, channel, velocity). Velocities are unique per kind.
type Id = (bool, u8, u8);

fn id(m: &MidiMessage) -> Option<Id> {
    let (ch, vel) = (m.channel()?, m.velocity()?);
    Some((m.is_note_on(), ch, vel))
}

fn pair_ids(s: &MidiMessageSequence) -> BTreeSet<(Id, Id)> {
    s.iter()
        .filter_map(|e| {
            Some((
                id(e.message())?,
                id(s.event(e.matching_note_off()?)?.message())?,
            ))
        })
        .collect()
}

proptest! {
    #[test]
    fn b2_every_edit_keeps_order_and_pairs(
        notes in prop::collection::vec((0_u8..3, 0_u8..10, any::<bool>()), 0..60),
        ops in prop::collection::vec(op(), 0..20),
    ) {
        // Channel 1 events only; ids use velocity 1 + position, unique per kind.
        let msgs: Vec<MidiMessage> = notes.iter().enumerate().map(|(i, (n, t, is_on))| {
            let vel = u8::try_from(i + 1).unwrap();
            let make = if *is_on { MidiMessage::note_on } else { MidiMessage::note_off };
            make(1, 60 + n, vel).unwrap().with_timestamp(f64::from(*t))
        }).collect();
        let mut s = seq(&msgs);
        s.update_matched_pairs();
        let mut expected = pair_ids(&s);
        for op in ops {
            match op {
                Op::Add(t, v) => { s.add_event(cc(1, 1, v, f64::from(t)), 0.0).unwrap(); }
                Op::Delete(i, both) => {
                    let removed: Vec<_> = [Some(i), s.index_of_matching_key_up(i).filter(|_| both)]
                        .into_iter().flatten().filter_map(|k| id(s.event(k)?.message())).collect();
                    let existed = i < s.len();
                    prop_assert_eq!(s.delete_event(i, both).is_ok(), existed);
                    expected.retain(|(a, b)| !removed.contains(a) && !removed.contains(b));
                }
                Op::Retime(i, t) => {
                    if let Some(m) = s.message_mut(i) { m.set_timestamp(f64::from(t)); }
                    s.sort();
                }
                Op::Merge(t) => {
                    let other = seq(&[cc(1, 2, 0, f64::from(t))]);
                    s.add_sequence(&other, 0.5).unwrap();
                }
            }
            let ts = times(&s);
            prop_assert!(ts.windows(2).all(|w| w[0] <= w[1]));
            for (k, e) in s.iter().enumerate() {
                if let Some(o) = e.matching_note_off() {
                    prop_assert!(o < s.len() && o != k);
                    prop_assert!(s.event(o).unwrap().message().is_note_off());
                }
            }
            prop_assert_eq!(pair_ids(&s), expected.clone());
        }
    }
}

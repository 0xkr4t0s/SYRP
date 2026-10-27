// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`MidiMessageSequence`]: a time-ordered list of [`MidiEventHolder`]s, each a
//! [`MidiMessage`] plus the index of its matching note off.
//!
//! Matching pairs are indices, never pointers, and every edit (insertion, deletion, merging,
//! sorting) remaps them so they stay correct. Timestamps are compared with
//! [`f64::total_cmp`], so the order is total (a NaN sorts after every number).
//!
//! ```
//! use syrp_midi::MidiMessage;
//! use syrp_midi_utils::sequence::MidiMessageSequence;
//! let mut seq = MidiMessageSequence::new();
//! seq.add_event(MidiMessage::note_off(1, 60, 0)?.with_timestamp(1.0), 0.0)?;
//! seq.add_event(MidiMessage::note_on(1, 60, 100)?, 0.0)?;
//! seq.update_matched_pairs();
//! assert_eq!(seq.time_of_matching_key_up(0), Some(1.0));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::collections::HashMap;
use std::collections::VecDeque;
use std::fmt;

use syrp_error::{Diagnostic, ErrorCode};
use syrp_midi::{MidiData, MidiMessage, MidiMessageKind};

/// Errors from editing a [`MidiMessageSequence`].
/// ```
/// use syrp_error::Diagnostic;
/// let mut seq = syrp_midi_utils::sequence::MidiMessageSequence::new();
/// assert_eq!(seq.delete_event(0, false).unwrap_err().code().as_str(), "SYRP-MIDI-0005");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SequenceError {
    /// The event index is not below [`len`](MidiMessageSequence::len) (`SYRP-MIDI-0005`).
    IndexOutOfRange,
    /// A channel argument is outside 1 to 16 (`SYRP-MIDI-0002`).
    ValueOutOfRange,
    /// The allocator could not provide room for the new events (`SYRP-MIDI-0006`). The
    /// sequence is unchanged.
    OutOfMemory,
}

impl fmt::Display for SequenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            SequenceError::IndexOutOfRange => "MIDI sequence event index out of range",
            SequenceError::ValueOutOfRange => "MIDI value out of range",
            SequenceError::OutOfMemory => "out of memory for a MIDI sequence",
        })
    }
}

impl std::error::Error for SequenceError {}

impl Diagnostic for SequenceError {
    fn code(&self) -> ErrorCode {
        ErrorCode::new(match self {
            SequenceError::IndexOutOfRange => "SYRP-MIDI-0005",
            SequenceError::ValueOutOfRange => "SYRP-MIDI-0002",
            SequenceError::OutOfMemory => "SYRP-MIDI-0006",
        })
    }
}

/// Checks a channel numbered 1 to 16.
fn check_channel(channel: u8) -> Result<(), SequenceError> {
    if (1..=16).contains(&channel) {
        Ok(())
    } else {
        Err(SequenceError::ValueOutOfRange)
    }
}

/// One event of a [`MidiMessageSequence`]: a message (whose timestamp is the event time)
/// and, for a note on, the index of its matching note off.
/// ```
/// use syrp_midi::MidiMessage;
/// let mut seq = syrp_midi_utils::sequence::MidiMessageSequence::new();
/// seq.add_event(MidiMessage::note_on(1, 60, 1)?.with_timestamp(2.0), 0.5)?;
/// let e = seq.event(0).unwrap();
/// assert_eq!((e.time(), e.matching_note_off()), (2.5, None));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct MidiEventHolder {
    message: MidiMessage,
    note_off: Option<usize>,
}

impl MidiEventHolder {
    /// The message.
    /// ```
    /// use syrp_midi::{MidiData, MidiMessage};
    /// let mut seq = syrp_midi_utils::sequence::MidiMessageSequence::new();
    /// seq.add_event(MidiMessage::program_change(1, 4)?, 0.0)?;
    /// assert_eq!(seq.event(0).unwrap().message().program_number(), Some(4));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub fn message(&self) -> &MidiMessage {
        &self.message
    }

    /// The event time: the message's timestamp.
    /// ```
    /// let seq = syrp_midi_utils::sequence::MidiMessageSequence::new();
    /// assert!(seq.event(0).map(|e| e.time()).is_none());
    /// ```
    #[must_use]
    pub fn time(&self) -> f64 {
        self.message.timestamp()
    }

    /// The index of the matching note off, for a note on paired by
    /// [`update_matched_pairs`](MidiMessageSequence::update_matched_pairs).
    /// ```
    /// let seq = syrp_midi_utils::sequence::MidiMessageSequence::new();
    /// assert!(seq.iter().all(|e| e.matching_note_off().is_none()));
    /// ```
    #[doc(alias = "noteOffObject")]
    #[must_use]
    pub fn matching_note_off(&self) -> Option<usize> {
        self.note_off
    }
}

/// A time-ordered sequence of MIDI events, in the role of JUCE's `MidiMessageSequence`.
///
/// Events are kept sorted by time, equal times in insertion order (except after
/// [`message_mut`](Self::message_mut), until [`sort`](Self::sort)). Note ons can be paired
/// with their note offs by index ([`update_matched_pairs`](Self::update_matched_pairs)); every
/// edit keeps those indices pointing at the same events. Message thread only: edits allocate.
/// ```
/// use syrp_midi::MidiMessage;
/// use syrp_midi_utils::sequence::MidiMessageSequence;
/// let mut seq = MidiMessageSequence::new();
/// for (t, cc) in [(2.0, 1), (1.0, 2)] {
///     seq.add_event(MidiMessage::controller(1, cc, 0)?.with_timestamp(t), 0.0)?;
/// }
/// assert_eq!((seq.start_time(), seq.end_time()), (Some(1.0), Some(2.0)));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MidiMessageSequence {
    events: Vec<MidiEventHolder>,
}

impl MidiMessageSequence {
    /// An empty sequence.
    /// ```
    /// assert!(syrp_midi_utils::sequence::MidiMessageSequence::new().is_empty());
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The number of events.
    /// ```
    /// assert_eq!(syrp_midi_utils::sequence::MidiMessageSequence::new().len(), 0);
    /// ```
    #[doc(alias = "getNumEvents")]
    #[must_use]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// True when the sequence has no events.
    /// ```
    /// assert!(syrp_midi_utils::sequence::MidiMessageSequence::new().is_empty());
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Removes every event.
    /// ```
    /// let mut seq = syrp_midi_utils::sequence::MidiMessageSequence::new();
    /// seq.add_event(syrp_midi_utils::message::end_of_track(), 0.0)?;
    /// seq.clear();
    /// assert!(seq.is_empty());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn clear(&mut self) {
        self.events.clear();
    }

    /// The event at `index`.
    /// ```
    /// assert!(syrp_midi_utils::sequence::MidiMessageSequence::new().event(0).is_none());
    /// ```
    #[doc(alias = "getEventPointer")]
    #[must_use]
    pub fn event(&self, index: usize) -> Option<&MidiEventHolder> {
        self.events.get(index)
    }

    /// Mutable access to the message at `index`. After changing a timestamp, call
    /// [`sort`](Self::sort); after changing a note, call
    /// [`update_matched_pairs`](Self::update_matched_pairs).
    /// ```
    /// use syrp_midi::MidiMessage;
    /// let mut seq = syrp_midi_utils::sequence::MidiMessageSequence::new();
    /// seq.add_event(MidiMessage::controller(1, 1, 1)?, 0.0)?;
    /// seq.message_mut(0).unwrap().set_timestamp(4.0);
    /// assert_eq!(seq.event_time(0), Some(4.0));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub fn message_mut(&mut self, index: usize) -> Option<&mut MidiMessage> {
        self.events.get_mut(index).map(|e| &mut e.message)
    }

    /// Iterates over the events in order.
    /// ```
    /// let seq = syrp_midi_utils::sequence::MidiMessageSequence::new();
    /// assert_eq!(seq.iter().count(), 0);
    /// ```
    pub fn iter(&self) -> std::slice::Iter<'_, MidiEventHolder> {
        self.events.iter()
    }

    /// The time of the event at `index`.
    /// ```
    /// assert_eq!(syrp_midi_utils::sequence::MidiMessageSequence::new().event_time(0), None);
    /// ```
    #[doc(alias = "getEventTime")]
    #[must_use]
    pub fn event_time(&self, index: usize) -> Option<f64> {
        self.events.get(index).map(MidiEventHolder::time)
    }

    /// The time of the first event, or `None` when the sequence is empty.
    /// ```
    /// assert_eq!(syrp_midi_utils::sequence::MidiMessageSequence::new().start_time(), None);
    /// ```
    #[doc(alias = "getStartTime")]
    #[must_use]
    pub fn start_time(&self) -> Option<f64> {
        self.events.first().map(MidiEventHolder::time)
    }

    /// The time of the last event, or `None` when the sequence is empty.
    /// ```
    /// assert_eq!(syrp_midi_utils::sequence::MidiMessageSequence::new().end_time(), None);
    /// ```
    #[doc(alias = "getEndTime")]
    #[must_use]
    pub fn end_time(&self) -> Option<f64> {
        self.events.last().map(MidiEventHolder::time)
    }

    /// The index of the first event at or after `time`; [`len`](Self::len) when there is none.
    /// ```
    /// use syrp_midi::MidiMessage;
    /// let mut seq = syrp_midi_utils::sequence::MidiMessageSequence::new();
    /// seq.add_event(MidiMessage::controller(1, 1, 1)?.with_timestamp(1.0), 0.0)?;
    /// assert_eq!((seq.next_index_at_time(1.0), seq.next_index_at_time(1.5)), (0, 1));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[doc(alias = "getNextIndexAtTime")]
    #[must_use]
    pub fn next_index_at_time(&self, time: f64) -> usize {
        self.events
            .partition_point(|e| e.time().total_cmp(&time).is_lt())
    }

    /// The index of the note off matched to the note on at `index`.
    /// ```
    /// assert_eq!(syrp_midi_utils::sequence::MidiMessageSequence::new().index_of_matching_key_up(0), None);
    /// ```
    #[doc(alias = "getIndexOfMatchingKeyUp")]
    #[must_use]
    pub fn index_of_matching_key_up(&self, index: usize) -> Option<usize> {
        self.events.get(index)?.note_off
    }

    /// The time of the note off matched to the note on at `index`.
    /// ```
    /// assert_eq!(syrp_midi_utils::sequence::MidiMessageSequence::new().time_of_matching_key_up(0), None);
    /// ```
    #[doc(alias = "getTimeOfMatchingKeyUp")]
    #[must_use]
    pub fn time_of_matching_key_up(&self, index: usize) -> Option<f64> {
        self.event_time(self.index_of_matching_key_up(index)?)
    }

    /// Adds `message` at its timestamp plus `time_adjustment`, after any events at the same
    /// time, and returns its index. The new event has no matching note off until
    /// [`update_matched_pairs`](Self::update_matched_pairs); existing pairs are kept.
    /// ```
    /// use syrp_midi::MidiMessage;
    /// let mut seq = syrp_midi_utils::sequence::MidiMessageSequence::new();
    /// seq.add_event(MidiMessage::controller(1, 1, 1)?.with_timestamp(1.0), 0.0)?;
    /// assert_eq!(seq.add_event(MidiMessage::controller(1, 2, 1)?, 0.5)?, 0);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[doc(alias = "addEvent")]
    pub fn add_event(
        &mut self,
        message: MidiMessage,
        time_adjustment: f64,
    ) -> Result<usize, SequenceError> {
        self.events
            .try_reserve(1)
            .map_err(|_| SequenceError::OutOfMemory)?;
        let time = message.timestamp() + time_adjustment;
        let index = self
            .events
            .partition_point(|e| e.time().total_cmp(&time).is_le());
        for e in &mut self.events {
            if let Some(off) = e.note_off.as_mut().filter(|off| **off >= index) {
                *off += 1;
            }
        }
        let message = message.with_timestamp(time);
        self.events.insert(
            index,
            MidiEventHolder {
                message,
                note_off: None,
            },
        );
        Ok(index)
    }

    /// Deletes the event at `index` and, when `delete_matching_note_up` is true and it is a
    /// matched note on, its note off as well. A note on whose note off is deleted loses its
    /// match.
    /// ```
    /// use syrp_midi::MidiMessage;
    /// let mut seq = syrp_midi_utils::sequence::MidiMessageSequence::new();
    /// seq.add_event(MidiMessage::note_on(1, 60, 1)?, 0.0)?;
    /// seq.add_event(MidiMessage::note_off(1, 60, 0)?.with_timestamp(1.0), 0.0)?;
    /// seq.update_matched_pairs();
    /// seq.delete_event(0, true)?;
    /// assert!(seq.is_empty());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[doc(alias = "deleteEvent")]
    pub fn delete_event(
        &mut self,
        index: usize,
        delete_matching_note_up: bool,
    ) -> Result<(), SequenceError> {
        let event = self
            .events
            .get(index)
            .ok_or(SequenceError::IndexOutOfRange)?;
        let partner = event.note_off.filter(|_| delete_matching_note_up);
        self.remove_where(|i, _| i == index || Some(i) == partner);
        Ok(())
    }

    /// Merges every event of `other` into this sequence, each time shifted by
    /// `time_adjustment`. Pairs within `other` are kept; at equal times, existing events come
    /// first.
    /// ```
    /// use syrp_midi::MidiMessage;
    /// use syrp_midi_utils::sequence::MidiMessageSequence;
    /// let mut a = MidiMessageSequence::new();
    /// a.add_event(MidiMessage::controller(1, 1, 1)?, 0.0)?;
    /// let b = a.clone();
    /// a.add_sequence(&b, 2.0)?;
    /// assert_eq!(a.end_time(), Some(2.0));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[doc(alias = "addSequence")]
    pub fn add_sequence(
        &mut self,
        other: &Self,
        time_adjustment: f64,
    ) -> Result<(), SequenceError> {
        self.merge_filtered(other, time_adjustment, |_| true)
    }

    /// Like [`add_sequence`](Self::add_sequence), but only adds events whose adjusted time is
    /// at least `first_allowable_time` and below `end_of_allowable_time`. A note on whose note
    /// off falls outside the range keeps no match.
    /// ```
    /// use syrp_midi::MidiMessage;
    /// use syrp_midi_utils::sequence::MidiMessageSequence;
    /// let mut src = MidiMessageSequence::new();
    /// for t in [0.0, 1.0, 2.0] {
    ///     src.add_event(MidiMessage::controller(1, 1, 1)?.with_timestamp(t), 0.0)?;
    /// }
    /// let mut dst = MidiMessageSequence::new();
    /// dst.add_sequence_range(&src, 10.0, 11.0, 12.0)?;
    /// assert_eq!(dst.len(), 1);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[doc(alias = "addSequence")]
    pub fn add_sequence_range(
        &mut self,
        other: &Self,
        time_adjustment: f64,
        first_allowable_time: f64,
        end_of_allowable_time: f64,
    ) -> Result<(), SequenceError> {
        self.merge_filtered(other, time_adjustment, |e| {
            let t = e.time() + time_adjustment;
            t.total_cmp(&first_allowable_time).is_ge()
                && t.total_cmp(&end_of_allowable_time).is_lt()
        })
    }

    /// Pairs every note on with a later note off of the same channel and note, replacing all
    /// existing pairs. Each note off closes the earliest still-open note on of its key, so
    /// overlapping notes on one key pair first-in, first-out (Syrp's choice where JUCE's
    /// documentation is silent). A note on with no later note off keeps no match.
    /// ```
    /// use syrp_midi::MidiMessage;
    /// let mut seq = syrp_midi_utils::sequence::MidiMessageSequence::new();
    /// seq.add_event(MidiMessage::note_on(2, 64, 9)?, 0.0)?;
    /// seq.add_event(MidiMessage::note_on(2, 64, 0)?.with_timestamp(3.0), 0.0)?;
    /// seq.update_matched_pairs();
    /// assert_eq!(seq.index_of_matching_key_up(0), Some(1));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[doc(alias = "updateMatchedPairs")]
    pub fn update_matched_pairs(&mut self) {
        let mut open: HashMap<(u8, u8), VecDeque<usize>> = HashMap::new();
        for i in 0..self.events.len() {
            self.events[i].note_off = None;
            let m = &self.events[i].message;
            let (Some(channel), Some(note)) = (m.channel(), m.note_number()) else {
                continue;
            };
            if m.is_note_on() {
                open.entry((channel, note)).or_default().push_back(i);
            } else if m.is_note_off()
                && let Some(on) = open.get_mut(&(channel, note)).and_then(VecDeque::pop_front)
            {
                self.events[on].note_off = Some(i);
            }
        }
    }

    /// Sorts the events by time, keeping equal times in their current order, and remaps the
    /// pairs. Needed only after [`message_mut`](Self::message_mut) changed a timestamp.
    /// ```
    /// use syrp_midi::MidiMessage;
    /// let mut seq = syrp_midi_utils::sequence::MidiMessageSequence::new();
    /// seq.add_event(MidiMessage::controller(1, 1, 1)?, 0.0)?;
    /// seq.add_event(MidiMessage::controller(1, 2, 1)?.with_timestamp(1.0), 0.0)?;
    /// seq.message_mut(0).unwrap().set_timestamp(2.0);
    /// seq.sort();
    /// assert_eq!(seq.start_time(), Some(1.0));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn sort(&mut self) {
        let mut order: Vec<usize> = (0..self.events.len()).collect();
        order.sort_by(|a, b| self.events[*a].time().total_cmp(&self.events[*b].time()));
        let mut new_index = vec![0; order.len()];
        for (new, old) in order.iter().enumerate() {
            new_index[*old] = new;
        }
        let mut old: Vec<Option<MidiEventHolder>> = self.events.drain(..).map(Some).collect();
        self.events
            .extend(order.iter().filter_map(|i| old[*i].take()).map(|mut e| {
                e.note_off = e.note_off.map(|off| new_index[off]);
                e
            }));
    }

    /// Adds to `dest` every channel message of `channel` (1 to 16), and every meta event too
    /// when `also_include_meta_events` is true. Pairs between copied events are kept.
    /// ```
    /// use syrp_midi::MidiMessage;
    /// use syrp_midi_utils::sequence::MidiMessageSequence;
    /// let mut seq = MidiMessageSequence::new();
    /// seq.add_event(MidiMessage::controller(3, 1, 1)?, 0.0)?;
    /// seq.add_event(MidiMessage::controller(4, 1, 1)?, 0.0)?;
    /// let mut ch3 = MidiMessageSequence::new();
    /// seq.extract_midi_channel_messages(3, &mut ch3, false)?;
    /// assert_eq!(ch3.len(), 1);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[doc(alias = "extractMidiChannelMessages")]
    pub fn extract_midi_channel_messages(
        &self,
        channel: u8,
        dest: &mut Self,
        also_include_meta_events: bool,
    ) -> Result<(), SequenceError> {
        check_channel(channel)?;
        dest.merge_filtered(self, 0.0, |e| {
            let m = &e.message;
            m.channel() == Some(channel)
                || (also_include_meta_events && m.kind() == MidiMessageKind::Meta)
        })
    }

    /// Adds every SysEx message to `dest`.
    /// ```
    /// use syrp_midi::MidiMessage;
    /// use syrp_midi_utils::sequence::MidiMessageSequence;
    /// let mut seq = MidiMessageSequence::new();
    /// seq.add_event(MidiMessage::sysex(&[0x7E])?, 0.0)?;
    /// let mut sx = MidiMessageSequence::new();
    /// seq.extract_sys_ex_messages(&mut sx)?;
    /// assert_eq!(sx.len(), 1);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[doc(alias = "extractSysExMessages")]
    pub fn extract_sys_ex_messages(&self, dest: &mut Self) -> Result<(), SequenceError> {
        dest.merge_filtered(self, 0.0, |e| e.message.kind() == MidiMessageKind::SysEx)
    }

    /// Deletes every channel message of `channel` (1 to 16).
    /// ```
    /// use syrp_midi::MidiMessage;
    /// let mut seq = syrp_midi_utils::sequence::MidiMessageSequence::new();
    /// seq.add_event(MidiMessage::controller(3, 1, 1)?, 0.0)?;
    /// seq.delete_midi_channel_messages(3)?;
    /// assert!(seq.is_empty());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[doc(alias = "deleteMidiChannelMessages")]
    pub fn delete_midi_channel_messages(&mut self, channel: u8) -> Result<(), SequenceError> {
        check_channel(channel)?;
        self.remove_where(|_, e| e.message.channel() == Some(channel));
        Ok(())
    }

    /// Deletes every SysEx message.
    /// ```
    /// use syrp_midi::MidiMessage;
    /// let mut seq = syrp_midi_utils::sequence::MidiMessageSequence::new();
    /// seq.add_event(MidiMessage::sysex(&[1])?, 0.0)?;
    /// seq.delete_sys_ex_messages();
    /// assert!(seq.is_empty());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[doc(alias = "deleteSysExMessages")]
    pub fn delete_sys_ex_messages(&mut self) {
        self.remove_where(|_, e| e.message.kind() == MidiMessageKind::SysEx);
    }

    /// The messages that bring `channel` (1 to 16) to the controller, program and pitch-bend
    /// state it has at `time`, built from the events before `time`, each stamped `time`.
    ///
    /// Order and selection (Syrp's choice where JUCE's documentation is silent): first the
    /// bank selects (controllers 0 and 32) that no later program change supersedes, in their
    /// original order with the final program change; then the last value of every other
    /// controller, by number; then every data entry controller (6, 38, 96 to 101) in its
    /// original order, since together they set RPN and NRPN values; then the last pitch bend.
    /// Channel mode messages (controllers 120 to 127) are not state and are left out.
    /// ```
    /// use syrp_midi::{MidiData, MidiMessage};
    /// let mut seq = syrp_midi_utils::sequence::MidiMessageSequence::new();
    /// seq.add_event(MidiMessage::controller(1, 7, 90)?, 0.0)?;
    /// seq.add_event(MidiMessage::controller(1, 7, 100)?.with_timestamp(1.0), 0.0)?;
    /// let state = seq.create_controller_updates_for_time(1, 5.0)?;
    /// assert_eq!(state[0].bytes(), &[0xB0, 7, 100]);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[doc(alias = "createControllerUpdatesForTime")]
    pub fn create_controller_updates_for_time(
        &self,
        channel: u8,
        time: f64,
    ) -> Result<Vec<MidiMessage>, SequenceError> {
        check_channel(channel)?;
        let before = &self.events[..self.next_index_at_time(time)];
        let on_channel = || {
            before
                .iter()
                .map(|e| &e.message)
                .filter(|m| m.channel() == Some(channel))
        };
        let cc = |m: &MidiMessage| m.controller_number();
        let programs: Vec<usize> = before
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                e.message.channel() == Some(channel) && e.message.program_number().is_some()
            })
            .map(|(i, _)| i)
            .collect();
        // Bank selects before the last-but-one program change are superseded.
        let bank_from = programs.len().checked_sub(2).map_or(0, |k| programs[k] + 1);
        let last_program = programs.last().copied();
        let mut out = Vec::new();
        for (i, e) in before.iter().enumerate().skip(bank_from) {
            let m = &e.message;
            let is_bank = m.channel() == Some(channel) && matches!(cc(m), Some(0 | 32));
            if is_bank || Some(i) == last_program {
                out.push(m.clone());
            }
        }
        let mut last_value: [Option<&MidiMessage>; 120] = [None; 120];
        for m in on_channel() {
            if let Some(n @ (1..=5 | 7..=31 | 33..=37 | 39..=95 | 102..=119)) = cc(m) {
                last_value[usize::from(n)] = Some(m);
            }
        }
        out.extend(last_value.iter().flatten().map(|m| (*m).clone()));
        out.extend(
            on_channel()
                .filter(|m| matches!(cc(m), Some(6 | 38 | 96..=101)))
                .cloned(),
        );
        out.extend(
            on_channel()
                .rfind(|m| m.pitch_bend_value().is_some())
                .cloned(),
        );
        for m in &mut out {
            m.set_timestamp(time);
        }
        Ok(out)
    }

    /// Merges the events of `other` that `keep` accepts, shifted by `time_adjustment`, keeping
    /// pairs whose two ends are both copied.
    fn merge_filtered(
        &mut self,
        other: &Self,
        time_adjustment: f64,
        keep: impl Fn(&MidiEventHolder) -> bool,
    ) -> Result<(), SequenceError> {
        let base = self.events.len();
        let mut target = Vec::new();
        target
            .try_reserve_exact(other.events.len())
            .map_err(|_| SequenceError::OutOfMemory)?;
        let mut count = 0;
        for e in &other.events {
            target.push(keep(e).then(|| {
                count += 1;
                base + count - 1
            }));
        }
        self.events
            .try_reserve_exact(count)
            .map_err(|_| SequenceError::OutOfMemory)?;
        for (e, slot) in other.events.iter().zip(&target) {
            if slot.is_some() {
                let message = e.message.clone().with_timestamp(e.time() + time_adjustment);
                let note_off = e.note_off.and_then(|off| target[off]);
                self.events.push(MidiEventHolder { message, note_off });
            }
        }
        self.sort();
        Ok(())
    }

    /// Removes the events for which `remove(index, event)` is true and remaps the pairs.
    fn remove_where(&mut self, remove: impl Fn(usize, &MidiEventHolder) -> bool) {
        let mut new_index = Vec::with_capacity(self.events.len());
        let mut kept = 0;
        for (i, e) in self.events.iter().enumerate() {
            let gone = remove(i, e);
            new_index.push((!gone).then_some(kept));
            kept += usize::from(!gone);
        }
        let mut i = 0;
        self.events.retain(|_| {
            i += 1;
            new_index[i - 1].is_some()
        });
        for e in &mut self.events {
            e.note_off = e.note_off.and_then(|off| new_index[off]);
        }
    }
}

impl<'a> IntoIterator for &'a MidiMessageSequence {
    type Item = &'a MidiEventHolder;
    type IntoIter = std::slice::Iter<'a, MidiEventHolder>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

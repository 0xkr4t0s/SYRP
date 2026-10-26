// SPDX-License-Identifier: MIT OR Apache-2.0
//! [`UmpBuffer`]: packed event heads plus one `u32` word arena (ADR 0003 D2).

use std::ops::Range;

use syrp_rt::RtSafe;

use crate::{MidiError, UmpPacket};

/// One event's fixed-size head: where it sits in the block and where its words are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct UmpHead {
    sample_offset: u32,
    start: u32,
}

/// A list of Universal MIDI Packets ordered by sample offset within one audio block.
///
/// The same packed layout as [`MidiBuffer`](crate::MidiBuffer): fixed-size heads kept sorted
/// by sample offset (equal offsets in the order added) plus one arena of `u32` words holding
/// every packet back to back. [`try_add`](Self::try_add) never allocates; it fails with
/// [`MidiError::BufferFull`] when either capacity is used up. [`add`](Self::add) grows the
/// buffer and is for the message thread. Drop the buffer off the audio thread.
/// ```
/// use syrp_midi::{UmpBuffer, UmpPacket};
/// let mut buf = UmpBuffer::with_capacity(8, 32);
/// buf.try_add(UmpPacket::midi2_note_on(0, 1, 60, 0xFFFF, 0, 0).unwrap(), 10).unwrap();
/// buf.try_add(UmpPacket::system(0, 0xF8, 0, 0).unwrap(), 2).unwrap();
/// let offsets: Vec<u32> = buf.iter().map(|e| e.sample_offset()).collect();
/// assert_eq!(offsets, [2, 10]);
/// ```
#[derive(Debug, Clone, Default)]
pub struct UmpBuffer {
    heads: Vec<UmpHead>,
    arena: Vec<u32>,
}

impl UmpBuffer {
    /// An empty buffer that allocates nothing.
    /// ```
    /// assert!(syrp_midi::UmpBuffer::new().is_empty());
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty buffer with room for `events` packets and `words` words in total.
    /// ```
    /// let buf = syrp_midi::UmpBuffer::with_capacity(64, 256);
    /// assert!(buf.event_capacity() >= 64 && buf.word_capacity() >= 256);
    /// ```
    #[must_use]
    pub fn with_capacity(events: usize, words: usize) -> Self {
        UmpBuffer {
            heads: Vec::with_capacity(events),
            arena: Vec::with_capacity(words),
        }
    }

    /// Grows both capacities so that `events` more packets and `words` more words fit.
    /// Allocates: message thread (or prepare) only.
    /// ```
    /// let mut buf = syrp_midi::UmpBuffer::new();
    /// buf.reserve(4, 16);
    /// assert!(buf.event_capacity() >= 4 && buf.word_capacity() >= 16);
    /// ```
    #[doc(alias = "ensureSize")]
    pub fn reserve(&mut self, events: usize, words: usize) {
        self.heads.reserve(events);
        self.arena.reserve(words);
    }

    /// How many packets fit in total without allocating.
    /// ```
    /// assert_eq!(syrp_midi::UmpBuffer::new().event_capacity(), 0);
    /// ```
    #[must_use]
    pub fn event_capacity(&self) -> usize {
        self.heads.capacity()
    }

    /// How many words fit in total without allocating.
    /// ```
    /// assert_eq!(syrp_midi::UmpBuffer::new().word_capacity(), 0);
    /// ```
    #[must_use]
    pub fn word_capacity(&self) -> usize {
        self.arena.capacity()
    }

    /// Adds one packet at `sample_offset` without ever allocating (RT-safe). Fails with
    /// [`MidiError::BufferFull`], leaving the buffer unchanged, when either capacity is full.
    /// ```
    /// use syrp_midi::{MidiError, UmpBuffer, UmpPacket};
    /// let mut buf = UmpBuffer::with_capacity(4, 1);
    /// let on = UmpPacket::midi2_note_on(0, 1, 60, 1, 0, 0).unwrap(); // two words
    /// assert_eq!(buf.try_add(on, 0), Err(MidiError::BufferFull));
    /// ```
    pub fn try_add(&mut self, packet: UmpPacket, sample_offset: u32) -> Result<(), MidiError> {
        let fits = self.heads.len() < self.heads.capacity()
            && self.arena.capacity() - self.arena.len() >= packet.words().len();
        if !fits {
            return Err(MidiError::BufferFull);
        }
        self.insert(packet, sample_offset)
    }

    /// Adds one packet at `sample_offset`, growing the buffer when needed (message thread).
    /// Fails only past 2^32 words, with [`MidiError::BufferFull`].
    /// ```
    /// let mut buf = syrp_midi::UmpBuffer::new();
    /// buf.add(syrp_midi::UmpPacket::noop(), 0).unwrap();
    /// assert_eq!(buf.len(), 1);
    /// ```
    pub fn add(&mut self, packet: UmpPacket, sample_offset: u32) -> Result<(), MidiError> {
        self.insert(packet, sample_offset)
    }

    /// Inserts after every packet at or before `sample_offset`.
    fn insert(&mut self, packet: UmpPacket, sample_offset: u32) -> Result<(), MidiError> {
        let start = u32::try_from(self.arena.len()).map_err(|_| MidiError::BufferFull)?;
        start.checked_add(4).ok_or(MidiError::BufferFull)?;
        let at = self
            .heads
            .partition_point(|h| h.sample_offset <= sample_offset);
        self.arena.extend_from_slice(packet.words());
        self.heads.insert(
            at,
            UmpHead {
                sample_offset,
                start,
            },
        );
        Ok(())
    }

    /// Removes every packet, keeping both capacities (RT-safe).
    /// ```
    /// let mut buf = syrp_midi::UmpBuffer::new();
    /// buf.add(syrp_midi::UmpPacket::noop(), 0).unwrap();
    /// buf.clear();
    /// assert!(buf.is_empty());
    /// ```
    pub fn clear(&mut self) {
        self.heads.clear();
        self.arena.clear();
    }

    /// The number of packets.
    /// ```
    /// assert_eq!(syrp_midi::UmpBuffer::new().len(), 0);
    /// ```
    #[must_use]
    pub fn len(&self) -> usize {
        self.heads.len()
    }

    /// True when the buffer holds no packets.
    /// ```
    /// assert!(syrp_midi::UmpBuffer::new().is_empty());
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.heads.is_empty()
    }

    /// The offset of the earliest packet.
    /// ```
    /// assert_eq!(syrp_midi::UmpBuffer::new().first_sample_offset(), None);
    /// ```
    #[must_use]
    pub fn first_sample_offset(&self) -> Option<u32> {
        self.heads.first().map(|h| h.sample_offset)
    }

    /// The offset of the latest packet.
    /// ```
    /// assert_eq!(syrp_midi::UmpBuffer::new().last_sample_offset(), None);
    /// ```
    #[must_use]
    pub fn last_sample_offset(&self) -> Option<u32> {
        self.heads.last().map(|h| h.sample_offset)
    }

    /// Iterates over every packet in order (RT-safe).
    /// ```
    /// let mut buf = syrp_midi::UmpBuffer::new();
    /// buf.add(syrp_midi::UmpPacket::noop(), 3).unwrap();
    /// assert_eq!(buf.iter().next().map(|e| e.packet()), Some(syrp_midi::UmpPacket::noop()));
    /// ```
    #[must_use]
    pub fn iter(&self) -> UmpEvents<'_> {
        UmpEvents {
            heads: self.heads.iter(),
            arena: &self.arena,
        }
    }

    /// Iterates over the packets whose offsets lie in `range` (RT-safe).
    /// ```
    /// let mut buf = syrp_midi::UmpBuffer::new();
    /// for t in [1, 4, 8] { buf.add(syrp_midi::UmpPacket::noop(), t).unwrap(); }
    /// assert_eq!(buf.range(2..8).count(), 1);
    /// ```
    #[must_use]
    pub fn range(&self, range: Range<u32>) -> UmpEvents<'_> {
        let lo = self
            .heads
            .partition_point(|h| h.sample_offset < range.start);
        let hi = self.heads.partition_point(|h| h.sample_offset < range.end);
        UmpEvents {
            heads: self.heads[lo..hi.max(lo)].iter(),
            arena: &self.arena,
        }
    }
}

impl<'a> IntoIterator for &'a UmpBuffer {
    type Item = UmpEvent;
    type IntoIter = UmpEvents<'a>;

    fn into_iter(self) -> UmpEvents<'a> {
        self.iter()
    }
}

// SAFETY: every method documented as RT-safe (`try_add`, `clear`, the accessors and iteration)
// works within existing capacity and never allocates, frees, locks or blocks. Moving the buffer
// moves two `Vec` headers. Dropping frees, as documented.
unsafe impl RtSafe for UmpBuffer {}

/// One packet read out of a [`UmpBuffer`], by value, with its sample offset.
/// ```
/// let mut buf = syrp_midi::UmpBuffer::new();
/// buf.add(syrp_midi::UmpPacket::noop(), 5).unwrap();
/// let ev: syrp_midi::UmpEvent = buf.iter().next().unwrap();
/// assert_eq!(ev.sample_offset(), 5);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UmpEvent {
    packet: UmpPacket,
    sample_offset: u32,
}

impl UmpEvent {
    /// The packet.
    /// ```
    /// let mut buf = syrp_midi::UmpBuffer::new();
    /// buf.add(syrp_midi::UmpPacket::noop(), 0).unwrap();
    /// assert_eq!(buf.iter().next().unwrap().packet().words(), &[0]);
    /// ```
    #[must_use]
    pub fn packet(self) -> UmpPacket {
        self.packet
    }

    /// The position of the packet within its block, in samples.
    /// ```
    /// let mut buf = syrp_midi::UmpBuffer::new();
    /// buf.add(syrp_midi::UmpPacket::noop(), 9).unwrap();
    /// assert_eq!(buf.iter().next().unwrap().sample_offset(), 9);
    /// ```
    #[must_use]
    pub fn sample_offset(self) -> u32 {
        self.sample_offset
    }
}

// SAFETY: a `Copy` packet and a `u32`; no destructor, no allocating method.
unsafe impl RtSafe for UmpEvent {}

/// An iterator over a [`UmpBuffer`]'s packets, yielding [`UmpEvent`] values.
/// ```
/// let buf = syrp_midi::UmpBuffer::new();
/// let events: syrp_midi::UmpEvents<'_> = buf.iter();
/// assert_eq!(events.len(), 0);
/// ```
#[derive(Debug, Clone)]
pub struct UmpEvents<'a> {
    heads: std::slice::Iter<'a, UmpHead>,
    arena: &'a [u32],
}

impl UmpEvents<'_> {
    fn event(&self, h: &UmpHead) -> UmpEvent {
        // Words in the arena were copied from a valid packet, so `read` cannot fail here; the
        // fallback keeps the iterator panic-free regardless.
        let words = self.arena.get(h.start as usize..).unwrap_or_default();
        let packet = UmpPacket::read(words).map_or(UmpPacket::noop(), |(p, _)| p);
        UmpEvent {
            packet,
            sample_offset: h.sample_offset,
        }
    }
}

impl Iterator for UmpEvents<'_> {
    type Item = UmpEvent;

    fn next(&mut self) -> Option<UmpEvent> {
        let h = self.heads.next()?;
        Some(self.event(h))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.heads.size_hint()
    }
}

impl DoubleEndedIterator for UmpEvents<'_> {
    fn next_back(&mut self) -> Option<UmpEvent> {
        let h = self.heads.next_back()?;
        Some(self.event(h))
    }
}

impl ExactSizeIterator for UmpEvents<'_> {}

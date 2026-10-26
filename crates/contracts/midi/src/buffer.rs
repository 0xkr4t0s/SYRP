// SPDX-License-Identifier: MIT OR Apache-2.0
//! [`MidiBuffer`]: packed event heads plus one byte arena (ADR 0003 D2).

use std::ops::Range;

use syrp_rt::RtSafe;

use crate::{MidiError, MidiMessageRef, validate};

/// One event's fixed-size head: where it sits in the block and where its bytes are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EventHead {
    sample_offset: u32,
    start: u32,
    len: u32,
}

/// A list of MIDI events ordered by sample offset within one audio block.
///
/// Storage is packed: a `Vec` of fixed-size event heads kept sorted by sample offset, plus one
/// byte arena holding every message's bytes back to back, SysEx included. Events at the same
/// offset keep the order they were added in. Iteration yields borrowed [`MidiMessageRef`]
/// views and never copies.
///
/// On the audio thread, add events with [`try_add_event`](Self::try_add_event), which never
/// allocates and fails with [`MidiError::BufferFull`] when either capacity is used up; size
/// the buffer with [`with_capacity`](Self::with_capacity) and [`clear`](Self::clear) it each
/// block. [`add_event`](Self::add_event) grows the buffer and is for the message thread.
/// Drop the buffer off the audio thread (dropping frees its storage).
/// ```
/// use syrp_midi::{MidiBuffer, MidiData, MidiMessage};
/// let mut buf = MidiBuffer::with_capacity(8, 64);
/// buf.try_add_event(MidiMessage::note_off(1, 60, 0).unwrap().bytes(), 20).unwrap();
/// buf.try_add_event(MidiMessage::note_on(1, 62, 90).unwrap().bytes(), 4).unwrap();
/// let offsets: Vec<u32> = buf.iter().map(|e| e.sample_offset()).collect();
/// assert_eq!(offsets, [4, 20]);
/// ```
#[derive(Debug, Clone, Default)]
pub struct MidiBuffer {
    heads: Vec<EventHead>,
    arena: Vec<u8>,
}

impl MidiBuffer {
    /// An empty buffer that allocates nothing.
    /// ```
    /// assert!(syrp_midi::MidiBuffer::new().is_empty());
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty buffer with room for `events` events and `bytes` message bytes in total.
    /// ```
    /// let buf = syrp_midi::MidiBuffer::with_capacity(128, 1024);
    /// assert!(buf.event_capacity() >= 128 && buf.byte_capacity() >= 1024);
    /// ```
    #[must_use]
    pub fn with_capacity(events: usize, bytes: usize) -> Self {
        MidiBuffer {
            heads: Vec::with_capacity(events),
            arena: Vec::with_capacity(bytes),
        }
    }

    /// Grows both capacities so that `events` more events and `bytes` more bytes fit.
    /// Allocates: message thread (or prepare) only.
    /// ```
    /// let mut buf = syrp_midi::MidiBuffer::new();
    /// buf.reserve(4, 16);
    /// assert!(buf.event_capacity() >= 4 && buf.byte_capacity() >= 16);
    /// ```
    #[doc(alias = "ensureSize")]
    pub fn reserve(&mut self, events: usize, bytes: usize) {
        self.heads.reserve(events);
        self.arena.reserve(bytes);
    }

    /// How many events fit in total without allocating.
    /// ```
    /// assert_eq!(syrp_midi::MidiBuffer::new().event_capacity(), 0);
    /// ```
    #[must_use]
    pub fn event_capacity(&self) -> usize {
        self.heads.capacity()
    }

    /// How many message bytes fit in total without allocating.
    /// ```
    /// assert_eq!(syrp_midi::MidiBuffer::new().byte_capacity(), 0);
    /// ```
    #[must_use]
    pub fn byte_capacity(&self) -> usize {
        self.arena.capacity()
    }

    /// Adds one message at `sample_offset` without ever allocating (RT-safe).
    ///
    /// Fails with [`MidiError::Malformed`] unless `bytes` is exactly one well-formed message,
    /// and with [`MidiError::BufferFull`] when the event or byte capacity is full; the buffer
    /// is unchanged on failure.
    /// ```
    /// use syrp_midi::{MidiBuffer, MidiError}; let mut buf = MidiBuffer::with_capacity(1, 3);
    /// buf.try_add_event(&[0x90, 60, 100], 0).unwrap();
    /// assert_eq!(buf.try_add_event(&[0xF8], 0), Err(MidiError::BufferFull));
    /// ```
    #[doc(alias = "addEvent")]
    pub fn try_add_event(&mut self, bytes: &[u8], sample_offset: u32) -> Result<(), MidiError> {
        validate(bytes)?;
        let fits = self.heads.len() < self.heads.capacity()
            && self.arena.capacity() - self.arena.len() >= bytes.len();
        if !fits {
            return Err(MidiError::BufferFull);
        }
        self.insert(bytes, sample_offset)
    }

    /// Adds one message at `sample_offset`, growing the buffer when needed.
    /// Allocates: message thread only. Fails only with [`MidiError::Malformed`] (or
    /// [`MidiError::BufferFull`] past 4 GiB of bytes).
    /// ```
    /// let mut buf = syrp_midi::MidiBuffer::new();
    /// buf.add_event(&[0xF0, 0x7D, 0xF7], 9).unwrap();
    /// assert_eq!(buf.len(), 1);
    /// ```
    #[doc(alias = "addEvent")]
    pub fn add_event(&mut self, bytes: &[u8], sample_offset: u32) -> Result<(), MidiError> {
        validate(bytes)?;
        self.insert(bytes, sample_offset)
    }

    /// Inserts validated bytes after every event at or before `sample_offset`.
    fn insert(&mut self, bytes: &[u8], sample_offset: u32) -> Result<(), MidiError> {
        let start = u32::try_from(self.arena.len()).map_err(|_| MidiError::BufferFull)?;
        let len = u32::try_from(bytes.len()).map_err(|_| MidiError::BufferFull)?;
        start.checked_add(len).ok_or(MidiError::BufferFull)?;
        let at = self
            .heads
            .partition_point(|h| h.sample_offset <= sample_offset);
        self.arena.extend_from_slice(bytes);
        self.heads.insert(
            at,
            EventHead {
                sample_offset,
                start,
                len,
            },
        );
        Ok(())
    }

    /// Removes every event, keeping both capacities (RT-safe).
    /// ```
    /// let mut buf = syrp_midi::MidiBuffer::with_capacity(4, 4);
    /// buf.try_add_event(&[0xF8], 0).unwrap();
    /// buf.clear();
    /// assert!(buf.is_empty());
    /// ```
    pub fn clear(&mut self) {
        self.heads.clear();
        self.arena.clear();
    }

    /// The number of events.
    /// ```
    /// assert_eq!(syrp_midi::MidiBuffer::new().len(), 0);
    /// ```
    #[doc(alias = "getNumEvents")]
    #[must_use]
    pub fn len(&self) -> usize {
        self.heads.len()
    }

    /// True when the buffer holds no events.
    /// ```
    /// assert!(syrp_midi::MidiBuffer::new().is_empty());
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.heads.is_empty()
    }

    /// The offset of the earliest event.
    /// ```
    /// let mut buf = syrp_midi::MidiBuffer::new();
    /// buf.add_event(&[0xF8], 7).unwrap();
    /// assert_eq!(buf.first_sample_offset(), Some(7));
    /// ```
    #[doc(alias = "getFirstEventTime")]
    #[must_use]
    pub fn first_sample_offset(&self) -> Option<u32> {
        self.heads.first().map(|h| h.sample_offset)
    }

    /// The offset of the latest event.
    /// ```
    /// assert_eq!(syrp_midi::MidiBuffer::new().last_sample_offset(), None);
    /// ```
    #[doc(alias = "getLastEventTime")]
    #[must_use]
    pub fn last_sample_offset(&self) -> Option<u32> {
        self.heads.last().map(|h| h.sample_offset)
    }

    /// Iterates over every event in order (RT-safe, no copies).
    /// ```
    /// use syrp_midi::MidiData; let mut buf = syrp_midi::MidiBuffer::new();
    /// buf.add_event(&[0xC0, 3], 0).unwrap();
    /// assert_eq!(buf.iter().next().and_then(|e| e.program_number()), Some(3));
    /// ```
    #[must_use]
    pub fn iter(&self) -> Events<'_> {
        Events {
            heads: self.heads.iter(),
            arena: &self.arena,
        }
    }

    /// Iterates over the events whose offsets lie in `range` (RT-safe, no copies).
    /// ```
    /// let mut buf = syrp_midi::MidiBuffer::new();
    /// for t in [1, 4, 8] { buf.add_event(&[0xF8], t).unwrap(); }
    /// assert_eq!(buf.range(2..8).count(), 1);
    /// ```
    #[doc(alias = "findNextSamplePosition")]
    #[must_use]
    pub fn range(&self, range: Range<u32>) -> Events<'_> {
        let r = self.index_range(range);
        Events {
            heads: self.heads[r].iter(),
            arena: &self.arena,
        }
    }

    /// The head indices covering sample offsets in `range` (empty for an inverted range).
    fn index_range(&self, range: Range<u32>) -> Range<usize> {
        let lo = self
            .heads
            .partition_point(|h| h.sample_offset < range.start);
        let hi = self.heads.partition_point(|h| h.sample_offset < range.end);
        lo..hi.max(lo)
    }
}

impl<'a> IntoIterator for &'a MidiBuffer {
    type Item = MidiMessageRef<'a>;
    type IntoIter = Events<'a>;

    fn into_iter(self) -> Events<'a> {
        self.iter()
    }
}

// SAFETY: every method documented as RT-safe (`try_add_event`, `clear`, the
// accessors and iteration) works within existing capacity and never allocates, frees, locks
// or blocks. Moving the buffer moves two `Vec` headers. Dropping frees, as documented.
unsafe impl RtSafe for MidiBuffer {}

/// An iterator over a [`MidiBuffer`]'s events as borrowed [`MidiMessageRef`] views.
/// ```
/// let buf = syrp_midi::MidiBuffer::new();
/// let events: syrp_midi::Events<'_> = buf.iter();
/// assert_eq!(events.len(), 0);
/// ```
#[derive(Debug, Clone)]
pub struct Events<'a> {
    heads: std::slice::Iter<'a, EventHead>,
    arena: &'a [u8],
}

impl<'a> Events<'a> {
    fn view(&self, h: &EventHead) -> MidiMessageRef<'a> {
        let (start, len) = (h.start as usize, h.len as usize);
        MidiMessageRef::from_valid(&self.arena[start..start + len], h.sample_offset)
    }
}

impl<'a> Iterator for Events<'a> {
    type Item = MidiMessageRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let h = self.heads.next()?;
        Some(self.view(h))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.heads.size_hint()
    }
}

impl DoubleEndedIterator for Events<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let h = self.heads.next_back()?;
        Some(self.view(h))
    }
}

impl ExactSizeIterator for Events<'_> {}

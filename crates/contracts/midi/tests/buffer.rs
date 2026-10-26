// SPDX-License-Identifier: MIT OR Apache-2.0
//! Behaviour tests for MidiBuffer (WP-0.2-03 increment 2: B2, and B5 for the buffer).
//! A separate binary because it installs the `syrp_rt` allocation guard.

use proptest::prelude::*;
use syrp_error::Diagnostic;
use syrp_midi::MidiError::{BufferFull, Malformed};
use syrp_midi::{MidiBuffer, MidiData, MidiError, MidiMessage as M, MidiMessageRef};
use syrp_rt::{GuardedAllocator, RtSafe, no_alloc};

#[global_allocator]
static ALLOC: GuardedAllocator = GuardedAllocator;

type R = Result<(), MidiError>;

fn offsets<'a>(events: impl Iterator<Item = MidiMessageRef<'a>>) -> Vec<(u32, u8)> {
    events
        .map(|e| (e.sample_offset(), e.as_slice()[1]))
        .collect()
}

#[test]
fn b2_events_are_ordered_by_offset_and_stable_for_equal_offsets() -> R {
    let mut buf = MidiBuffer::new();
    for (t, n) in [(10, 1), (0, 2), (10, 3), (5, 4), (0, 5), (10, 6)] {
        buf.add_event(&[0x90, n, 1], t)?;
    }
    assert_eq!(
        offsets(buf.iter()),
        [(0, 2), (0, 5), (5, 4), (10, 1), (10, 3), (10, 6)]
    );
    assert_eq!((buf.len(), buf.iter().len(), buf.is_empty()), (6, 6, false));
    let ends = (buf.first_sample_offset(), buf.last_sample_offset());
    assert_eq!(ends, (Some(0), Some(10)));
    assert_eq!(buf.iter().next_back().map(|e| e.as_slice()[1]), Some(6));
    assert_eq!((&buf).into_iter().count(), 6);

    // Iteration over a sample range [start, end).
    assert_eq!(offsets(buf.range(5..10)), [(5, 4)]);
    assert_eq!(offsets(buf.range(1..11)).len(), 4);
    assert_eq!(buf.range(11..20).count(), 0);
    #[allow(clippy::reversed_empty_ranges)]
    let inverted = buf.range(9..3).count();
    assert_eq!(inverted, 0);
    Ok(())
}

#[test]
fn b2_storage_is_one_arena_and_iteration_borrows_it() -> R {
    let mut buf = MidiBuffer::with_capacity(4, 64);
    let sx = M::sysex(&[1, 2, 3, 4, 5])?;
    buf.try_add_event(&[0x90, 60, 1], 2)?;
    buf.try_add_event(sx.bytes(), 1)?;
    let views: Vec<MidiMessageRef<'_>> = buf.iter().collect();
    assert_eq!(views[0].sysex_data(), Some(&[1, 2, 3, 4, 5][..]));
    // Both views point into one contiguous allocation, bytes in insertion order.
    let (a, b) = (views[1].as_slice().as_ptr(), views[0].as_slice().as_ptr());
    assert_eq!(b as usize - a as usize, 3);
    Ok(())
}

#[test]
fn b2_try_add_event_never_allocates_and_fails_when_full() -> R {
    let mut buf = MidiBuffer::with_capacity(3, 8);
    let caps = (buf.event_capacity(), buf.byte_capacity());
    no_alloc(|| -> R {
        buf.try_add_event(&[0x90, 60, 100], 5)?;
        buf.try_add_event(&[0xF0, 1, 0xF7], 0)?;
        // Byte arena full: 6 of 8 bytes used, a 3-byte message does not fit.
        assert_eq!(buf.try_add_event(&[0x80, 60, 0], 9), Err(BufferFull));
        buf.try_add_event(&[0xF8], 9)?;
        // Event heads full.
        assert_eq!(buf.try_add_event(&[0xF8], 9), Err(BufferFull));
        assert_eq!(buf.iter().filter(|e| e.is_note_on()).count(), 1);
        buf.clear();
        buf.try_add_event(&[0x80, 60, 0], 1)
    })?;
    assert_eq!((buf.event_capacity(), buf.byte_capacity()), caps);
    assert_eq!(BufferFull.code().as_str(), "SYRP-MIDI-0003");
    Ok(())
}

#[test]
fn b2_rt_types_are_rt_safe() {
    fn rt_safe<T: RtSafe>() {}
    rt_safe::<MidiBuffer>();
    rt_safe::<MidiMessageRef<'static>>();
}

proptest! {
    #[test]
    fn b5_buffer_add_never_panics_on_random_bytes(
        bytes in proptest::collection::vec(any::<u8>(), 0..40),
        t in any::<u32>(),
    ) {
        let parsed = M::from_bytes(&bytes);
        let added = MidiBuffer::with_capacity(1, 8).try_add_event(&bytes, t);
        match parsed {
            Err(_) => prop_assert_eq!(added, Err(Malformed)),
            Ok(_) => prop_assert!(added.is_ok() || (added == Err(BufferFull) && bytes.len() > 8)),
        }
    }

    #[test]
    fn b5_buffer_keeps_order_under_random_adds(
        events in proptest::collection::vec((0_u32..64, 0_u8..128), 0..64),
    ) {
        let mut buf = MidiBuffer::new();
        let mut expected = Vec::new();
        for (i, (t, n)) in events.into_iter().enumerate() {
            buf.add_event(&[0xB0, n, (i % 128) as u8], t).unwrap();
            expected.push((t, n, (i % 128) as u8));
        }
        expected.sort_by_key(|e| e.0); // stable sort: the order for equal offsets
        let got = buf.iter().map(|e| (e.sample_offset(), e.bytes()[1], e.bytes()[2]));
        prop_assert_eq!(got.collect::<Vec<_>>(), expected);
    }
}

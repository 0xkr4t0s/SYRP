// SPDX-License-Identifier: MIT OR Apache-2.0
//! Behaviour tests for UMP packets and UmpBuffer (WP-0.2-03 increment 3: B3, and B5 for UMP).
//! A separate binary because it installs the `syrp_rt` allocation guard.

use proptest::prelude::*;
use syrp_error::Diagnostic;
use syrp_midi::MidiError::{BufferFull, MalformedUmp, ValueOutOfRange};
use syrp_midi::SysExStatus::{Complete, Continue, End, Start};
use syrp_midi::UmpMessageType as T;
use syrp_midi::{MidiError, UmpBuffer, UmpEvent, UmpPacket as P};
use syrp_rt::{GuardedAllocator, RtSafe, no_alloc};

#[global_allocator]
static ALLOC: GuardedAllocator = GuardedAllocator;

type R = Result<(), MidiError>;

#[test]
fn b3_every_message_type_builds_serialises_and_parses_back() -> R {
    let cases: [(P, T, &[u32]); 9] = [
        (P::utility(4, 0xABCDE)?, T::Utility, &[0x004A_BCDE]),
        (P::system(3, 0xF2, 0x10, 0x20)?, T::System, &[0x13F2_1020]),
        (
            P::midi1_channel_voice(0, 0x9F, 60, 100)?,
            T::Midi1ChannelVoice,
            &[0x209F_3C64],
        ),
        (
            P::sysex7(1, Start, &[1, 2, 3, 4, 5, 6])?,
            T::Data64,
            &[0x3116_0102, 0x0304_0506],
        ),
        (
            P::midi2_channel_voice(0, 0xE, 2, 0, 0x8000_0000)?,
            T::Midi2ChannelVoice,
            &[0x40E1_0000, 0x8000_0000],
        ),
        (
            P::sysex8(0, Complete, 9, &[0xAA, 0xBB])?,
            T::Data128,
            &[0x5003_09AA, 0xBB00_0000, 0, 0],
        ),
        (
            P::flex_data(0, 0, 1, 1, 0, 0, [50_000_000, 0, 0])?,
            T::FlexData,
            &[0xD010_0000, 50_000_000, 0, 0],
        ),
        (
            P::stream(0, 0x001, 0x0101, [0, 0, 0])?,
            T::Stream,
            &[0xF001_0101, 0, 0, 0],
        ),
        (P::noop(), T::Utility, &[0]),
    ];
    for (p, mt, words) in cases {
        assert_eq!((p.message_type(), p.words()), (mt, words), "{p:08X?}");
        let mut stream = words.to_vec();
        stream.push(0x1234_5678);
        assert_eq!(P::read(&stream)?, (p, &[0x1234_5678][..]));
    }
    // Reserved types are carried at their size so a reader can skip them.
    for (w0, n) in [
        (0x6000_0000_u32, 1),
        (0x8000_0000, 2),
        (0xB000_0000, 3),
        (0xE000_0000, 4),
    ] {
        let words = [w0, 1, 2, 3, 4];
        let (p, rest) = P::read(&words)?;
        assert_eq!((p.words().len(), rest.len(), p.group()), (n, 5 - n, None));
    }
    Ok(())
}

#[test]
fn b3_accessors_read_each_message_type() -> R {
    let on = P::midi2_note_on(5, 16, 60, 0xC000, 3, 0x0102)?;
    assert_eq!(on.words(), &[0x459F_3C03, 0xC000_0102]);
    let read = (
        on.group(),
        on.channel(),
        on.opcode(),
        on.status(),
        on.index(),
        on.value(),
    );
    assert_eq!(
        read,
        (
            Some(5),
            Some(16),
            Some(0x9),
            0x9F,
            Some(0x3C03),
            Some(0xC000_0102)
        )
    );
    let off = P::midi2_note_off(5, 16, 60, 0xC000, 3, 0x0102)?;
    assert_eq!((off.opcode(), off.words()[1]), (Some(0x8), 0xC000_0102));

    let m1 = P::midi1_channel_voice(2, 0xB3, 7, 99)?;
    assert_eq!(
        (m1.group(), m1.channel(), m1.opcode(), m1.index()),
        (Some(2), Some(4), Some(0xB), None)
    );
    let sys = P::system(0, 0xFA, 0, 0)?;
    assert_eq!(
        (sys.status(), sys.channel(), sys.opcode()),
        (0xFA, None, None)
    );
    let flex = P::flex_data(1, 0, 0, 3, 1, 2, [1, 2, 3])?;
    assert_eq!(
        (flex.channel(), flex.status(), flex.opcode()),
        (Some(3), 0x0102, None)
    );
    assert_eq!(P::stream(3, 0x3FF, 0, [0; 3])?.status(), 0x3FF);
    assert_eq!(P::utility(2, 7)?.status(), 2);

    let s7 = P::sysex7(0, End, &[0x7F])?;
    assert_eq!((s7.sysex_status(), s7.stream_id()), (Some(End), None));
    assert_eq!(
        s7.sysex_bytes().map(Iterator::collect::<Vec<_>>),
        Some(vec![0x7F])
    );
    let data: Vec<u8> = (0xF0..=0xFC).collect(); // 13 bytes, the SysEx8 maximum
    let s8 = P::sysex8(0, Continue, 0x42, &data)?;
    assert_eq!(
        (s8.sysex_status(), s8.stream_id()),
        (Some(Continue), Some(0x42))
    );
    assert_eq!(
        s8.sysex_bytes().map(Iterator::collect::<Vec<_>>),
        Some(data)
    );
    // A mixed data set header (Data 128 status 8) is not SysEx.
    let (mds, _) = P::read(&[0x5080_0000, 0, 0, 0])?;
    assert_eq!(
        (
            mds.sysex_status(),
            mds.stream_id(),
            mds.sysex_bytes().is_none()
        ),
        (None, None, true)
    );
    Ok(())
}

#[test]
fn b3_constructors_reject_out_of_range_fields() {
    let bad = [
        P::utility(16, 0),
        P::utility(0, 1 << 20),
        P::system(16, 0xF8, 0, 0),
        P::system(0, 0xF7, 0, 0),
        P::system(0, 0xF2, 0x80, 0),
        P::midi1_channel_voice(0, 0x70, 0, 0),
        P::midi1_channel_voice(0, 0xF0, 0, 0),
        P::sysex7(0, Complete, &[0; 7]),
        P::sysex7(0, Complete, &[0x80]),
        P::midi2_channel_voice(0, 16, 1, 0, 0),
        P::midi2_channel_voice(0, 9, 0, 0, 0),
        P::midi2_note_on(0, 1, 128, 0, 0, 0),
        P::sysex8(0, Complete, 0, &[0; 14]),
        P::flex_data(0, 4, 0, 1, 0, 0, [0; 3]),
        P::stream(4, 0, 0, [0; 3]),
        P::stream(0, 0x400, 0, [0; 3]),
    ];
    for b in bad {
        assert_eq!(b, Err(ValueOutOfRange));
    }
}

#[test]
fn b3_read_rejects_short_or_malformed_packets() {
    let bad: &[&[u32]] = &[
        &[],
        &[0x4090_3C00],          // MIDI 2.0 channel voice needs two words
        &[0xF000_0000, 0, 0],    // stream needs four
        &[0x10F7_0000],          // 0xF7 is not a System status
        &[0x1080_0000],          // nor is a channel status
        &[0x2070_0000],          // MIDI 1.0 voice status without its high bit
        &[0x2090_3C80],          // data byte above 0x7F
        &[0x3007_0000, 0],       // SysEx7 count above 6
        &[0x3041_0000, 0],       // undefined Data 64 status
        &[0x3001_8000, 0],       // SysEx7 data byte above 0x7F
        &[0x5000_0000, 0, 0, 0], // SysEx8 count must include the stream id
        &[0x500F_0000, 0, 0, 0], // SysEx8 count above 14
        &[0x5040_0000, 0, 0, 0], // undefined Data 128 status
    ];
    for words in bad {
        assert_eq!(P::read(words), Err(MalformedUmp), "{words:08X?}");
    }
    assert_eq!(MalformedUmp.code().as_str(), "SYRP-MIDI-0004");
}

#[test]
fn b3_ump_buffer_is_ordered_and_iterates_by_value() -> R {
    let mut buf = UmpBuffer::new();
    for (t, note) in [(10, 1), (0, 2), (10, 3), (5, 4), (0, 5)] {
        buf.add(P::midi2_note_on(0, 1, note, 1, 0, 0)?, t)?;
    }
    buf.add(P::sysex8(0, Complete, 0, &[1])?, 5)?;
    let got: Vec<(u32, T)> = buf
        .iter()
        .map(|e| (e.sample_offset(), e.packet().message_type()))
        .collect();
    assert_eq!(got.len(), 6);
    let notes: Vec<u16> = buf
        .iter()
        .filter_map(|e| e.packet().index())
        .map(|i| i >> 8)
        .collect();
    assert_eq!(notes, [2, 5, 4, 1, 3]);
    assert_eq!(got[3], (5, T::Data128)); // after the note at 5 that came first
    let ranged: Vec<u32> = buf.range(5..10).map(UmpEvent::sample_offset).collect();
    assert_eq!(ranged, [5, 5]);
    assert_eq!(buf.iter().next_back().map(|e| e.sample_offset()), Some(10));
    assert_eq!(
        (buf.first_sample_offset(), buf.last_sample_offset()),
        (Some(0), Some(10))
    );
    assert_eq!(
        ((&buf).into_iter().len(), buf.len(), buf.is_empty()),
        (6, 6, false)
    );
    Ok(())
}

#[test]
fn b3_ump_buffer_try_add_never_allocates_and_fails_when_full() -> R {
    fn rt_safe<T: RtSafe>() {}
    rt_safe::<UmpBuffer>();
    rt_safe::<P>();
    rt_safe::<UmpEvent>();
    let mut buf = UmpBuffer::with_capacity(3, 6);
    let caps = (buf.event_capacity(), buf.word_capacity());
    let (on, sx) = (
        P::midi2_note_on(0, 1, 60, 1, 0, 0)?,
        P::sysex8(0, Start, 0, &[1])?,
    );
    no_alloc(|| -> R {
        buf.try_add(on, 4)?;
        buf.try_add(sx, 0)?; // 6 of 6 words used
        assert_eq!(buf.try_add(P::noop(), 9), Err(BufferFull));
        buf.clear();
        for t in 0..3 {
            buf.try_add(P::noop(), t)?;
        }
        assert_eq!(buf.try_add(P::noop(), 9), Err(BufferFull)); // heads full
        assert_eq!(buf.iter().map(|e| e.sample_offset()).sum::<u32>(), 3);
        Ok(())
    })?;
    assert_eq!((buf.event_capacity(), buf.word_capacity()), caps);
    buf.reserve(2, 8); // off the audio thread: room for two more 4-word packets
    let more = P::sysex8(0, Complete, 0, &[])?;
    no_alloc(|| buf.try_add(more, 0).and(buf.try_add(more, 1)))?;
    Ok(())
}

proptest! {
    #[test]
    fn b5_ump_read_never_panics_on_random_words(words in proptest::collection::vec(any::<u32>(), 0..8)) {
        if let Ok((p, rest)) = P::read(&words) {
            let n = p.words().len();
            prop_assert_eq!(p.words(), &words[..n]);
            prop_assert_eq!(rest.len(), words.len() - n);
            // Every accessor works on anything that parsed, and the buffer gives it back.
            let _ = (p.group(), p.channel(), p.opcode(), p.status(), p.index(), p.value());
            let _ = (p.sysex_status(), p.stream_id(), p.sysex_bytes().map(Iterator::count));
            let mut buf = UmpBuffer::with_capacity(1, 4);
            buf.try_add(p, 0).unwrap();
            prop_assert_eq!(buf.iter().next().map(UmpEvent::packet), Some(p));
        }
    }

    #[test]
    fn b5_built_packets_parse_back_unchanged(
        g in 0_u8..16, ch in 1_u8..=16, op in 0_u8..16, a in 0_u8..128, w in any::<u32>(),
        data in proptest::collection::vec(any::<u8>(), 0..=13),
    ) {
        let d7: Vec<u8> = data.iter().take(6).map(|b| b & 0x7F).collect();
        let built = [
            P::utility(op, w >> 12), P::system(g, 0xF1 + (a % 6), a, 0), P::midi1_channel_voice(g, 0x80 | (a & 0x6F), a, a),
            P::sysex7(g, Continue, &d7), P::midi2_channel_voice(g, op, ch, w as u16, w),
            P::sysex8(g, End, a, &data), P::flex_data(g, op & 3, 1, ch, a, a, [w; 3]),
            P::stream(op & 3, w as u16 & 0x3FF, a.into(), [w; 3]),
        ];
        for p in built {
            let p = p.unwrap();
            prop_assert_eq!(P::read(p.words()).unwrap(), (p, &[][..]));
        }
    }
}

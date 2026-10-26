// SPDX-License-Identifier: MIT OR Apache-2.0
//! Behaviour tests for syrp-midi (WP-0.2-03, first increment: B1, B5 and B6 for MIDI 1.0 messages).

use proptest::prelude::*;
use syrp_error::Diagnostic;
use syrp_midi::MidiError::{Malformed, ValueOutOfRange};
use syrp_midi::MidiMessageKind as K;
use syrp_midi::{MidiData, MidiError, MidiMessage as M, MidiMessageRef};
use syrp_rt::RtSafe;

type R = Result<(), MidiError>;

#[test]
fn b1_channel_voice_messages_build_classify_and_read_back() -> R {
    let on = M::note_on(3, 60, 100)?;
    assert_eq!(on.bytes(), &[0x92, 60, 100]);
    let read = (on.kind(), on.channel(), on.note_number(), on.velocity());
    assert_eq!(read, (K::NoteOn, Some(3), Some(60), Some(100)));
    assert!(on.is_note_on() && !on.is_note_off());

    let off = M::note_off(16, 61, 40)?;
    assert_eq!(
        (off.bytes()[0], off.kind(), off.velocity()),
        (0x8F, K::NoteOff, Some(40))
    );
    let zero = M::note_on(1, 61, 0)?;
    assert!(zero.is_note_off() && !zero.is_note_on());

    let cc = M::controller(1, 74, 5)?;
    let read = (cc.kind(), cc.controller_number(), cc.controller_value());
    assert_eq!(read, (K::Controller, Some(74), Some(5)));
    let pc = M::program_change(2, 9)?;
    let read = (pc.bytes(), pc.kind(), pc.program_number());
    assert_eq!(read, (&[0xC1, 9][..], K::ProgramChange, Some(9)));
    let pb = M::pitch_bend(1, 0x2001)?;
    assert_eq!(pb.bytes(), &[0xE0, 0x01, 0x40]);
    assert_eq!(
        (pb.kind(), pb.pitch_bend_value()),
        (K::PitchBend, Some(0x2001))
    );
    let cp = M::channel_pressure(1, 33)?;
    assert_eq!(
        (cp.kind(), cp.channel_pressure_value()),
        (K::ChannelPressure, Some(33))
    );
    let pp = M::poly_pressure(1, 50, 44)?;
    let read = (
        pp.kind(),
        pp.note_number(),
        pp.poly_pressure_value(),
        pp.velocity(),
    );
    assert_eq!(read, (K::PolyPressure, Some(50), Some(44), None));

    // Accessors of the wrong kind read nothing.
    let read = (cc.note_number(), on.controller_value(), pb.program_number());
    assert_eq!(read, (None, None, None));
    Ok(())
}

#[test]
fn b1_constructors_reject_out_of_range_values() {
    assert_eq!(M::note_on(0, 60, 1), Err(ValueOutOfRange));
    assert_eq!(M::note_on(17, 60, 1), Err(ValueOutOfRange));
    assert_eq!(M::controller(1, 128, 0), Err(ValueOutOfRange));
    assert_eq!(M::pitch_bend(1, 16384), Err(ValueOutOfRange));
    assert_eq!(M::sysex(&[1, 0xF7]), Err(ValueOutOfRange));
    assert_eq!(M::meta(0x80, &[]), Err(ValueOutOfRange));
    assert_eq!(ValueOutOfRange.code().as_str(), "SYRP-MIDI-0002");
}

#[test]
fn b1_sysex_meta_and_system_messages() -> R {
    let sx = M::sysex(&[0x7E, 0x7F, 0x09, 0x01])?;
    assert_eq!((sx.kind(), sx.channel()), (K::SysEx, None));
    assert_eq!(sx.sysex_data(), Some(&[0x7E, 0x7F, 0x09, 0x01][..]));
    assert_eq!(M::sysex(&[])?.bytes(), &[0xF0, 0xF7]);

    let tempo = M::meta(0x51, &[0x07, 0xA1, 0x20])?;
    assert_eq!(tempo.bytes(), &[0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20]);
    assert_eq!((tempo.kind(), tempo.meta_type()), (K::Meta, Some(0x51)));
    let long = vec![b'a'; 200];
    let text = M::meta(0x01, &long)?;
    // 200 as a variable-length quantity is 0x81 0x48.
    assert_eq!(&text.bytes()[..4], &[0xFF, 0x01, 0x81, 0x48]);
    assert_eq!(text.meta_data(), Some(&long[..]));
    assert_eq!(M::from_bytes(text.bytes())?, text);

    let reset = M::from_bytes(&[0xFF])?;
    assert_eq!((reset.kind(), reset.meta_type()), (K::SystemRealtime, None));
    assert_eq!(M::from_bytes(&[0xF8])?.kind(), K::SystemRealtime);
    assert_eq!(M::from_bytes(&[0xF2, 0x10, 0x20])?.kind(), K::SystemCommon);
    assert_eq!(M::from_bytes(&[0xF6])?.kind(), K::SystemCommon);
    Ok(())
}

#[test]
fn b1_timestamps_and_views() -> R {
    let mut m = M::note_on(1, 60, 1)?.with_timestamp(1.5);
    assert_eq!(m.timestamp(), 1.5);
    m.set_timestamp(48.0);
    assert_eq!(m.timestamp(), 48.0);
    assert_eq!(m.as_view(), m.view(0));
    let v = m.view(12);
    assert_eq!(
        (v.bytes(), v.sample_offset(), v.as_slice()),
        (m.bytes(), 12, m.bytes())
    );
    assert_eq!(v.to_message(), m.clone().with_timestamp(12.0));
    fn rt_safe<T: RtSafe>(_: T) {}
    rt_safe(v); // views are usable on the audio thread
    Ok(())
}

#[test]
fn b1_view_payloads_outlive_the_view() -> R {
    let sysex = [0xF0, 0x7E, 0x01, 0xF7];
    let tempo = [0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20];
    let views = [
        MidiMessageRef::new(&sysex, 0)?,
        MidiMessageRef::new(&tempo, 1)?,
    ];
    let sysex_data: Vec<&[u8]> = views
        .iter()
        .copied()
        .filter_map(|e| e.sysex_data())
        .collect();
    let meta_data: Vec<&[u8]> = views.into_iter().filter_map(|e| e.meta_data()).collect();
    assert_eq!(sysex_data, [&[0x7E, 0x01][..]]);
    assert_eq!(meta_data, [&[0x07, 0xA1, 0x20][..]]);
    Ok(())
}

#[test]
fn b1_from_bytes_rejects_malformed_messages() {
    let bad: &[&[u8]] = &[
        &[],
        &[60, 100],             // running status (no status byte)
        &[0x90, 60],            // too short
        &[0x90, 60, 100, 1],    // too long
        &[0x90, 60, 0x80],      // status byte in data position
        &[0xC0, 1, 2],          // program change takes one data byte
        &[0xF0, 1, 2],          // unterminated SysEx
        &[0xF0, 1, 0x90, 0xF7], // status byte inside SysEx
        &[0xF7],                // stray end of SysEx
        &[0xF8, 0],             // real-time is one byte
        &[0xFF, 0x51, 0x03, 1], // meta length disagrees with data
        &[0xFF, 0x51, 0x80],    // truncated length field
    ];
    for b in bad {
        assert_eq!(M::from_bytes(b), Err(Malformed), "{b:02X?}");
        assert_eq!(MidiMessageRef::new(b, 0), Err(Malformed));
    }
    assert_eq!(Malformed.code().as_str(), "SYRP-MIDI-0001");
}

proptest! {
    #[test]
    fn b5_parsers_never_panic_on_random_bytes(
        bytes in proptest::collection::vec(any::<u8>(), 0..40),
        t in any::<u32>(),
    ) {
        let owned = M::from_bytes(&bytes);
        prop_assert_eq!(owned.is_ok(), MidiMessageRef::new(&bytes, t).is_ok());
        if let Ok(m) = owned {
            // Every accessor works on anything that parsed.
            let _ = (m.kind(), m.channel(), m.note_number(), m.velocity(), m.pitch_bend_value());
            let _ = (m.sysex_data(), m.meta_type(), m.meta_data(), m.controller_value());
            prop_assert_eq!(m.bytes(), &bytes[..]);
        }
    }

    #[test]
    fn b5_built_messages_parse_back_unchanged(
        ch in 1_u8..=16, a in 0_u8..128, b in 0_u8..128, bend in 0_u16..16384,
        data in proptest::collection::vec(0_u8..128, 0..300),
    ) {
        let built = [
            M::note_on(ch, a, b), M::note_off(ch, a, b), M::poly_pressure(ch, a, b),
            M::controller(ch, a, b), M::program_change(ch, a), M::channel_pressure(ch, a),
            M::pitch_bend(ch, bend), M::sysex(&data), M::meta(a, &data),
        ];
        for m in built {
            let m = m.unwrap();
            prop_assert_eq!(&M::from_bytes(m.bytes()).unwrap(), &m);
            prop_assert_eq!(m.channel().is_some(), !matches!(m.kind(), K::SysEx | K::Meta));
        }
    }
}

#[test]
fn b6_contract_types_have_no_public_fields() {
    let src = [
        include_str!("../src/lib.rs"),
        include_str!("../src/message.rs"),
        include_str!("../src/buffer.rs"),
        include_str!("../src/convert.rs"),
        include_str!("../src/ump.rs"),
        include_str!("../src/ump_buffer.rs"),
    ];
    let mut in_struct = false;
    for line in src.iter().flat_map(|s| s.lines()) {
        if line.starts_with("pub struct") {
            assert!(!line.contains("(pub"), "public tuple field: {line}");
            in_struct = line.ends_with('{');
        } else if line.starts_with('}') {
            in_struct = false;
        } else if in_struct {
            assert!(
                !line.trim_start().starts_with("pub"),
                "public field: {line}"
            );
        }
    }
}

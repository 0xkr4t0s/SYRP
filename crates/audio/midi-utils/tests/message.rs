// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Tests for work/specs/WP-2.1-07.md, increment 1: B1 (message helpers) and B5 (crate layout).

use proptest::prelude::*;
use syrp_midi::{MidiData, MidiError, MidiMessage, MidiMessageRef};
use syrp_midi_utils::message as m;

#[global_allocator]
static ALLOC: syrp_rt::GuardedAllocator = syrp_rt::GuardedAllocator;

#[test]
fn b1_tempo_meta_event_encodes_24_bit_microseconds() {
    let e = m::tempo_meta_event(0xFF_FFFF).unwrap();
    assert_eq!(e.bytes(), &[0xFF, 0x51, 3, 0xFF, 0xFF, 0xFF]);
    assert_eq!(
        m::tempo_meta_event(0x100_0000),
        Err(MidiError::ValueOutOfRange)
    );
    assert_eq!(
        m::tempo_seconds_per_quarter_note(&m::tempo_meta_event(1_000_000).unwrap()),
        Some(1.0)
    );
}

#[test]
fn b1_readers_reject_other_messages_and_impossible_data() {
    let note = MidiMessage::note_on(1, 60, 1).unwrap();
    assert_eq!(m::tempo_seconds_per_quarter_note(&note), None);
    assert_eq!(m::time_signature(&note), None);
    assert_eq!(m::key_signature(&note), None);
    assert_eq!(m::text(&note), None);
    assert!(!m::is_end_of_track(&note));
    // Wrong lengths and values no builder produces.
    let bad = [
        MidiMessage::meta(0x51, &[1, 2]).unwrap(),
        MidiMessage::meta(0x58, &[4, 32, 24, 8]).unwrap(),
        MidiMessage::meta(0x58, &[0, 2, 24, 8]).unwrap(),
        MidiMessage::meta(0x58, &[4, 2, 24]).unwrap(),
        MidiMessage::meta(0x59, &[8, 0]).unwrap(),
        MidiMessage::meta(0x59, &[0xF8, 0]).unwrap(),
        MidiMessage::meta(0x59, &[0, 2]).unwrap(),
        MidiMessage::meta(0x20, &[16]).unwrap(),
        MidiMessage::meta(0x2F, &[0]).unwrap(),
    ];
    for b in &bad {
        assert_eq!(m::tempo_microseconds_per_quarter(b), None, "{b:?}");
        assert_eq!(m::time_signature(b), None, "{b:?}");
        assert_eq!(m::key_signature(b), None, "{b:?}");
        assert_eq!(m::midi_channel_meta_event_channel(b), None, "{b:?}");
        assert!(!m::is_end_of_track(b), "{b:?}");
    }
}

#[test]
fn b1_time_signature_round_trips_and_rejects_bad_denominators() {
    let e = m::time_signature_meta_event(4, 4).unwrap();
    assert_eq!(e.bytes(), &[0xFF, 0x58, 4, 4, 2, 24, 8]);
    assert_eq!(m::time_signature(&e), Some((4, 4)));
    let big = m::time_signature_meta_event(7, 1 << 31).unwrap();
    assert_eq!(big.meta_data(), Some(&[7, 31, 1, 8][..]));
    assert_eq!(m::time_signature(&big), Some((7, 1 << 31)));
    for (n, d) in [(0, 4), (4, 0), (4, 3), (4, 12)] {
        assert_eq!(
            m::time_signature_meta_event(n, d),
            Err(MidiError::ValueOutOfRange)
        );
    }
}

#[test]
fn b1_key_signature_and_channel_prefix_check_ranges() {
    assert_eq!(
        m::key_signature(&m::key_signature_meta_event(-7, false).unwrap()),
        Some((-7, false))
    );
    assert_eq!(
        m::key_signature_meta_event(8, false),
        Err(MidiError::ValueOutOfRange)
    );
    assert_eq!(
        m::key_signature_meta_event(-8, true),
        Err(MidiError::ValueOutOfRange)
    );
    assert_eq!(
        m::midi_channel_meta_event(1).unwrap().bytes(),
        &[0xFF, 0x20, 1, 0]
    );
    assert_eq!(
        m::midi_channel_meta_event(0),
        Err(MidiError::ValueOutOfRange)
    );
    assert_eq!(
        m::midi_channel_meta_event(17),
        Err(MidiError::ValueOutOfRange)
    );
}

#[test]
fn b1_text_meta_events_cover_types_1_to_15() {
    for kind in 1..=15 {
        let e = m::text_meta_event(kind, "Grüße").unwrap();
        assert_eq!(e.meta_type(), Some(kind));
        assert_eq!(m::text(&e).as_deref(), Some("Grüße"));
    }
    assert_eq!(m::text_meta_event(0, "x"), Err(MidiError::ValueOutOfRange));
    assert_eq!(m::text(&MidiMessage::meta(0x7F, b"x").unwrap()), None);
    // Not UTF-8: replaced, not rejected.
    assert_eq!(
        m::text(&MidiMessage::meta(1, &[0x41, 0xFF]).unwrap()).as_deref(),
        Some("A\u{FFFD}")
    );
}

#[test]
fn b1_end_of_track_reads_through_borrowed_views() {
    let bytes = [0xFF, 0x2F, 0x00];
    let view = MidiMessageRef::new(&bytes, 3).unwrap();
    assert!(m::is_end_of_track(&view));
    assert_eq!(m::end_of_track().bytes(), &bytes);
}

#[test]
fn b1_float_velocity_scales_clamps_and_keeps_quiet_notes_on() {
    let vel = |v: f32| m::note_on_float(1, 60, v).unwrap().velocity().unwrap();
    assert_eq!(
        [vel(0.0), vel(0.5), vel(1.0), vel(2.0), vel(-1.0)],
        [0, 64, 127, 127, 0]
    );
    // Syrp's choice: any positive velocity is at least 1, so it stays a note on.
    assert_eq!(vel(0.001), 1);
    assert!(m::note_on_float(1, 60, 0.001).unwrap().is_note_on());
    assert_eq!(
        m::note_on_float(1, 60, f32::NAN),
        Err(MidiError::ValueOutOfRange)
    );
    assert_eq!(
        m::note_on_float(0, 60, 0.5),
        Err(MidiError::ValueOutOfRange)
    );
    let off = MidiMessage::note_off(1, 60, 127).unwrap();
    assert_eq!(m::float_velocity(&off), Some(1.0));
    assert_eq!(
        m::float_velocity(&MidiMessage::controller(1, 1, 1).unwrap()),
        None
    );
}

#[test]
fn b1_note_names_and_frequencies() {
    assert_eq!(m::note_name(61, true, true, 3).as_deref(), Some("C#3"));
    assert_eq!(m::note_name(61, false, true, 5).as_deref(), Some("Db5"));
    assert_eq!(m::note_name(127, true, true, 3).as_deref(), Some("G8"));
    assert_eq!(m::note_name(69, true, false, 3).as_deref(), Some("A"));
    assert_eq!(m::note_name(128, true, true, 3), None);
    assert_eq!(m::note_frequency(81, 440.0), 880.0);
    assert_eq!(m::note_frequency(57, 432.0), 216.0);
}

#[test]
fn b1_controller_and_general_midi_names() {
    assert_eq!(m::controller_name(0), Some("Bank Select"));
    assert_eq!(m::controller_name(42), Some("Pan (LSB)"));
    assert_eq!(m::controller_name(64), Some("Sustain Pedal"));
    assert_eq!(m::controller_name(127), Some("Poly Mode On"));
    assert_eq!(m::controller_name(102), None);
    assert_eq!(m::controller_name(200), None);
    assert_eq!(m::gm_instrument_name(127), Some("Gunshot"));
    assert_eq!(m::gm_instrument_name(40), Some("Violin"));
    assert_eq!(m::gm_instrument_category_name(15), Some("Sound Effects"));
    assert_eq!(m::gm_instrument_category_name(16), None);
    assert_eq!(m::gm_drum_name(35), Some("Acoustic Bass Drum"));
    assert_eq!(m::gm_drum_name(81), Some("Open Triangle"));
    assert_eq!(m::gm_drum_name(82), None);
    assert!((0..128).all(|p| m::gm_instrument_name(p).is_some_and(|s| !s.is_empty())));
}

#[test]
fn b1_name_lookups_and_readers_never_allocate() {
    let tempo = m::tempo_meta_event(500_000).unwrap();
    let names = syrp_rt::no_alloc(|| {
        let _ = m::tempo_seconds_per_quarter_note(&tempo);
        let _ = m::text(&tempo);
        (
            m::controller_name(1),
            m::gm_instrument_name(1),
            m::gm_drum_name(40),
        )
    });
    assert_eq!(
        names,
        (
            Some("Modulation Wheel"),
            Some("Bright Acoustic Piano"),
            Some("Electric Snare")
        )
    );
}

#[test]
fn b1_channel_mode_messages() {
    assert_eq!(m::all_notes_off(1).unwrap().bytes(), &[0xB0, 123, 0]);
    assert_eq!(m::all_sound_off(16).unwrap().bytes(), &[0xBF, 120, 0]);
    assert_eq!(m::all_controllers_off(3).unwrap().bytes(), &[0xB2, 121, 0]);
    assert_eq!(m::all_notes_off(17), Err(MidiError::ValueOutOfRange));
}

proptest! {
    #[test]
    fn b1_meta_builders_round_trip(
        us in 0_u32..=0xFF_FFFF,
        n in 1_u8..=255,
        power in 0_u32..32,
        sf in -7_i8..=7,
        minor: bool,
        kind in 1_u8..=15,
        text in ".{0,40}",
        channel in 1_u8..=16,
    ) {
        prop_assert_eq!(m::tempo_microseconds_per_quarter(&m::tempo_meta_event(us).unwrap()), Some(us));
        let ts = m::time_signature_meta_event(n, 1 << power).unwrap();
        prop_assert_eq!(m::time_signature(&ts), Some((n, 1 << power)));
        prop_assert_eq!(m::key_signature(&m::key_signature_meta_event(sf, minor).unwrap()), Some((sf, minor)));
        let t = m::text_meta_event(kind, &text).unwrap();
        let read = m::text(&t);
        prop_assert_eq!(read.as_deref(), Some(text.as_str()));
        let c = m::midi_channel_meta_event(channel).unwrap();
        prop_assert_eq!(m::midi_channel_meta_event_channel(&c), Some(channel));
    }

    #[test]
    fn b1_float_velocity_is_monotonic_and_round_trips_bytes(a in 0.0_f32..=1.0, b in 0.0_f32..=1.0, v in 0_u8..128) {
        let byte = |x: f32| m::note_on_float(1, 60, x).unwrap().velocity().unwrap();
        if a <= b {
            prop_assert!(byte(a) <= byte(b));
        }
        let back = m::float_velocity(&MidiMessage::note_on(1, 60, v).unwrap()).unwrap();
        prop_assert_eq!(byte(back), v);
    }
}

#[test]
fn b5_crate_layout_leaves_file_and_ump_to_their_work_packages() {
    let lib = include_str!("../src/lib.rs");
    let manifest = include_str!("../Cargo.toml");
    let has = |text: &str, line: &str| text.lines().any(|l| l.trim() == line);
    assert!(has(lib, "pub mod file;") && has(lib, "pub mod ump;"));
    assert!(has(manifest, "name = \"ump\"") && has(manifest, "harness = false"));
    let dev = manifest.split("[dev-dependencies]").nth(1).unwrap();
    assert!(dev.lines().any(|l| l.starts_with("proptest ")));
    assert!(dev.lines().any(|l| l.starts_with("syrp-rt ")));
    let deps = manifest
        .split("[dependencies]")
        .nth(1)
        .unwrap()
        .split("\n[")
        .next()
        .unwrap();
    let names: Vec<&str> = deps
        .lines()
        .filter_map(|l| l.split(' ').next())
        .filter(|n| !n.is_empty())
        .collect();
    assert_eq!(names, ["syrp-error", "syrp-midi", "syrp-rt"]);
}

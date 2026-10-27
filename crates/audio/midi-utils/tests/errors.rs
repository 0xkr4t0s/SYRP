// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Tests for work/specs/WP-2.1-07.md, increment 4: B6 (error codes and dependencies).

use syrp_error::Diagnostic;
use syrp_midi_utils::{KeyboardError, SequenceError};

/// The `errors/midi.toml` catalogue this crate continues.
const CATALOGUE: &str = include_str!("../../../../errors/midi.toml");

#[test]
fn b6_every_error_code_is_catalogued_and_new_codes_start_at_0005() {
    let codes = [
        SequenceError::IndexOutOfRange.code(),
        SequenceError::ValueOutOfRange.code(),
        SequenceError::OutOfMemory.code(),
        KeyboardError::ValueOutOfRange.code(),
        KeyboardError::QueueFull.code(),
        KeyboardError::BufferFull.code(),
    ];
    for code in codes {
        let entry = format!("code = \"{}\"", code.as_str());
        assert!(
            CATALOGUE.lines().any(|l| l.trim() == entry),
            "{entry} missing from errors/midi.toml"
        );
    }
    for new in ["SYRP-MIDI-0005", "SYRP-MIDI-0006", "SYRP-MIDI-0007"] {
        assert!(codes.iter().any(|c| c.as_str() == new));
    }
}

#[test]
fn b6_dependencies_are_syrp_midi_error_and_rt_only() {
    let manifest = include_str!("../Cargo.toml");
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

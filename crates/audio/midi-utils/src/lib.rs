// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Syrp MIDI utilities (master plan §5.2, layer L3): the parts of JUCE's `juce_audio_basics`
//! MIDI classes that sit on top of the frozen `syrp-midi` contract.
//!
//! - [`message`]: helpers over [`syrp_midi::MidiMessage`]: named meta events, float
//!   velocity, note names and frequencies, controller and General MIDI names.
//! - [`file`](mod@file): Standard MIDI Files (WP-2.1-08).
//! - [`ump`]: MIDI 1.0 and MIDI 2.0 translation and UMP dispatchers (WP-2.1-09).
//!
//! `MidiMessageSequence`, `MidiKeyboardState` and the RPN detector and generator arrive in
//! later increments of WP-2.1-07.
//!
//! ```
//! use syrp_midi::MidiData;
//! use syrp_midi_utils::message;
//! let m = message::note_on_float(10, 38, 0.5)?;
//! assert_eq!(m.velocity(), Some(64));
//! assert_eq!(message::gm_drum_name(38), Some("Acoustic Snare"));
//! # Ok::<(), syrp_midi::MidiError>(())
//! ```

pub mod file;
pub mod message;
pub mod ump;

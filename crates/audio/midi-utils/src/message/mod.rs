// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Helpers over [`syrp_midi::MidiMessage`] and [`MidiData`] for the parts of JUCE's
//! `MidiMessage` that the frozen `syrp-midi` contract leaves out: named meta events and their
//! readers, float velocities, note names and frequencies, controller and General MIDI names,
//! and channel mode messages.
//!
//! Builders return [`MidiError::ValueOutOfRange`] (`SYRP-MIDI-0002`) for arguments outside
//! their range, as the contract's constructors do. Readers take any [`MidiData`] (an owned
//! message or a borrowed view) and return `None` when the message is not of their kind or
//! holds data no builder here produces.
//!
//! ```
//! use syrp_midi_utils::message;
//! let tempo = message::tempo_meta_event(500_000)?;
//! assert_eq!(message::tempo_seconds_per_quarter_note(&tempo), Some(0.5));
//! assert_eq!(message::note_name(61, true, true, 3).as_deref(), Some("C#3"));
//! # Ok::<(), syrp_midi::MidiError>(())
//! ```

use std::borrow::Cow;

use syrp_midi::{MidiData, MidiError, MidiMessage};

mod names;

/// Meta event type of a tempo change.
const META_TEMPO: u8 = 0x51;
/// Meta event type of a time signature.
const META_TIME_SIGNATURE: u8 = 0x58;
/// Meta event type of a key signature.
const META_KEY_SIGNATURE: u8 = 0x59;
/// Meta event type of the end-of-track marker.
const META_END_OF_TRACK: u8 = 0x2F;
/// Meta event type of a MIDI channel prefix.
const META_CHANNEL_PREFIX: u8 = 0x20;

/// The data of a meta event of type `kind`, or `None` for any other message.
fn meta_of<M: MidiData + ?Sized>(message: &M, kind: u8) -> Option<&[u8]> {
    if message.meta_type()? == kind {
        message.meta_data()
    } else {
        None
    }
}

/// A tempo meta event (`FF 51 03 tt tt tt`), giving the length of a quarter note in
/// microseconds (at most `0xFF_FFFF`, the largest 24-bit value).
/// ```
/// use syrp_midi::MidiData;
/// let m = syrp_midi_utils::message::tempo_meta_event(500_000)?;
/// assert_eq!(m.bytes(), &[0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20]);
/// # Ok::<(), syrp_midi::MidiError>(())
/// ```
#[doc(alias = "tempoMetaEvent")]
pub fn tempo_meta_event(microseconds_per_quarter: u32) -> Result<MidiMessage, MidiError> {
    if microseconds_per_quarter > 0xFF_FFFF {
        return Err(MidiError::ValueOutOfRange);
    }
    let [_, a, b, c] = microseconds_per_quarter.to_be_bytes();
    MidiMessage::meta(META_TEMPO, &[a, b, c])
}

/// The microseconds per quarter note of a tempo meta event (exactly three data bytes).
/// ```
/// let m = syrp_midi_utils::message::tempo_meta_event(600_000)?;
/// assert_eq!(syrp_midi_utils::message::tempo_microseconds_per_quarter(&m), Some(600_000));
/// # Ok::<(), syrp_midi::MidiError>(())
/// ```
pub fn tempo_microseconds_per_quarter<M: MidiData + ?Sized>(message: &M) -> Option<u32> {
    match meta_of(message, META_TEMPO)? {
        [a, b, c] => Some(u32::from_be_bytes([0, *a, *b, *c])),
        _ => None,
    }
}

/// The length of a quarter note in seconds, read from a tempo meta event.
/// ```
/// let m = syrp_midi_utils::message::tempo_meta_event(250_000)?;
/// assert_eq!(syrp_midi_utils::message::tempo_seconds_per_quarter_note(&m), Some(0.25));
/// # Ok::<(), syrp_midi::MidiError>(())
/// ```
#[doc(alias = "getTempoSecondsPerQuarterNote")]
pub fn tempo_seconds_per_quarter_note<M: MidiData + ?Sized>(message: &M) -> Option<f64> {
    tempo_microseconds_per_quarter(message).map(|us| f64::from(us) / 1_000_000.0)
}

/// A time signature meta event (`FF 58 04 nn dd cc bb`). `numerator` is 1 to 255 and
/// `denominator` a power of two from 1 to 2^31.
///
/// The two bytes after the signature are Syrp's choice: one metronome click per
/// denominator beat (`96 / denominator` MIDI clocks, at least 1) and eight 32nd notes per
/// quarter note.
/// ```
/// use syrp_midi::MidiData;
/// let m = syrp_midi_utils::message::time_signature_meta_event(6, 8)?;
/// assert_eq!(m.bytes(), &[0xFF, 0x58, 0x04, 6, 3, 12, 8]);
/// # Ok::<(), syrp_midi::MidiError>(())
/// ```
#[doc(alias = "timeSignatureMetaEvent")]
pub fn time_signature_meta_event(
    numerator: u8,
    denominator: u32,
) -> Result<MidiMessage, MidiError> {
    if numerator == 0 || !denominator.is_power_of_two() {
        return Err(MidiError::ValueOutOfRange);
    }
    let power = denominator.trailing_zeros();
    let clocks = (96 / denominator).max(1);
    // Both fit in a byte: `power` is below 32 and `clocks` is at most 96.
    MidiMessage::meta(
        META_TIME_SIGNATURE,
        &[numerator, power as u8, clocks as u8, 8],
    )
}

/// The `(numerator, denominator)` of a time signature meta event (four data bytes, a
/// numerator of at least 1 and a denominator power below 32).
/// ```
/// let m = syrp_midi_utils::message::time_signature_meta_event(3, 4)?;
/// assert_eq!(syrp_midi_utils::message::time_signature(&m), Some((3, 4)));
/// # Ok::<(), syrp_midi::MidiError>(())
/// ```
#[doc(alias = "getTimeSignatureInfo")]
pub fn time_signature<M: MidiData + ?Sized>(message: &M) -> Option<(u8, u32)> {
    match meta_of(message, META_TIME_SIGNATURE)? {
        [n, d, _, _] if *n > 0 && *d < 32 => Some((*n, 1 << d)),
        _ => None,
    }
}

/// A key signature meta event (`FF 59 02 sf mi`): `sharps_or_flats` is -7 (seven flats) to
/// 7 (seven sharps), and `is_minor` selects a minor key.
/// ```
/// use syrp_midi::MidiData;
/// let m = syrp_midi_utils::message::key_signature_meta_event(-3, true)?;
/// assert_eq!(m.bytes(), &[0xFF, 0x59, 0x02, 0xFD, 1]);
/// # Ok::<(), syrp_midi::MidiError>(())
/// ```
#[doc(alias = "keySignatureMetaEvent")]
pub fn key_signature_meta_event(
    sharps_or_flats: i8,
    is_minor: bool,
) -> Result<MidiMessage, MidiError> {
    if !(-7..=7).contains(&sharps_or_flats) {
        return Err(MidiError::ValueOutOfRange);
    }
    MidiMessage::meta(
        META_KEY_SIGNATURE,
        &[sharps_or_flats.to_be_bytes()[0], u8::from(is_minor)],
    )
}

/// The `(sharps_or_flats, is_minor)` of a key signature meta event (two data bytes, a count
/// from -7 to 7 and a mode of 0 or 1).
/// ```
/// let m = syrp_midi_utils::message::key_signature_meta_event(2, false)?;
/// assert_eq!(syrp_midi_utils::message::key_signature(&m), Some((2, false)));
/// # Ok::<(), syrp_midi::MidiError>(())
/// ```
#[doc(alias = "getKeySignatureNumberOfSharpsOrFlats")]
#[doc(alias = "isKeySignatureMajorKey")]
pub fn key_signature<M: MidiData + ?Sized>(message: &M) -> Option<(i8, bool)> {
    match meta_of(message, META_KEY_SIGNATURE)? {
        [sf, mi @ (0 | 1)] => {
            let count = i8::from_be_bytes([*sf]);
            (-7..=7).contains(&count).then_some((count, *mi == 1))
        }
        _ => None,
    }
}

/// A text meta event of type `kind`, 1 to 15: 1 text, 2 copyright, 3 track name,
/// 4 instrument name, 5 lyric, 6 marker, 7 cue point; 8 to 15 are further text types the
/// Standard MIDI File format reserves. The text is stored as UTF-8.
/// ```
/// let m = syrp_midi_utils::message::text_meta_event(3, "Bass")?;
/// assert_eq!(syrp_midi_utils::message::text(&m).as_deref(), Some("Bass"));
/// assert!(syrp_midi_utils::message::text_meta_event(16, "x").is_err());
/// # Ok::<(), syrp_midi::MidiError>(())
/// ```
#[doc(alias = "textMetaEvent")]
pub fn text_meta_event(kind: u8, text: &str) -> Result<MidiMessage, MidiError> {
    if !(1..=15).contains(&kind) {
        return Err(MidiError::ValueOutOfRange);
    }
    MidiMessage::meta(kind, text.as_bytes())
}

/// The text of a text meta event (types 1 to 15). Bytes that are not UTF-8 are replaced with
/// `U+FFFD`, so text written by other tools in another encoding still reads.
/// ```
/// let m = syrp_midi::MidiMessage::meta(0x05, b"la")?;
/// assert_eq!(syrp_midi_utils::message::text(&m).as_deref(), Some("la"));
/// # Ok::<(), syrp_midi::MidiError>(())
/// ```
#[doc(alias = "getTextFromTextMetaEvent")]
pub fn text<M: MidiData + ?Sized>(message: &M) -> Option<Cow<'_, str>> {
    if (1..=15).contains(&message.meta_type()?) {
        message.meta_data().map(String::from_utf8_lossy)
    } else {
        None
    }
}

/// The end-of-track meta event, `FF 2F 00`.
/// ```
/// use syrp_midi::MidiData;
/// assert_eq!(syrp_midi_utils::message::end_of_track().bytes(), &[0xFF, 0x2F, 0x00]);
/// ```
#[doc(alias = "endOfTrack")]
#[must_use]
pub fn end_of_track() -> MidiMessage {
    MidiMessage::meta(META_END_OF_TRACK, &[]).expect("type 0x2F with no data is a valid meta event")
}

/// True for the end-of-track meta event.
/// ```
/// assert!(syrp_midi_utils::message::is_end_of_track(&syrp_midi_utils::message::end_of_track()));
/// ```
#[doc(alias = "isEndOfTrackMetaEvent")]
pub fn is_end_of_track<M: MidiData + ?Sized>(message: &M) -> bool {
    meta_of(message, META_END_OF_TRACK).is_some_and(<[u8]>::is_empty)
}

/// A MIDI channel prefix meta event (`FF 20 01 cc`) for a channel numbered 1 to 16.
/// ```
/// use syrp_midi::MidiData;
/// let m = syrp_midi_utils::message::midi_channel_meta_event(10)?;
/// assert_eq!(m.bytes(), &[0xFF, 0x20, 0x01, 9]);
/// # Ok::<(), syrp_midi::MidiError>(())
/// ```
#[doc(alias = "midiChannelMetaEvent")]
pub fn midi_channel_meta_event(channel: u8) -> Result<MidiMessage, MidiError> {
    if !(1..=16).contains(&channel) {
        return Err(MidiError::ValueOutOfRange);
    }
    MidiMessage::meta(META_CHANNEL_PREFIX, &[channel - 1])
}

/// The channel (1 to 16) of a MIDI channel prefix meta event.
/// ```
/// let m = syrp_midi_utils::message::midi_channel_meta_event(16)?;
/// assert_eq!(syrp_midi_utils::message::midi_channel_meta_event_channel(&m), Some(16));
/// # Ok::<(), syrp_midi::MidiError>(())
/// ```
#[doc(alias = "getMidiChannelMetaEventChannel")]
pub fn midi_channel_meta_event_channel<M: MidiData + ?Sized>(message: &M) -> Option<u8> {
    match meta_of(message, META_CHANNEL_PREFIX)? {
        [c @ 0..=15] => Some(c + 1),
        _ => None,
    }
}

/// Converts a velocity from 0.0 to 1.0 into a 7-bit MIDI value.
///
/// Values are clamped to 0.0..=1.0 and scaled by 127 with rounding. Syrp's choice: a velocity
/// above zero never rounds down to 0, so a quiet note on stays a note on. NaN is out of range.
fn velocity_byte(velocity: f32) -> Result<u8, MidiError> {
    if velocity.is_nan() {
        return Err(MidiError::ValueOutOfRange);
    }
    let v = velocity.clamp(0.0, 1.0);
    // In 0.0..=127.0 after clamping, so the cast is exact.
    let byte = (v * 127.0).round() as u8;
    Ok(if v > 0.0 { byte.max(1) } else { 0 })
}

/// A note on whose velocity is given from 0.0 to 1.0 (clamped), scaled to 0 to 127.
/// A velocity above zero gives at least 1 (Syrp's choice); NaN is
/// [`MidiError::ValueOutOfRange`].
/// ```
/// use syrp_midi::MidiData;
/// let m = syrp_midi_utils::message::note_on_float(1, 60, 1.0)?;
/// assert_eq!(m.velocity(), Some(127));
/// assert_eq!(syrp_midi_utils::message::float_velocity(&m), Some(1.0));
/// # Ok::<(), syrp_midi::MidiError>(())
/// ```
#[doc(alias = "noteOn")]
pub fn note_on_float(channel: u8, note: u8, velocity: f32) -> Result<MidiMessage, MidiError> {
    MidiMessage::note_on(channel, note, velocity_byte(velocity)?)
}

/// The velocity of a note on or note off as 0.0 to 1.0 (the 7-bit value divided by 127).
/// ```
/// let m = syrp_midi::MidiMessage::note_off(1, 60, 0)?;
/// assert_eq!(syrp_midi_utils::message::float_velocity(&m), Some(0.0));
/// # Ok::<(), syrp_midi::MidiError>(())
/// ```
#[doc(alias = "getFloatVelocity")]
pub fn float_velocity<M: MidiData + ?Sized>(message: &M) -> Option<f32> {
    message.velocity().map(|v| f32::from(v) / 127.0)
}

/// Note names within an octave, spelled with sharps.
const SHARP_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];
/// Note names within an octave, spelled with flats.
const FLAT_NAMES: [&str; 12] = [
    "C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B",
];

/// The name of MIDI note `note` (0 to 127), such as `"C#3"`, or `None` above 127.
///
/// `use_sharps` spells black keys as sharps (`C#`) rather than flats (`Db`).
/// `include_octave` appends the octave number, where middle C (note 60) is in octave
/// `middle_c_octave` (commonly 3, 4 or 5).
/// ```
/// use syrp_midi_utils::message::note_name;
/// assert_eq!(note_name(60, true, true, 4).as_deref(), Some("C4"));
/// assert_eq!(note_name(70, false, false, 4).as_deref(), Some("Bb"));
/// assert_eq!(note_name(0, true, true, 3).as_deref(), Some("C-2"));
/// ```
#[doc(alias = "getMidiNoteName")]
#[must_use]
pub fn note_name(
    note: u8,
    use_sharps: bool,
    include_octave: bool,
    middle_c_octave: i32,
) -> Option<String> {
    if note > 127 {
        return None;
    }
    let names = if use_sharps {
        &SHARP_NAMES
    } else {
        &FLAT_NAMES
    };
    let name = names[usize::from(note % 12)];
    Some(if include_octave {
        let octave = i32::from(note / 12) - 5 + middle_c_octave;
        format!("{name}{octave}")
    } else {
        name.to_owned()
    })
}

/// The frequency in hertz of MIDI note `note` in twelve-tone equal temperament, with note 69
/// (A above middle C) at `a4_hz`.
/// ```
/// use syrp_midi_utils::message::note_frequency;
/// assert_eq!(note_frequency(69, 440.0), 440.0);
/// assert!((note_frequency(60, 440.0) - 261.6256).abs() < 1e-4);
/// ```
#[doc(alias = "getMidiNoteInHertz")]
#[must_use]
pub fn note_frequency(note: u8, a4_hz: f64) -> f64 {
    a4_hz * ((f64::from(note) - 69.0) / 12.0).exp2()
}

/// The name of control change number `n`, or `None` when the MIDI 1.0 specification gives it
/// no standard assignment (or `n` is above 127). Fine (LSB) partners of named coarse
/// controllers are named too, as `"Pan (LSB)"`.
/// ```
/// use syrp_midi_utils::message::controller_name;
/// assert_eq!(controller_name(7), Some("Channel Volume"));
/// assert_eq!(controller_name(3), None);
/// ```
#[doc(alias = "getControllerName")]
#[must_use]
pub fn controller_name(n: u8) -> Option<&'static str> {
    names::CONTROLLERS
        .get(usize::from(n))
        .copied()
        .filter(|s| !s.is_empty())
}

/// The General MIDI Level 1 name of program `program` (0 to 127).
/// ```
/// assert_eq!(syrp_midi_utils::message::gm_instrument_name(0), Some("Acoustic Grand Piano"));
/// assert_eq!(syrp_midi_utils::message::gm_instrument_name(128), None);
/// ```
#[doc(alias = "getGMInstrumentName")]
#[must_use]
pub fn gm_instrument_name(program: u8) -> Option<&'static str> {
    names::GM_PROGRAMS.get(usize::from(program)).copied()
}

/// The General MIDI family name of family `category` (0 to 15; program `p` is in family
/// `p / 8`).
/// ```
/// assert_eq!(syrp_midi_utils::message::gm_instrument_category_name(0), Some("Piano"));
/// ```
#[doc(alias = "getGMInstrumentBankName")]
#[must_use]
pub fn gm_instrument_category_name(category: u8) -> Option<&'static str> {
    names::GM_CATEGORIES.get(usize::from(category)).copied()
}

/// The General MIDI percussion name of key `note` (35 to 81) on the percussion channel 10.
/// ```
/// assert_eq!(syrp_midi_utils::message::gm_drum_name(38), Some("Acoustic Snare"));
/// assert_eq!(syrp_midi_utils::message::gm_drum_name(34), None);
/// ```
#[doc(alias = "getRhythmInstrumentName")]
#[must_use]
pub fn gm_drum_name(note: u8) -> Option<&'static str> {
    let index = note.checked_sub(names::FIRST_DRUM)?;
    names::GM_DRUMS.get(usize::from(index)).copied()
}

/// An All Notes Off channel mode message (controller 123, value 0) for a channel 1 to 16.
/// ```
/// use syrp_midi::MidiData;
/// assert_eq!(syrp_midi_utils::message::all_notes_off(2)?.bytes(), &[0xB1, 123, 0]);
/// # Ok::<(), syrp_midi::MidiError>(())
/// ```
#[doc(alias = "allNotesOff")]
pub fn all_notes_off(channel: u8) -> Result<MidiMessage, MidiError> {
    MidiMessage::controller(channel, 123, 0)
}

/// An All Sound Off channel mode message (controller 120, value 0) for a channel 1 to 16.
/// ```
/// use syrp_midi::MidiData;
/// assert_eq!(syrp_midi_utils::message::all_sound_off(1)?.bytes(), &[0xB0, 120, 0]);
/// # Ok::<(), syrp_midi::MidiError>(())
/// ```
#[doc(alias = "allSoundOff")]
pub fn all_sound_off(channel: u8) -> Result<MidiMessage, MidiError> {
    MidiMessage::controller(channel, 120, 0)
}

/// A Reset All Controllers channel mode message (controller 121, value 0) for a channel 1 to
/// 16.
/// ```
/// use syrp_midi::MidiData;
/// assert_eq!(syrp_midi_utils::message::all_controllers_off(16)?.bytes(), &[0xBF, 121, 0]);
/// # Ok::<(), syrp_midi::MidiError>(())
/// ```
#[doc(alias = "allControllersOff")]
pub fn all_controllers_off(channel: u8) -> Result<MidiMessage, MidiError> {
    MidiMessage::controller(channel, 121, 0)
}

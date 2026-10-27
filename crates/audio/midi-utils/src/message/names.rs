// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Name tables for controllers, General MIDI programs, program categories and percussion keys.
//!
//! The lists follow the MIDI 1.0 control change table and the General MIDI Level 1 sound set
//! and percussion map published by the MIDI Association. The wording is Syrp's.

/// Control change names, indexed by controller number; `""` where the number has no
/// standard assignment.
#[rustfmt::skip]
pub(super) const CONTROLLERS: [&str; 128] = [
    // 0..=31: coarse (MSB) controllers.
    "Bank Select", "Modulation Wheel", "Breath Controller", "",
    "Foot Controller", "Portamento Time", "Data Entry", "Channel Volume",
    "Balance", "", "Pan", "Expression",
    "Effect Control 1", "Effect Control 2", "", "",
    "General Purpose Controller 1", "General Purpose Controller 2",
    "General Purpose Controller 3", "General Purpose Controller 4",
    "", "", "", "", "", "", "", "", "", "", "", "",
    // 32..=63: fine (LSB) partners of 0..=31.
    "Bank Select (LSB)", "Modulation Wheel (LSB)", "Breath Controller (LSB)", "",
    "Foot Controller (LSB)", "Portamento Time (LSB)", "Data Entry (LSB)",
    "Channel Volume (LSB)", "Balance (LSB)", "", "Pan (LSB)", "Expression (LSB)",
    "Effect Control 1 (LSB)", "Effect Control 2 (LSB)", "", "",
    "General Purpose Controller 1 (LSB)", "General Purpose Controller 2 (LSB)",
    "General Purpose Controller 3 (LSB)", "General Purpose Controller 4 (LSB)",
    "", "", "", "", "", "", "", "", "", "", "", "",
    // 64..=95: switches, sound controllers and effect depths.
    "Sustain Pedal", "Portamento On/Off", "Sostenuto Pedal", "Soft Pedal",
    "Legato Footswitch", "Hold 2", "Sound Variation", "Timbre / Harmonic Content",
    "Release Time", "Attack Time", "Brightness", "Decay Time",
    "Vibrato Rate", "Vibrato Depth", "Vibrato Delay", "Sound Controller 10",
    "General Purpose Controller 5", "General Purpose Controller 6",
    "General Purpose Controller 7", "General Purpose Controller 8",
    "Portamento Control", "", "", "",
    "High Resolution Velocity Prefix", "", "", "Reverb Send Level",
    "Tremolo Depth", "Chorus Send Level", "Celeste (Detune) Depth", "Phaser Depth",
    // 96..=127: parameter numbers and channel mode messages.
    "Data Increment", "Data Decrement",
    "Non-Registered Parameter Number (LSB)", "Non-Registered Parameter Number (MSB)",
    "Registered Parameter Number (LSB)", "Registered Parameter Number (MSB)",
    "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "",
    "All Sound Off", "Reset All Controllers", "Local Control", "All Notes Off",
    "Omni Mode Off", "Omni Mode On", "Mono Mode On", "Poly Mode On",
];

/// General MIDI Level 1 program names, indexed by program number 0 to 127.
#[rustfmt::skip]
pub(super) const GM_PROGRAMS: [&str; 128] = [
    "Acoustic Grand Piano", "Bright Acoustic Piano", "Electric Grand Piano", "Honky-tonk Piano",
    "Electric Piano 1", "Electric Piano 2", "Harpsichord", "Clavinet",
    "Celesta", "Glockenspiel", "Music Box", "Vibraphone",
    "Marimba", "Xylophone", "Tubular Bells", "Dulcimer",
    "Drawbar Organ", "Percussive Organ", "Rock Organ", "Church Organ",
    "Reed Organ", "Accordion", "Harmonica", "Tango Accordion",
    "Acoustic Guitar (nylon)", "Acoustic Guitar (steel)", "Electric Guitar (jazz)",
    "Electric Guitar (clean)", "Electric Guitar (muted)", "Overdriven Guitar",
    "Distortion Guitar", "Guitar Harmonics",
    "Acoustic Bass", "Electric Bass (finger)", "Electric Bass (pick)", "Fretless Bass",
    "Slap Bass 1", "Slap Bass 2", "Synth Bass 1", "Synth Bass 2",
    "Violin", "Viola", "Cello", "Contrabass",
    "Tremolo Strings", "Pizzicato Strings", "Orchestral Harp", "Timpani",
    "String Ensemble 1", "String Ensemble 2", "Synth Strings 1", "Synth Strings 2",
    "Choir Aahs", "Voice Oohs", "Synth Voice", "Orchestra Hit",
    "Trumpet", "Trombone", "Tuba", "Muted Trumpet",
    "French Horn", "Brass Section", "Synth Brass 1", "Synth Brass 2",
    "Soprano Sax", "Alto Sax", "Tenor Sax", "Baritone Sax",
    "Oboe", "English Horn", "Bassoon", "Clarinet",
    "Piccolo", "Flute", "Recorder", "Pan Flute",
    "Blown Bottle", "Shakuhachi", "Whistle", "Ocarina",
    "Lead 1 (square)", "Lead 2 (sawtooth)", "Lead 3 (calliope)", "Lead 4 (chiff)",
    "Lead 5 (charang)", "Lead 6 (voice)", "Lead 7 (fifths)", "Lead 8 (bass + lead)",
    "Pad 1 (new age)", "Pad 2 (warm)", "Pad 3 (polysynth)", "Pad 4 (choir)",
    "Pad 5 (bowed)", "Pad 6 (metallic)", "Pad 7 (halo)", "Pad 8 (sweep)",
    "FX 1 (rain)", "FX 2 (soundtrack)", "FX 3 (crystal)", "FX 4 (atmosphere)",
    "FX 5 (brightness)", "FX 6 (goblins)", "FX 7 (echoes)", "FX 8 (sci-fi)",
    "Sitar", "Banjo", "Shamisen", "Koto",
    "Kalimba", "Bagpipe", "Fiddle", "Shanai",
    "Tinkle Bell", "Agogo", "Steel Drums", "Woodblock",
    "Taiko Drum", "Melodic Tom", "Synth Drum", "Reverse Cymbal",
    "Guitar Fret Noise", "Breath Noise", "Seashore", "Bird Tweet",
    "Telephone Ring", "Helicopter", "Applause", "Gunshot",
];

/// General MIDI program families: program `p` belongs to family `p / 8`.
#[rustfmt::skip]
pub(super) const GM_CATEGORIES: [&str; 16] = [
    "Piano", "Chromatic Percussion", "Organ", "Guitar",
    "Bass", "Strings", "Ensemble", "Brass",
    "Reed", "Pipe", "Synth Lead", "Synth Pad",
    "Synth Effects", "Ethnic", "Percussive", "Sound Effects",
];

/// The first key of the General MIDI percussion map.
pub(super) const FIRST_DRUM: u8 = 35;

/// General MIDI percussion names for keys [`FIRST_DRUM`] onwards.
#[rustfmt::skip]
pub(super) const GM_DRUMS: [&str; 47] = [
    "Acoustic Bass Drum", "Bass Drum 1", "Side Stick", "Acoustic Snare",
    "Hand Clap", "Electric Snare", "Low Floor Tom", "Closed Hi-Hat",
    "High Floor Tom", "Pedal Hi-Hat", "Low Tom", "Open Hi-Hat",
    "Low-Mid Tom", "Hi-Mid Tom", "Crash Cymbal 1", "High Tom",
    "Ride Cymbal 1", "Chinese Cymbal", "Ride Bell", "Tambourine",
    "Splash Cymbal", "Cowbell", "Crash Cymbal 2", "Vibraslap",
    "Ride Cymbal 2", "Hi Bongo", "Low Bongo", "Mute Hi Conga",
    "Open Hi Conga", "Low Conga", "High Timbale", "Low Timbale",
    "High Agogo", "Low Agogo", "Cabasa", "Maracas",
    "Short Whistle", "Long Whistle", "Short Guiro", "Long Guiro",
    "Claves", "Hi Wood Block", "Low Wood Block", "Mute Cuica",
    "Open Cuica", "Mute Triangle", "Open Triangle",
];

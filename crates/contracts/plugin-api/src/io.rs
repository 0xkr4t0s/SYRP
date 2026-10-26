// SPDX-License-Identifier: MIT OR Apache-2.0
//! `ProcessIo`: one block's audio, parameter changes and MIDI/UMP events.

use syrp_audio_api::AudioBlockMut;
use syrp_midi::{MidiBuffer, UmpBuffer};

use crate::ParamChanges;

/// An empty change stream for blocks built without one.
static NO_CHANGES: ParamChanges = ParamChanges::new();

/// The audio buffer of one block, in the precision the processor was prepared for.
#[derive(Debug)]
enum Audio<'a> {
    Single(AudioBlockMut<'a, f32>),
    Double(AudioBlockMut<'a, f64>),
}

/// One block's inputs and outputs: in-place audio, the parameter change stream, and MIDI 1.0
/// and UMP event input and output.
///
/// The audio buffer holds the inputs in its first [`input_channels`](Self::input_channels)
/// channels on entry and the outputs on return. Events carry sample offsets into the block.
/// ```
/// let mut data = [0.0f32; 8];
/// let audio = syrp_audio_api::AudioBlockMut::new(&mut data, 2, 4, 4).unwrap();
/// let io = syrp_plugin_api::ProcessIo::new(audio, 2);
/// assert_eq!((io.channels(), io.frames(), io.param_changes().len()), (2, 4, 0));
/// ```
#[derive(Debug)]
pub struct ProcessIo<'a> {
    audio: Audio<'a>,
    input_channels: usize,
    changes: &'a ParamChanges,
    midi_in: Option<&'a MidiBuffer>,
    midi_out: Option<&'a mut MidiBuffer>,
    ump_in: Option<&'a UmpBuffer>,
    ump_out: Option<&'a mut UmpBuffer>,
}

impl<'a> ProcessIo<'a> {
    /// Single-precision I/O over `audio`, whose first `input_channels` channels (clamped to
    /// the channel count) hold input.
    /// ```
    /// let mut d = [0.0f32; 1];
    /// let io = syrp_plugin_api::ProcessIo::new(syrp_audio_api::AudioBlockMut::new(&mut d, 1, 1, 1).unwrap(), 9);
    /// assert_eq!(io.input_channels(), 1);
    /// ```
    #[must_use]
    pub fn new(audio: AudioBlockMut<'a, f32>, input_channels: usize) -> Self {
        let inputs = input_channels.min(audio.channels());
        Self::with_audio(Audio::Single(audio), inputs)
    }

    /// Double-precision I/O; see [`new`](Self::new).
    /// ```
    /// let mut d = [0.0f64; 2];
    /// let mut io = syrp_plugin_api::ProcessIo::new_f64(syrp_audio_api::AudioBlockMut::new(&mut d, 1, 2, 2).unwrap(), 1);
    /// assert!(io.audio_f64().is_some() && io.audio_f32().is_none());
    /// ```
    #[must_use]
    pub fn new_f64(audio: AudioBlockMut<'a, f64>, input_channels: usize) -> Self {
        let inputs = input_channels.min(audio.channels());
        Self::with_audio(Audio::Double(audio), inputs)
    }

    fn with_audio(audio: Audio<'a>, input_channels: usize) -> Self {
        Self {
            audio,
            input_channels,
            changes: &NO_CHANGES,
            midi_in: None,
            midi_out: None,
            ump_in: None,
            ump_out: None,
        }
    }

    /// Attaches this block's parameter change stream.
    /// ```
    /// # let mut d = [0.0f32; 1]; let audio = syrp_audio_api::AudioBlockMut::new(&mut d, 1, 1, 1).unwrap();
    /// let changes = syrp_plugin_api::ParamChanges::new();
    /// let io = syrp_plugin_api::ProcessIo::new(audio, 1).with_param_changes(&changes);
    /// assert!(io.param_changes().is_empty());
    /// ```
    #[must_use]
    pub fn with_param_changes(mut self, changes: &'a ParamChanges) -> Self {
        self.changes = changes;
        self
    }

    /// Attaches MIDI 1.0 event input and output.
    /// ```
    /// # let mut d = [0.0f32; 1]; let audio = syrp_audio_api::AudioBlockMut::new(&mut d, 1, 1, 1).unwrap();
    /// let (input, mut output) = (syrp_midi::MidiBuffer::new(), syrp_midi::MidiBuffer::new());
    /// let io = syrp_plugin_api::ProcessIo::new(audio, 1).with_midi(&input, &mut output);
    /// assert!(io.midi_in().is_some());
    /// ```
    #[must_use]
    pub fn with_midi(mut self, input: &'a MidiBuffer, output: &'a mut MidiBuffer) -> Self {
        self.midi_in = Some(input);
        self.midi_out = Some(output);
        self
    }

    /// Attaches Universal MIDI Packet event input and output.
    /// ```
    /// # let mut d = [0.0f32; 1]; let audio = syrp_audio_api::AudioBlockMut::new(&mut d, 1, 1, 1).unwrap();
    /// let (input, mut output) = (syrp_midi::UmpBuffer::new(), syrp_midi::UmpBuffer::new());
    /// let io = syrp_plugin_api::ProcessIo::new(audio, 1).with_ump(&input, &mut output);
    /// assert!(io.ump_in().is_some());
    /// ```
    #[must_use]
    pub fn with_ump(mut self, input: &'a UmpBuffer, output: &'a mut UmpBuffer) -> Self {
        self.ump_in = Some(input);
        self.ump_out = Some(output);
        self
    }

    /// Frames in this block.
    /// ```
    /// # let mut d = [0.0f32; 3];
    /// let io = syrp_plugin_api::ProcessIo::new(syrp_audio_api::AudioBlockMut::new(&mut d, 1, 3, 3).unwrap(), 1);
    /// assert_eq!(io.frames(), 3);
    /// ```
    #[must_use]
    pub fn frames(&self) -> usize {
        match &self.audio {
            Audio::Single(a) => a.frames(),
            Audio::Double(a) => a.frames(),
        }
    }

    /// Channels in the buffer: the larger of the input and output channel counts.
    /// ```
    /// # let mut d = [0.0f32; 2];
    /// let io = syrp_plugin_api::ProcessIo::new(syrp_audio_api::AudioBlockMut::new(&mut d, 2, 1, 1).unwrap(), 0);
    /// assert_eq!(io.channels(), 2);
    /// ```
    #[must_use]
    pub fn channels(&self) -> usize {
        match &self.audio {
            Audio::Single(a) => a.channels(),
            Audio::Double(a) => a.channels(),
        }
    }

    /// How many leading channels held input on entry.
    /// ```
    /// # let mut d = [0.0f32; 2];
    /// let io = syrp_plugin_api::ProcessIo::new(syrp_audio_api::AudioBlockMut::new(&mut d, 2, 1, 1).unwrap(), 1);
    /// assert_eq!(io.input_channels(), 1);
    /// ```
    #[must_use]
    pub fn input_channels(&self) -> usize {
        self.input_channels
    }

    /// The `f32` audio buffer, or `None` for a double-precision block.
    /// ```
    /// # let mut d = [1.0f32; 1];
    /// let mut io = syrp_plugin_api::ProcessIo::new(syrp_audio_api::AudioBlockMut::new(&mut d, 1, 1, 1).unwrap(), 1);
    /// io.audio_f32().unwrap().fill(0.5);
    /// # drop(io); assert_eq!(d, [0.5]);
    /// ```
    pub fn audio_f32(&mut self) -> Option<AudioBlockMut<'_, f32>> {
        match &mut self.audio {
            Audio::Single(a) => Some(a.reborrow()),
            Audio::Double(_) => None,
        }
    }

    /// The `f64` audio buffer, or `None` for a single-precision block.
    /// ```
    /// # let mut d = [1.0f32; 1];
    /// let mut io = syrp_plugin_api::ProcessIo::new(syrp_audio_api::AudioBlockMut::new(&mut d, 1, 1, 1).unwrap(), 1);
    /// assert!(io.audio_f64().is_none());
    /// ```
    pub fn audio_f64(&mut self) -> Option<AudioBlockMut<'_, f64>> {
        match &mut self.audio {
            Audio::Double(a) => Some(a.reborrow()),
            Audio::Single(_) => None,
        }
    }

    /// This block's parameter changes, sorted by sample offset (empty when none attached).
    /// ```
    /// # let mut d = [0.0f32; 1];
    /// let io = syrp_plugin_api::ProcessIo::new(syrp_audio_api::AudioBlockMut::new(&mut d, 1, 1, 1).unwrap(), 1);
    /// assert!(io.param_changes().is_empty());
    /// ```
    #[must_use]
    pub fn param_changes(&self) -> &'a ParamChanges {
        self.changes
    }

    /// The MIDI 1.0 input events, if the block carries MIDI.
    /// ```
    /// # let mut d = [0.0f32; 1];
    /// let io = syrp_plugin_api::ProcessIo::new(syrp_audio_api::AudioBlockMut::new(&mut d, 1, 1, 1).unwrap(), 1);
    /// assert!(io.midi_in().is_none());
    /// ```
    #[must_use]
    pub fn midi_in(&self) -> Option<&'a MidiBuffer> {
        self.midi_in
    }

    /// The MIDI 1.0 output buffer. Add events with its non-allocating `try_add_event`.
    /// ```
    /// # let mut d = [0.0f32; 1];
    /// let mut io = syrp_plugin_api::ProcessIo::new(syrp_audio_api::AudioBlockMut::new(&mut d, 1, 1, 1).unwrap(), 1);
    /// assert!(io.midi_out().is_none());
    /// ```
    pub fn midi_out(&mut self) -> Option<&mut MidiBuffer> {
        self.midi_out.as_deref_mut()
    }

    /// The UMP input events, if the block carries UMP.
    /// ```
    /// # let mut d = [0.0f32; 1];
    /// let io = syrp_plugin_api::ProcessIo::new(syrp_audio_api::AudioBlockMut::new(&mut d, 1, 1, 1).unwrap(), 1);
    /// assert!(io.ump_in().is_none());
    /// ```
    #[must_use]
    pub fn ump_in(&self) -> Option<&'a UmpBuffer> {
        self.ump_in
    }

    /// The UMP output buffer. Add packets with its non-allocating `try_add`.
    /// ```
    /// # let mut d = [0.0f32; 1];
    /// let mut io = syrp_plugin_api::ProcessIo::new(syrp_audio_api::AudioBlockMut::new(&mut d, 1, 1, 1).unwrap(), 1);
    /// assert!(io.ump_out().is_none());
    /// ```
    pub fn ump_out(&mut self) -> Option<&mut UmpBuffer> {
        self.ump_out.as_deref_mut()
    }
}

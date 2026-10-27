// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`ProcessContext`]: the blocks and bypass flag one `process` call works on.

use std::marker::PhantomData;

use syrp_audio_api::{AudioBlock, AudioBlockMut};

use crate::DspSample;

/// An input block and an output block, borrowed together.
type InputOutput<'b, E> = (AudioBlock<'b, E>, AudioBlockMut<'b, E>);

/// Where the audio lives: one block for input and output, or two.
enum Blocks<'a, E> {
    Replacing(AudioBlockMut<'a, E>),
    NonReplacing {
        input: AudioBlock<'a, E>,
        output: AudioBlockMut<'a, E>,
    },
}

/// The audio and flags passed to [`DspProcessor::process`](crate::DspProcessor::process).
///
/// A *replacing* context (JUCE's `dsp::ProcessContextReplacing`) holds one block that is both
/// input and output. A *non-replacing* one (JUCE's `dsp::ProcessContextNonReplacing`) holds a
/// separate input block and output block. Both carry the `is_bypassed` flag: when it is set a
/// processor is expected to do whatever is appropriate for it in a bypassed state, usually
/// passing the input through.
///
/// The blocks hold `S::Element` (`f32` or `f64`); `S` is the sample type the processor
/// computes in. Nothing here allocates.
///
/// ```
/// use syrp_audio_api::{AudioBlock, AudioBlockMut};
/// use syrp_dsp_core::ProcessContext;
/// let input = [1.0_f32, 2.0];
/// let mut output = [0.0_f32; 2];
/// let mut ctx = ProcessContext::<f32>::non_replacing(
///     AudioBlock::new(&input, 1, 2, 2)?,
///     AudioBlockMut::new(&mut output, 1, 2, 2)?,
/// );
/// assert!(ctx.uses_separate_input_and_output_blocks());
/// ctx.copy_input_to_output();
/// assert_eq!(output, [1.0, 2.0]);
/// # Ok::<(), syrp_audio_api::AudioError>(())
/// ```
#[doc(alias = "dsp::ProcessContextReplacing")]
#[doc(alias = "dsp::ProcessContextNonReplacing")]
pub struct ProcessContext<'a, S: DspSample> {
    blocks: Blocks<'a, S::Element>,
    bypassed: bool,
    _sample: PhantomData<fn() -> S>,
}

impl<'a, S: DspSample> ProcessContext<'a, S> {
    /// A replacing context: `block` is both the input and the output. Not bypassed.
    ///
    /// ```
    /// use syrp_audio_api::AudioBlockMut;
    /// let mut data = [0.0_f64; 4];
    /// let ctx = syrp_dsp_core::ProcessContext::<f64>::replacing(AudioBlockMut::new(&mut data, 2, 2, 2)?);
    /// assert!(!ctx.uses_separate_input_and_output_blocks() && !ctx.is_bypassed());
    /// # Ok::<(), syrp_audio_api::AudioError>(())
    /// ```
    #[must_use]
    pub fn replacing(block: AudioBlockMut<'a, S::Element>) -> Self {
        Self {
            blocks: Blocks::Replacing(block),
            bypassed: false,
            _sample: PhantomData,
        }
    }

    /// A non-replacing context reading `input` and writing `output`. Not bypassed. The
    /// blocks may differ in shape; a processor decides what to do with extra channels.
    ///
    /// ```
    /// use syrp_audio_api::{AudioBlock, AudioBlockMut};
    /// let (input, mut output) = ([0.5_f32; 2], [0.0_f32; 4]);
    /// let ctx = syrp_dsp_core::ProcessContext::<f32>::non_replacing(
    ///     AudioBlock::new(&input, 1, 2, 2)?, AudioBlockMut::new(&mut output, 2, 2, 2)?);
    /// assert_eq!(ctx.input_block().channels(), 1);
    /// # Ok::<(), syrp_audio_api::AudioError>(())
    /// ```
    #[must_use]
    pub fn non_replacing(
        input: AudioBlock<'a, S::Element>,
        output: AudioBlockMut<'a, S::Element>,
    ) -> Self {
        Self {
            blocks: Blocks::NonReplacing { input, output },
            bypassed: false,
            _sample: PhantomData,
        }
    }

    /// True for a non-replacing context.
    ///
    /// ```
    /// use syrp_audio_api::AudioBlockMut;
    /// let mut d = [0.0_f32; 1];
    /// let ctx = syrp_dsp_core::ProcessContext::<f32>::replacing(AudioBlockMut::new(&mut d, 1, 1, 1)?);
    /// assert!(!ctx.uses_separate_input_and_output_blocks());
    /// # Ok::<(), syrp_audio_api::AudioError>(())
    /// ```
    #[must_use]
    pub fn uses_separate_input_and_output_blocks(&self) -> bool {
        matches!(self.blocks, Blocks::NonReplacing { .. })
    }

    /// Whether the processor should behave as bypassed.
    ///
    /// ```
    /// use syrp_audio_api::AudioBlockMut;
    /// let mut d = [0.0_f32; 1];
    /// let mut ctx = syrp_dsp_core::ProcessContext::<f32>::replacing(AudioBlockMut::new(&mut d, 1, 1, 1)?);
    /// ctx.set_bypassed(true);
    /// assert!(ctx.is_bypassed());
    /// # Ok::<(), syrp_audio_api::AudioError>(())
    /// ```
    #[must_use]
    pub fn is_bypassed(&self) -> bool {
        self.bypassed
    }

    /// Sets the bypass flag.
    ///
    /// ```
    /// use syrp_audio_api::AudioBlockMut;
    /// let mut d = [0.0_f32; 1];
    /// let mut ctx = syrp_dsp_core::ProcessContext::<f32>::replacing(AudioBlockMut::new(&mut d, 1, 1, 1)?);
    /// ctx.set_bypassed(true);
    /// ctx.set_bypassed(false);
    /// assert!(!ctx.is_bypassed());
    /// # Ok::<(), syrp_audio_api::AudioError>(())
    /// ```
    pub fn set_bypassed(&mut self, bypassed: bool) {
        self.bypassed = bypassed;
    }

    /// The input: the shared block of a replacing context, else the input block.
    ///
    /// ```
    /// use syrp_audio_api::AudioBlockMut;
    /// let mut d = [3.0_f32; 2];
    /// let ctx = syrp_dsp_core::ProcessContext::<f32>::replacing(AudioBlockMut::new(&mut d, 1, 2, 2)?);
    /// assert_eq!(ctx.input_block().channel(0), &[3.0, 3.0]);
    /// # Ok::<(), syrp_audio_api::AudioError>(())
    /// ```
    #[must_use]
    pub fn input_block(&self) -> AudioBlock<'_, S::Element> {
        match &self.blocks {
            Blocks::Replacing(block) => block.as_block(),
            Blocks::NonReplacing { input, .. } => *input,
        }
    }

    /// The output: the shared block of a replacing context, else the output block.
    ///
    /// ```
    /// use syrp_audio_api::AudioBlockMut;
    /// let mut d = [0.0_f32; 2];
    /// let mut ctx = syrp_dsp_core::ProcessContext::<f32>::replacing(AudioBlockMut::new(&mut d, 1, 2, 2)?);
    /// ctx.output_block().fill(0.25);
    /// assert_eq!(d, [0.25; 2]);
    /// # Ok::<(), syrp_audio_api::AudioError>(())
    /// ```
    pub fn output_block(&mut self) -> AudioBlockMut<'_, S::Element> {
        match &mut self.blocks {
            Blocks::Replacing(block) | Blocks::NonReplacing { output: block, .. } => {
                block.reborrow()
            }
        }
    }

    /// The input and output blocks together, for a non-replacing context; `None` for a
    /// replacing one (whose input is its output).
    ///
    /// ```
    /// use syrp_audio_api::{AudioBlock, AudioBlockMut};
    /// let (input, mut output) = ([2.0_f32; 2], [0.0_f32; 2]);
    /// let mut ctx = syrp_dsp_core::ProcessContext::<f32>::non_replacing(
    ///     AudioBlock::new(&input, 1, 2, 2)?, AudioBlockMut::new(&mut output, 1, 2, 2)?);
    /// if let Some((i, mut o)) = ctx.separate_blocks() {
    ///     o.channel_mut(0)[0] = i.channel(0)[0] * 4.0;
    /// }
    /// assert_eq!(output[0], 8.0);
    /// # Ok::<(), syrp_audio_api::AudioError>(())
    /// ```
    pub fn separate_blocks(&mut self) -> Option<InputOutput<'_, S::Element>> {
        match &mut self.blocks {
            Blocks::Replacing(_) => None,
            Blocks::NonReplacing { input, output } => Some((*input, output.reborrow())),
        }
    }

    /// For a non-replacing context, copies the input into the output over the channels and
    /// frames both blocks have; output samples outside that region are left as they are.
    /// Does nothing for a replacing context. Processors that work in place call this first.
    ///
    /// ```
    /// use syrp_audio_api::{AudioBlock, AudioBlockMut};
    /// let (input, mut output) = ([1.0_f32, 2.0, 3.0], [9.0_f32; 2]);
    /// let mut ctx = syrp_dsp_core::ProcessContext::<f32>::non_replacing(
    ///     AudioBlock::new(&input, 1, 3, 3)?, AudioBlockMut::new(&mut output, 1, 2, 2)?);
    /// ctx.copy_input_to_output();
    /// assert_eq!(output, [1.0, 2.0]);
    /// # Ok::<(), syrp_audio_api::AudioError>(())
    /// ```
    pub fn copy_input_to_output(&mut self) {
        if let Blocks::NonReplacing { input, output } = &mut self.blocks {
            let channels = input.channels().min(output.channels());
            let frames = input.frames().min(output.frames());
            for ch in 0..channels {
                output.channel_mut(ch)[..frames].copy_from_slice(&input.channel(ch)[..frames]);
            }
        }
    }

    /// A replacing context over this context's output block, with the same bypass flag.
    /// [`ProcessorChain`](crate::ProcessorChain) runs every processor after the first one
    /// through it.
    ///
    /// ```
    /// use syrp_audio_api::{AudioBlock, AudioBlockMut};
    /// let (input, mut output) = ([1.0_f32], [0.0_f32]);
    /// let mut ctx = syrp_dsp_core::ProcessContext::<f32>::non_replacing(
    ///     AudioBlock::new(&input, 1, 1, 1)?, AudioBlockMut::new(&mut output, 1, 1, 1)?);
    /// let inner = ctx.as_replacing();
    /// assert!(!inner.uses_separate_input_and_output_blocks());
    /// # Ok::<(), syrp_audio_api::AudioError>(())
    /// ```
    pub fn as_replacing(&mut self) -> ProcessContext<'_, S> {
        let bypassed = self.bypassed;
        let mut ctx = ProcessContext::replacing(self.output_block());
        ctx.bypassed = bypassed;
        ctx
    }
}

impl<S: DspSample> std::fmt::Debug for ProcessContext<'_, S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (input, output) = match &self.blocks {
            Blocks::Replacing(b) => (None, b),
            Blocks::NonReplacing { input, output } => (Some(input), output),
        };
        f.debug_struct("ProcessContext")
            .field("input", &input)
            .field("output", output)
            .field("bypassed", &self.bypassed)
            .finish()
    }
}

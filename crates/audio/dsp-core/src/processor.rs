// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`DspProcessor`], the processor model.

use crate::{DspError, DspSample, DspSpec, ProcessContext};

/// A DSP processor: the interface every `syrp-dsp-*` processor implements (JUCE's
/// `dsp::ProcessorBase` and the duck-typed processor shape `ProcessorChain` expects).
///
/// `S` is the sample type the processor computes in; the blocks it receives hold
/// `S::Element`. Lifecycle: [`prepare`](Self::prepare) off the audio thread, then any number
/// of [`process`](Self::process) and [`reset`](Self::reset) calls on the audio thread.
///
/// # Real-time safety
/// `process` and `reset` must not allocate, free, lock or do I/O. `prepare` may allocate and
/// reports allocation failure as an error instead of aborting.
///
/// `Box<P>` (including `Box<dyn DspProcessor<S>>`) is a processor too, which is how a
/// processor is chosen at run time.
///
/// ```
/// use syrp_audio_api::{AudioBlockMut, ProcessConfig};
/// use syrp_dsp_core::{DspError, DspProcessor, DspSpec, ProcessContext};
/// /// Writes silence unless bypassed.
/// struct Mute;
/// impl DspProcessor<f32> for Mute {
///     fn prepare(&mut self, _: &DspSpec) -> Result<(), DspError> { Ok(()) }
///     fn reset(&mut self) {}
///     fn process(&mut self, ctx: &mut ProcessContext<'_, f32>) {
///         if ctx.is_bypassed() { ctx.copy_input_to_output() } else { ctx.output_block().fill(0.0) }
///     }
/// }
/// let mut p: Box<dyn DspProcessor<f32>> = Box::new(Mute);
/// p.prepare(&DspSpec::new(ProcessConfig::new(48_000.0, 4)?, 1))?;
/// let mut d = [1.0_f32; 4];
/// p.process(&mut ProcessContext::replacing(AudioBlockMut::new(&mut d, 1, 4, 4)?));
/// assert_eq!(d, [0.0; 4]);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[doc(alias = "dsp::ProcessorBase")]
pub trait DspProcessor<S: DspSample>: Send {
    /// Prepares for `spec`: allocates whatever `process` needs. Called off the audio thread,
    /// before the first `process` and again whenever the spec changes.
    ///
    /// ```
    /// # use syrp_audio_api::ProcessConfig;
    /// # use syrp_dsp_core::{DspError, DspProcessor, DspSpec, ProcessContext};
    /// struct Delay(Vec<f32>);
    /// impl DspProcessor<f32> for Delay {
    ///     fn prepare(&mut self, spec: &DspSpec) -> Result<(), DspError> {
    ///         self.0 = vec![0.0; spec.max_block_size()];
    ///         Ok(())
    ///     }
    ///     fn reset(&mut self) { self.0.fill(0.0) }
    ///     fn process(&mut self, _: &mut ProcessContext<'_, f32>) {}
    /// }
    /// let mut d = Delay(Vec::new());
    /// d.prepare(&DspSpec::new(ProcessConfig::new(48_000.0, 16)?, 1))?;
    /// assert_eq!(d.0.len(), 16);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    fn prepare(&mut self, spec: &DspSpec) -> Result<(), DspError>;

    /// Clears internal state (delay lines, filter memories) without changing parameters.
    /// Must not allocate.
    ///
    /// ```
    /// # use syrp_dsp_core::{DspError, DspProcessor, DspSpec, ProcessContext};
    /// struct Counter(u32);
    /// impl DspProcessor<f32> for Counter {
    ///     fn prepare(&mut self, _: &DspSpec) -> Result<(), DspError> { Ok(()) }
    ///     fn reset(&mut self) { self.0 = 0 }
    ///     fn process(&mut self, _: &mut ProcessContext<'_, f32>) { self.0 += 1 }
    /// }
    /// let mut c = Counter(7);
    /// c.reset();
    /// assert_eq!(c.0, 0);
    /// ```
    fn reset(&mut self);

    /// Processes one block. Must not allocate.
    ///
    /// ```
    /// # use syrp_audio_api::AudioBlockMut;
    /// # use syrp_dsp_core::{DspError, DspProcessor, DspSpec, ProcessContext};
    /// struct Invert;
    /// impl DspProcessor<f64> for Invert {
    ///     fn prepare(&mut self, _: &DspSpec) -> Result<(), DspError> { Ok(()) }
    ///     fn reset(&mut self) {}
    ///     fn process(&mut self, ctx: &mut ProcessContext<'_, f64>) {
    ///         ctx.copy_input_to_output();
    ///         for ch in ctx.output_block().split_channels_mut() { ch.iter_mut().for_each(|s| *s = -*s) }
    ///     }
    /// }
    /// let mut d = [1.0_f64, -2.0];
    /// Invert.process(&mut ProcessContext::replacing(AudioBlockMut::new(&mut d, 1, 2, 2)?));
    /// assert_eq!(d, [-1.0, 2.0]);
    /// # Ok::<(), syrp_audio_api::AudioError>(())
    /// ```
    fn process(&mut self, ctx: &mut ProcessContext<'_, S>);
}

impl<S: DspSample, P: DspProcessor<S> + ?Sized> DspProcessor<S> for Box<P> {
    fn prepare(&mut self, spec: &DspSpec) -> Result<(), DspError> {
        (**self).prepare(spec)
    }
    fn reset(&mut self) {
        (**self).reset();
    }
    fn process(&mut self, ctx: &mut ProcessContext<'_, S>) {
        (**self).process(ctx);
    }
}

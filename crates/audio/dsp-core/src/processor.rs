// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`DspProcessor`], the processor model, and [`ProcessorWrapper`].

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

/// The `prepare` hook of a [`ProcessorWrapper`].
type PrepareFn<P> = fn(&mut P, &DspSpec) -> Result<(), DspError>;

/// Adapts a value that has no [`DspProcessor`] impl (a type from another crate, or plain
/// state) into a processor, from a `process` closure plus optional `prepare` and `reset`
/// functions (JUCE's `dsp::ProcessorWrapper`).
///
/// Without hooks, `prepare` succeeds and `reset` does nothing. The closure runs on the audio
/// thread and must not allocate.
///
/// ```
/// use syrp_audio_api::{AudioBlockMut, ProcessConfig};
/// use syrp_dsp_core::{DspProcessor, DspSpec, ProcessContext, ProcessorWrapper};
/// /// A type from elsewhere: it knows nothing about Syrp's processor model.
/// struct Level { gain: f32 }
/// let mut p = ProcessorWrapper::new(Level { gain: 0.5 }, |lvl: &mut Level, ctx: &mut ProcessContext<'_, f32>| {
///     ctx.copy_input_to_output();
///     for ch in ctx.output_block().split_channels_mut() { ch.iter_mut().for_each(|s| *s *= lvl.gain) }
/// })
/// .with_reset(|lvl| lvl.gain = 1.0);
/// p.prepare(&DspSpec::new(ProcessConfig::new(48_000.0, 2)?, 1))?;
/// let mut d = [2.0_f32; 2];
/// p.process(&mut ProcessContext::replacing(AudioBlockMut::new(&mut d, 1, 2, 2)?));
/// assert_eq!(d, [1.0; 2]);
/// p.reset();
/// assert_eq!(p.get().gain, 1.0);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[doc(alias = "dsp::ProcessorWrapper")]
pub struct ProcessorWrapper<P, F> {
    inner: P,
    process: F,
    prepare: Option<PrepareFn<P>>,
    reset: Option<fn(&mut P)>,
}

impl<P, F> ProcessorWrapper<P, F> {
    /// Wraps `inner`, processing with `process`.
    ///
    /// ```
    /// use syrp_dsp_core::{ProcessContext, ProcessorWrapper};
    /// let w = ProcessorWrapper::new(3_u32, |_: &mut u32, _: &mut ProcessContext<'_, f32>| {});
    /// assert_eq!(*w.get(), 3);
    /// ```
    #[must_use]
    pub fn new<S>(inner: P, process: F) -> Self
    where
        S: DspSample,
        F: FnMut(&mut P, &mut ProcessContext<'_, S>) + Send,
    {
        Self {
            inner,
            process,
            prepare: None,
            reset: None,
        }
    }

    /// Runs `prepare` on the wrapped value in [`DspProcessor::prepare`].
    ///
    /// ```
    /// use syrp_audio_api::ProcessConfig;
    /// use syrp_dsp_core::{DspProcessor, DspSpec, ProcessContext, ProcessorWrapper};
    /// let mut w = ProcessorWrapper::new(0.0_f64, |_: &mut f64, _: &mut ProcessContext<'_, f32>| {})
    ///     .with_prepare(|sr, spec| { *sr = spec.sample_rate(); Ok(()) });
    /// w.prepare(&DspSpec::new(ProcessConfig::new(22_050.0, 8)?, 1))?;
    /// assert_eq!(*w.get(), 22_050.0);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub fn with_prepare(mut self, prepare: PrepareFn<P>) -> Self {
        self.prepare = Some(prepare);
        self
    }

    /// Runs `reset` on the wrapped value in [`DspProcessor::reset`]. It must not allocate.
    ///
    /// ```
    /// use syrp_dsp_core::{DspProcessor, ProcessContext, ProcessorWrapper};
    /// let mut w = ProcessorWrapper::new(5_i32, |_: &mut i32, _: &mut ProcessContext<'_, f32>| {})
    ///     .with_reset(|n| *n = 0);
    /// w.reset();
    /// assert_eq!(*w.get(), 0);
    /// ```
    #[must_use]
    pub fn with_reset(mut self, reset: fn(&mut P)) -> Self {
        self.reset = Some(reset);
        self
    }

    /// The wrapped value.
    ///
    /// ```
    /// use syrp_dsp_core::{ProcessContext, ProcessorWrapper};
    /// let w = ProcessorWrapper::new("state", |_: &mut &str, _: &mut ProcessContext<'_, f32>| {});
    /// assert_eq!(*w.get(), "state");
    /// ```
    #[must_use]
    pub fn get(&self) -> &P {
        &self.inner
    }

    /// The wrapped value, mutably.
    ///
    /// ```
    /// use syrp_dsp_core::{ProcessContext, ProcessorWrapper};
    /// let mut w = ProcessorWrapper::new(1_u8, |_: &mut u8, _: &mut ProcessContext<'_, f32>| {});
    /// *w.get_mut() = 2;
    /// assert_eq!(*w.get(), 2);
    /// ```
    pub fn get_mut(&mut self) -> &mut P {
        &mut self.inner
    }

    /// Unwraps the value.
    ///
    /// ```
    /// use syrp_dsp_core::{ProcessContext, ProcessorWrapper};
    /// let w = ProcessorWrapper::new(vec![1, 2], |_: &mut Vec<i32>, _: &mut ProcessContext<'_, f32>| {});
    /// assert_eq!(w.into_inner(), [1, 2]);
    /// ```
    #[must_use]
    pub fn into_inner(self) -> P {
        self.inner
    }
}

impl<S, P, F> DspProcessor<S> for ProcessorWrapper<P, F>
where
    S: DspSample,
    P: Send,
    F: FnMut(&mut P, &mut ProcessContext<'_, S>) + Send,
{
    fn prepare(&mut self, spec: &DspSpec) -> Result<(), DspError> {
        self.prepare.map_or(Ok(()), |f| f(&mut self.inner, spec))
    }

    fn reset(&mut self) {
        if let Some(f) = self.reset {
            f(&mut self.inner);
        }
    }

    fn process(&mut self, ctx: &mut ProcessContext<'_, S>) {
        (self.process)(&mut self.inner, ctx);
    }
}

impl<P: std::fmt::Debug, F> std::fmt::Debug for ProcessorWrapper<P, F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessorWrapper")
            .field("inner", &self.inner)
            .field("has_prepare", &self.prepare.is_some())
            .field("has_reset", &self.reset.is_some())
            .finish()
    }
}

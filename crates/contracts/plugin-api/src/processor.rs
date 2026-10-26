// SPDX-License-Identifier: MIT OR Apache-2.0
//! `Processor` and its capability tokens, `PrepareCtx` and `RtCtx`.

use std::marker::PhantomData;

use syrp_audio_api::{
    BusLayout, ChannelSet, LayoutResponse, PlayHeadInfo, Precision, ProcessConfig,
};

use crate::{ParamIndex, ParamLayout, Params, PluginError, ProcessIo};

/// Change-stream slots reserved per registered parameter unless `prepare` asks for more.
const DEFAULT_CHANGES_PER_PARAM: usize = 8;

/// How long a processor keeps producing output after its input falls silent.
/// ```
/// assert_eq!(syrp_plugin_api::Tail::default(), syrp_plugin_api::Tail::None);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
#[doc(alias = "getTailLengthSeconds")]
pub enum Tail {
    /// Output stops when input stops.
    #[default]
    None,
    /// Output continues for this many samples at the prepared rate.
    Samples(u64),
    /// Output may never stop (an oscillator, an infinite reverb).
    Infinite,
}

/// An audio processor: the one trait native and ported code both implement.
///
/// On the message thread the framework calls [`register_params`](Self::register_params),
/// negotiates the layout, then calls [`prepare_processor`], which lends [`prepare`](Self::prepare)
/// a [`PrepareCtx`]. Then an audio thread calls [`process`](Self::process) once per block.
/// `process` is given no `PrepareCtx` and must not allocate, lock or block.
/// ```
/// use syrp_audio_api::ProcessConfig;
/// use syrp_plugin_api::*;
///
/// struct Gain(ParamIndex);
/// impl Processor for Gain {
///     fn register_params(&mut self, layout: &mut ParamLayout) -> Result<(), PluginError> {
///         self.0 = layout.register(ParamSpec::new("gain", "Gain", ParamRange::new(0.0, 1.0)))?;
///         Ok(())
///     }
///     fn prepare(&mut self, _: &ProcessConfig, cx: &mut PrepareCtx<'_>) -> Result<(), PluginError> {
///         self.0 = cx.param_index("gain").ok_or(PluginError::PrepareFailed)?;
///         Ok(())
///     }
///     fn process(&mut self, io: &mut ProcessIo<'_>, params: &Params, _: &mut RtCtx<'_>) {
///         let g = params.get(self.0);
///         if let Some(mut audio) = io.audio_f32() {
///             audio.split_channels_mut().for_each(|ch| ch.iter_mut().for_each(|s| *s *= g));
///         }
///     }
/// }
/// ```
#[doc(alias = "AudioProcessor")]
pub trait Processor: Send + 'static {
    /// Registers the parameters, in the order that fixes their indices. Default: none.
    /// ```
    /// # use syrp_plugin_api::*; use syrp_audio_api::ProcessConfig; struct P; impl Processor for P { fn prepare(&mut self, _: &ProcessConfig, _: &mut PrepareCtx<'_>) -> Result<(), PluginError> { Ok(()) } fn process(&mut self, _: &mut ProcessIo<'_>, _: &Params, _: &mut RtCtx<'_>) {} }
    /// let mut layout = ParamLayout::new();
    /// P.register_params(&mut layout).unwrap();
    /// assert!(layout.is_empty());
    /// ```
    fn register_params(&mut self, layout: &mut ParamLayout) -> Result<(), PluginError> {
        let _ = layout;
        Ok(())
    }

    /// The preferred layout. Default: one stereo input bus and one stereo output bus.
    /// ```
    /// # use syrp_plugin_api::*; use syrp_audio_api::ProcessConfig; struct P; impl Processor for P { fn prepare(&mut self, _: &ProcessConfig, _: &mut PrepareCtx<'_>) -> Result<(), PluginError> { Ok(()) } fn process(&mut self, _: &mut ProcessIo<'_>, _: &Params, _: &mut RtCtx<'_>) {} }
    /// assert_eq!(P.default_layout().output_channels(), 2);
    /// ```
    fn default_layout(&self) -> BusLayout {
        BusLayout::new()
            .with_input("Input", ChannelSet::stereo())
            .with_output("Output", ChannelSet::stereo())
    }

    /// Answers a host's proposed layout. The default accepts layouts with the channel sets of
    /// [`default_layout`](Self::default_layout) and proposes that layout otherwise.
    /// ```
    /// # use syrp_plugin_api::*; use syrp_audio_api::ProcessConfig; struct P; impl Processor for P { fn prepare(&mut self, _: &ProcessConfig, _: &mut PrepareCtx<'_>) -> Result<(), PluginError> { Ok(()) } fn process(&mut self, _: &mut ProcessIo<'_>, _: &Params, _: &mut RtCtx<'_>) {} }
    /// use syrp_audio_api::{BusLayout, ChannelSet, LayoutResponse};
    /// let mono = BusLayout::new().with_input("In", ChannelSet::mono()).with_output("Out", ChannelSet::mono());
    /// assert_eq!(P.supports_layout(&mono), LayoutResponse::Proposed(P.default_layout()));
    /// ```
    #[doc(alias = "isBusesLayoutSupported")]
    fn supports_layout(&self, layout: &BusLayout) -> LayoutResponse {
        let preferred = self.default_layout();
        if layout.same_channel_sets(&preferred) {
            LayoutResponse::Accepted
        } else {
            LayoutResponse::Proposed(preferred)
        }
    }

    /// Whether `process` handles `f64` audio. Default `false`, and then
    /// [`prepare_processor`] refuses a double-precision configuration.
    /// ```
    /// # use syrp_plugin_api::*; use syrp_audio_api::ProcessConfig; struct P; impl Processor for P { fn prepare(&mut self, _: &ProcessConfig, _: &mut PrepareCtx<'_>) -> Result<(), PluginError> { Ok(()) } fn process(&mut self, _: &mut ProcessIo<'_>, _: &Params, _: &mut RtCtx<'_>) {} }
    /// assert!(!P.supports_double_precision());
    /// ```
    #[doc(alias = "supportsDoublePrecisionProcessing")]
    fn supports_double_precision(&self) -> bool {
        false
    }

    /// Prepares for processing at `config`, on the message thread, never concurrently with
    /// `process`. Allocate audio-path memory here.
    /// ```
    /// # use syrp_plugin_api::*; use syrp_audio_api::ProcessConfig; struct P(Vec<f32>); impl Processor for P {
    /// fn prepare(&mut self, config: &ProcessConfig, _: &mut PrepareCtx<'_>) -> Result<(), PluginError> {
    ///     self.0 = vec![0.0; config.max_block_size()];
    ///     Ok(())
    /// }
    /// # fn process(&mut self, _: &mut ProcessIo<'_>, _: &Params, _: &mut RtCtx<'_>) {} }
    /// ```
    #[doc(alias = "prepareToPlay")]
    fn prepare(
        &mut self,
        config: &ProcessConfig,
        cx: &mut PrepareCtx<'_>,
    ) -> Result<(), PluginError>;

    /// Processes one block in place: `io` holds the inputs on entry and the outputs on return.
    /// Runs on an audio thread: no allocation, locks, blocking or I/O.
    /// ```
    /// # use syrp_plugin_api::*; use syrp_audio_api::ProcessConfig; struct P; impl Processor for P { fn prepare(&mut self, _: &ProcessConfig, _: &mut PrepareCtx<'_>) -> Result<(), PluginError> { Ok(()) }
    /// fn process(&mut self, io: &mut ProcessIo<'_>, _: &Params, _: &mut RtCtx<'_>) {
    ///     if let Some(mut audio) = io.audio_f32() { audio.fill(0.0); }
    /// }
    /// # }
    /// ```
    #[doc(alias = "processBlock")]
    fn process(&mut self, io: &mut ProcessIo<'_>, params: &Params, rt: &mut RtCtx<'_>);

    /// Processes one block while bypassed. The default leaves the input channels in place
    /// (passing audio through) and silences output-only channels; event output stays empty.
    /// ```
    /// # use syrp_plugin_api::*; use syrp_audio_api::ProcessConfig; struct P; impl Processor for P { fn prepare(&mut self, _: &ProcessConfig, _: &mut PrepareCtx<'_>) -> Result<(), PluginError> { Ok(()) } fn process(&mut self, _: &mut ProcessIo<'_>, _: &Params, _: &mut RtCtx<'_>) {} }
    /// let mut data = [1.0f32; 2];
    /// let audio = syrp_audio_api::AudioBlockMut::new(&mut data, 2, 1, 1).unwrap();
    /// let params = Params::new(&ParamLayout::new()).unwrap();
    /// P.process_bypassed(&mut ProcessIo::new(audio, 1), &params, &mut RtCtx::new());
    /// assert_eq!(data, [1.0, 0.0]);
    /// ```
    #[doc(alias = "processBlockBypassed")]
    fn process_bypassed(&mut self, io: &mut ProcessIo<'_>, params: &Params, rt: &mut RtCtx<'_>) {
        let _ = (params, rt);
        let outputs_only = io.input_channels()..io.channels();
        if let Some(mut audio) = io.audio_f32() {
            audio.slice_channels_mut(outputs_only).fill(0.0);
        } else if let Some(mut audio) = io.audio_f64() {
            audio.slice_channels_mut(outputs_only).fill(0.0);
        }
    }

    /// Clears internal state (delay lines, envelopes) without reallocating. RT-safe.
    /// Default: does nothing.
    /// ```
    /// # use syrp_plugin_api::*; use syrp_audio_api::ProcessConfig; struct P; impl Processor for P { fn prepare(&mut self, _: &ProcessConfig, _: &mut PrepareCtx<'_>) -> Result<(), PluginError> { Ok(()) } fn process(&mut self, _: &mut ProcessIo<'_>, _: &Params, _: &mut RtCtx<'_>) {} }
    /// P.reset();
    /// ```
    fn reset(&mut self) {}

    /// The latency the processor adds, in samples at the prepared rate. Default 0.
    /// ```
    /// # use syrp_plugin_api::*; use syrp_audio_api::ProcessConfig; struct P; impl Processor for P { fn prepare(&mut self, _: &ProcessConfig, _: &mut PrepareCtx<'_>) -> Result<(), PluginError> { Ok(()) } fn process(&mut self, _: &mut ProcessIo<'_>, _: &Params, _: &mut RtCtx<'_>) {} }
    /// assert_eq!(P.latency_samples(), 0);
    /// ```
    #[doc(alias = "getLatencySamples")]
    fn latency_samples(&self) -> u32 {
        0
    }

    /// How long output continues after input stops. Default [`Tail::None`].
    /// ```
    /// # use syrp_plugin_api::*; use syrp_audio_api::ProcessConfig; struct P; impl Processor for P { fn prepare(&mut self, _: &ProcessConfig, _: &mut PrepareCtx<'_>) -> Result<(), PluginError> { Ok(()) } fn process(&mut self, _: &mut ProcessIo<'_>, _: &Params, _: &mut RtCtx<'_>) {} }
    /// assert_eq!(P.tail(), Tail::None);
    /// ```
    fn tail(&self) -> Tail {
        Tail::None
    }

    /// The parameter hosts should use as the bypass switch (a value of at least 0.5 means
    /// bypassed), if the processor has one. Default `None`: the wrapper adds its own.
    /// ```
    /// # use syrp_plugin_api::*; use syrp_audio_api::ProcessConfig; struct P; impl Processor for P { fn prepare(&mut self, _: &ProcessConfig, _: &mut PrepareCtx<'_>) -> Result<(), PluginError> { Ok(()) } fn process(&mut self, _: &mut ProcessIo<'_>, _: &Params, _: &mut RtCtx<'_>) {} }
    /// assert_eq!(P.bypass_param(), None);
    /// ```
    #[doc(alias = "getBypassParameter")]
    fn bypass_param(&self) -> Option<ParamIndex> {
        None
    }
}

/// The prepare-time capability token, lent only to [`Processor::prepare`].
///
/// It resolves parameter string ids to indices and sizes per-block storage. Only
/// [`prepare_processor`] creates one, and it is borrowed for one `prepare` call, so it cannot
/// be kept for `process`.
/// ```
/// # use syrp_plugin_api::*; use syrp_audio_api::ProcessConfig; struct P; impl Processor for P {
/// fn prepare(&mut self, _: &ProcessConfig, cx: &mut PrepareCtx<'_>) -> Result<(), PluginError> {
///     cx.reserve_param_changes(256);
///     assert_eq!(cx.param_index("missing"), None);
///     Ok(())
/// }
/// # fn process(&mut self, _: &mut ProcessIo<'_>, _: &Params, _: &mut RtCtx<'_>) {} }
/// ```
#[derive(Debug)]
pub struct PrepareCtx<'a> {
    layout: &'a ParamLayout,
    param_change_capacity: usize,
    _not_send: PhantomData<*const ()>,
}

impl<'a> PrepareCtx<'a> {
    /// Resolves a parameter's string id to its index.
    /// ```
    /// # fn f(cx: &syrp_plugin_api::PrepareCtx<'_>) -> Option<syrp_plugin_api::ParamIndex> {
    /// cx.param_index("gain")
    /// # }
    /// ```
    #[must_use]
    pub fn param_index(&self, id: &str) -> Option<ParamIndex> {
        self.layout.index_of(id)
    }

    /// Asks for room for at least `capacity` parameter changes per block. Requests only raise
    /// the capacity, which starts at eight per registered parameter.
    /// ```
    /// # fn f(cx: &mut syrp_plugin_api::PrepareCtx<'_>) {
    /// cx.reserve_param_changes(1024);
    /// # }
    /// ```
    pub fn reserve_param_changes(&mut self, capacity: usize) {
        self.param_change_capacity = self.param_change_capacity.max(capacity);
    }
}

/// What [`prepare_processor`] reports to the host for sizing per-block storage.
/// ```
/// let p = syrp_plugin_api::Prepared::default();
/// let changes = syrp_plugin_api::ParamChanges::with_capacity(p.param_change_capacity());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Prepared {
    param_change_capacity: usize,
}

impl Prepared {
    /// The capacity for the host's [`ParamChanges`](crate::ParamChanges) stream.
    /// ```
    /// assert_eq!(syrp_plugin_api::Prepared::default().param_change_capacity(), 0);
    /// ```
    #[must_use]
    pub fn param_change_capacity(&self) -> usize {
        self.param_change_capacity
    }
}

/// Prepares `processor` for `config` on the message thread; the only way to create a
/// [`PrepareCtx`]. It is host-side: call it from framework or host code, never from `process`.
///
/// Fails with [`PluginError::PrecisionUnsupported`] if `config` asks for double precision
/// from a processor without it, or with whatever `prepare` returns.
/// ```
/// # use syrp_plugin_api::*; use syrp_audio_api::ProcessConfig; struct P; impl Processor for P { fn prepare(&mut self, _: &ProcessConfig, _: &mut PrepareCtx<'_>) -> Result<(), PluginError> { Ok(()) } fn process(&mut self, _: &mut ProcessIo<'_>, _: &Params, _: &mut RtCtx<'_>) {} }
/// let config = ProcessConfig::new(48_000.0, 512).unwrap();
/// let prepared = prepare_processor(&mut P, &config, &ParamLayout::new()).unwrap();
/// assert_eq!(prepared.param_change_capacity(), 0);
/// ```
pub fn prepare_processor<P: Processor + ?Sized>(
    processor: &mut P,
    config: &ProcessConfig,
    layout: &ParamLayout,
) -> Result<Prepared, PluginError> {
    if config.precision() == Precision::Double && !processor.supports_double_precision() {
        return Err(PluginError::PrecisionUnsupported);
    }
    let mut cx = PrepareCtx {
        layout,
        param_change_capacity: layout.len().saturating_mul(DEFAULT_CHANGES_PER_PARAM),
        _not_send: PhantomData,
    };
    processor.prepare(config, &mut cx)?;
    Ok(Prepared {
        param_change_capacity: cx.param_change_capacity,
    })
}

/// The audio-thread capability token, lent to [`Processor::process`].
///
/// It carries only RT-safe services, so far the transport position.
/// ```
/// use syrp_plugin_api::RtCtx;
/// let rt = RtCtx::new().with_play_head(syrp_audio_api::PlayHeadInfo::new().with_bpm(120.0));
/// assert_eq!(rt.play_head().and_then(|p| p.bpm()), Some(120.0));
/// ```
#[derive(Debug, Default)]
pub struct RtCtx<'a> {
    play_head: Option<PlayHeadInfo>,
    _not_send: PhantomData<&'a *const ()>,
}

impl RtCtx<'_> {
    /// A context with no transport position.
    /// ```
    /// assert!(syrp_plugin_api::RtCtx::new().play_head().is_none());
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets this block's transport position.
    /// ```
    /// let rt = syrp_plugin_api::RtCtx::new().with_play_head(syrp_audio_api::PlayHeadInfo::new());
    /// assert!(rt.play_head().is_some());
    /// ```
    #[must_use]
    pub fn with_play_head(mut self, info: PlayHeadInfo) -> Self {
        self.play_head = Some(info);
        self
    }

    /// This block's transport position, if the host reported one. RT-safe.
    /// ```
    /// assert_eq!(syrp_plugin_api::RtCtx::new().play_head(), None);
    /// ```
    #[must_use]
    pub fn play_head(&self) -> Option<PlayHeadInfo> {
        self.play_head
    }
}

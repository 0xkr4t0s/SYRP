// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`ProcessorDuplicator`]: one mono processor per channel, sharing a state object.

use crate::error::vec_with_capacity;
use crate::{DspError, DspProcessor, DspSample, DspSpec, ProcessContext};

/// A processor for independent channels, one channel per lane of [`Lanes`](Self::Lanes),
/// that reads a shared state `St` (coefficients, parameters). [`ProcessorDuplicator`] runs it
/// over every channel of a block.
///
/// Write it generic over the sample type (`struct OnePole<S: DspSample>`) and choose the pack
/// when building the duplicator: `OnePole<F32x8>` processes eight channels per instance, and
/// `OnePole<f32>` one. A processor that is not lane-generic sets `Lanes = f32` (or `f64`).
///
/// ```
/// use syrp_dsp_core::{DspError, DspSample, DspSpec, LaneProcessor};
/// use syrp_simd::{F32x4, Sample};
/// /// y[n] = x[n] + y[n-1] * feedback.
/// struct Leaky<S> { y: S }
/// impl<S: DspSample> LaneProcessor<S::Element> for Leaky<S> {
///     type Lanes = S;
///     fn prepare(&mut self, _: &DspSpec, _: &S::Element) -> Result<(), DspError> { Ok(()) }
///     fn reset(&mut self) { self.y = S::zero() }
///     fn process_lanes(&mut self, feedback: &S::Element, samples: &mut [S], _: bool) {
///         for x in samples {
///             self.y = *x + self.y * S::splat(*feedback);
///             *x = self.y;
///         }
///     }
/// }
/// let mut l = Leaky { y: F32x4::zero() };
/// let mut xs = [F32x4::one(), F32x4::zero()];
/// l.process_lanes(&0.5, &mut xs, false);
/// assert_eq!(xs[1].to_array(), [0.5; 4]);
/// ```
pub trait LaneProcessor<St>: Send {
    /// The sample type: lane `i` is one channel.
    type Lanes: DspSample;

    /// Prepares for `spec`, whose channel count is the number of channels this instance
    /// carries (at most `Lanes::LANES`). Called off the audio thread; may allocate.
    ///
    /// ```
    /// use syrp_audio_api::ProcessConfig;
    /// use syrp_dsp_core::{DspError, DspSpec, LaneProcessor};
    /// struct Rate(f64);
    /// impl LaneProcessor<()> for Rate {
    ///     type Lanes = f32;
    ///     fn prepare(&mut self, spec: &DspSpec, _: &()) -> Result<(), DspError> { self.0 = spec.sample_rate(); Ok(()) }
    ///     fn reset(&mut self) {}
    ///     fn process_lanes(&mut self, _: &(), _: &mut [f32], _: bool) {}
    /// }
    /// let mut r = Rate(0.0);
    /// r.prepare(&DspSpec::new(ProcessConfig::new(32_000.0, 4)?, 1), &())?;
    /// assert_eq!(r.0, 32_000.0);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    fn prepare(&mut self, spec: &DspSpec, state: &St) -> Result<(), DspError>;

    /// Clears internal state. Must not allocate.
    ///
    /// ```
    /// use syrp_dsp_core::{DspError, DspSpec, LaneProcessor};
    /// struct Hold(f32);
    /// impl LaneProcessor<()> for Hold {
    ///     type Lanes = f32;
    ///     fn prepare(&mut self, _: &DspSpec, _: &()) -> Result<(), DspError> { Ok(()) }
    ///     fn reset(&mut self) { self.0 = 0.0 }
    ///     fn process_lanes(&mut self, _: &(), _: &mut [f32], _: bool) {}
    /// }
    /// let mut h = Hold(1.0);
    /// h.reset();
    /// assert_eq!(h.0, 0.0);
    /// ```
    fn reset(&mut self);

    /// Processes `samples` in place: one value per frame, lane `i` holding channel `i` of the
    /// channels this instance carries (unused lanes hold zeros and are discarded).
    /// `bypassed` is the context's bypass flag. Must not allocate.
    ///
    /// ```
    /// use syrp_dsp_core::{DspError, DspSpec, LaneProcessor};
    /// struct Half;
    /// impl LaneProcessor<()> for Half {
    ///     type Lanes = f64;
    ///     fn prepare(&mut self, _: &DspSpec, _: &()) -> Result<(), DspError> { Ok(()) }
    ///     fn reset(&mut self) {}
    ///     fn process_lanes(&mut self, _: &(), xs: &mut [f64], bypassed: bool) {
    ///         if !bypassed { xs.iter_mut().for_each(|x| *x *= 0.5) }
    ///     }
    /// }
    /// let mut xs = [4.0_f64];
    /// Half.process_lanes(&(), &mut xs, false);
    /// assert_eq!(xs, [2.0]);
    /// ```
    fn process_lanes(&mut self, state: &St, samples: &mut [Self::Lanes], bypassed: bool);
}

/// The element type of `P`'s lanes.
type Elem<P, St> = <<P as LaneProcessor<St>>::Lanes as syrp_simd::Sample>::Element;

/// Runs a [`LaneProcessor`] over every channel of a block, with all instances sharing one
/// state object (JUCE's `dsp::ProcessorDuplicator` and `dsp::ProcessorState`).
///
/// When `P::Lanes` is a lane pack, channels are packed into lanes: channel `c` is lane
/// `c % LANES` of instance `c / LANES`, so `ceil(channels / LANES)` instances serve the
/// block. When `P::Lanes` is `f32` or `f64` there is one instance per channel, run on the
/// channel in place. This is the rule ADR 0003 D5 sets for duplicated processors.
///
/// [`prepare`](DspProcessor::prepare) (re)builds the instances with the factory and
/// allocates the packing scratch. `process` copies a non-replacing input to the output and
/// then works in place on the output; it never allocates. Channels beyond the prepared count
/// are left untouched, and blocks longer than the prepared maximum are processed in pieces.
///
/// ```
/// use syrp_audio_api::{AudioBlockMut, ProcessConfig};
/// use syrp_dsp_core::{DspError, DspProcessor, DspSample, DspSpec, LaneProcessor, ProcessContext, ProcessorDuplicator};
/// use syrp_simd::{F32x4, Sample};
/// /// Multiplies by the shared gain.
/// #[derive(Default)]
/// struct Gain<S>(std::marker::PhantomData<S>);
/// impl<S: DspSample<Element = f32>> LaneProcessor<f32> for Gain<S> {
///     type Lanes = S;
///     fn prepare(&mut self, _: &DspSpec, _: &f32) -> Result<(), DspError> { Ok(()) }
///     fn reset(&mut self) {}
///     fn process_lanes(&mut self, gain: &f32, xs: &mut [S], _: bool) {
///         xs.iter_mut().for_each(|x| *x *= S::splat(*gain));
///     }
/// }
/// let mut dup = ProcessorDuplicator::new(3.0_f32, Gain::<F32x4>::default);
/// dup.prepare(&DspSpec::new(ProcessConfig::new(48_000.0, 8)?, 6))?;
/// assert_eq!(dup.processors().len(), 2); // six channels in two packs of four
/// let mut data = [1.0_f32; 6 * 2];
/// dup.process(&mut ProcessContext::replacing(AudioBlockMut::new(&mut data, 6, 2, 2)?));
/// assert_eq!(data, [3.0; 12]);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[doc(alias = "dsp::ProcessorDuplicator")]
#[doc(alias = "dsp::ProcessorState")]
pub struct ProcessorDuplicator<P: LaneProcessor<St>, St> {
    state: St,
    factory: Box<dyn FnMut() -> P + Send>,
    processors: Vec<P>,
    /// One pack per frame, `max_block_size` long; empty for one-lane processors.
    scratch: Vec<P::Lanes>,
    channels: usize,
}

impl<P: LaneProcessor<St>, St> ProcessorDuplicator<P, St> {
    /// A duplicator sharing `state`, building instances with `factory`. Nothing is built
    /// until [`prepare`](DspProcessor::prepare).
    ///
    /// ```
    /// use syrp_dsp_core::{DspError, DspSpec, LaneProcessor, ProcessorDuplicator};
    /// struct Nop;
    /// impl LaneProcessor<u8> for Nop {
    ///     type Lanes = f32;
    ///     fn prepare(&mut self, _: &DspSpec, _: &u8) -> Result<(), DspError> { Ok(()) }
    ///     fn reset(&mut self) {}
    ///     fn process_lanes(&mut self, _: &u8, _: &mut [f32], _: bool) {}
    /// }
    /// let dup = ProcessorDuplicator::new(7_u8, || Nop);
    /// assert_eq!((*dup.state(), dup.processors().len()), (7, 0));
    /// ```
    #[must_use]
    pub fn new(state: St, factory: impl FnMut() -> P + Send + 'static) -> Self {
        Self {
            state,
            factory: Box::new(factory),
            processors: Vec::new(),
            scratch: Vec::new(),
            channels: 0,
        }
    }

    /// The shared state.
    ///
    /// ```
    /// # use syrp_dsp_core::{DspError, DspSpec, LaneProcessor, ProcessorDuplicator};
    /// # struct Nop; impl LaneProcessor<u8> for Nop { type Lanes = f32; fn prepare(&mut self, _: &DspSpec, _: &u8) -> Result<(), DspError> { Ok(()) } fn reset(&mut self) {} fn process_lanes(&mut self, _: &u8, _: &mut [f32], _: bool) {} }
    /// assert_eq!(*ProcessorDuplicator::new(4_u8, || Nop).state(), 4);
    /// ```
    #[must_use]
    pub fn state(&self) -> &St {
        &self.state
    }

    /// The shared state, mutably; changes are seen by every instance on the next `process`.
    ///
    /// ```
    /// # use syrp_dsp_core::{DspError, DspSpec, LaneProcessor, ProcessorDuplicator};
    /// # struct Nop; impl LaneProcessor<u8> for Nop { type Lanes = f32; fn prepare(&mut self, _: &DspSpec, _: &u8) -> Result<(), DspError> { Ok(()) } fn reset(&mut self) {} fn process_lanes(&mut self, _: &u8, _: &mut [f32], _: bool) {} }
    /// let mut dup = ProcessorDuplicator::new(4_u8, || Nop);
    /// *dup.state_mut() = 9;
    /// assert_eq!(*dup.state(), 9);
    /// ```
    pub fn state_mut(&mut self) -> &mut St {
        &mut self.state
    }

    /// The instances built by the last `prepare`: one per `P::Lanes::LANES` channels.
    ///
    /// ```
    /// # use syrp_dsp_core::{DspError, DspSpec, LaneProcessor, ProcessorDuplicator};
    /// # struct Nop; impl LaneProcessor<u8> for Nop { type Lanes = f32; fn prepare(&mut self, _: &DspSpec, _: &u8) -> Result<(), DspError> { Ok(()) } fn reset(&mut self) {} fn process_lanes(&mut self, _: &u8, _: &mut [f32], _: bool) {} }
    /// assert!(ProcessorDuplicator::new(0_u8, || Nop).processors().is_empty());
    /// ```
    #[must_use]
    pub fn processors(&self) -> &[P] {
        &self.processors
    }

    /// The instances, mutably.
    ///
    /// ```
    /// # use syrp_dsp_core::{DspError, DspSpec, LaneProcessor, ProcessorDuplicator};
    /// # struct Nop; impl LaneProcessor<u8> for Nop { type Lanes = f32; fn prepare(&mut self, _: &DspSpec, _: &u8) -> Result<(), DspError> { Ok(()) } fn reset(&mut self) {} fn process_lanes(&mut self, _: &u8, _: &mut [f32], _: bool) {} }
    /// assert!(ProcessorDuplicator::new(0_u8, || Nop).processors_mut().is_empty());
    /// ```
    pub fn processors_mut(&mut self) -> &mut [P] {
        &mut self.processors
    }

    /// Runs `P` on channels `first..first + LANES` (clipped to `channels`) of `out`, frames
    /// `start..end`, through the scratch.
    fn process_packed(
        &mut self,
        out: &mut syrp_audio_api::AudioBlockMut<'_, Elem<P, St>>,
        group: usize,
        channels: usize,
        bypassed: bool,
    ) {
        let lanes = <P::Lanes as syrp_simd::Sample>::LANES;
        let first = group * lanes;
        let used = (channels - first).min(lanes);
        let frames = out.frames();
        let mut start = 0;
        while start < frames {
            let len = (frames - start).min(self.scratch.len());
            let scratch = &mut self.scratch[..len];
            for (t, pack) in scratch.iter_mut().enumerate() {
                *pack = P::Lanes::from_fn(|l| {
                    if l < used {
                        out.channel(first + l)[start + t]
                    } else {
                        Elem::<P, St>::default()
                    }
                });
            }
            self.processors[group].process_lanes(&self.state, scratch, bypassed);
            for l in 0..used {
                let ch = &mut out.channel_mut(first + l)[start..start + len];
                for (s, pack) in ch.iter_mut().zip(scratch.iter()) {
                    *s = pack.extract(l);
                }
            }
            start += len;
        }
    }
}

impl<P, St> DspProcessor<Elem<P, St>> for ProcessorDuplicator<P, St>
where
    P: LaneProcessor<St>,
    St: Send,
    Elem<P, St>: DspSample,
{
    fn prepare(&mut self, spec: &DspSpec) -> Result<(), DspError> {
        let lanes = <P::Lanes as syrp_simd::Sample>::LANES;
        let channels = spec.channels();
        let groups = channels.div_ceil(lanes);
        let mut processors = vec_with_capacity("duplicated processors", groups)?;
        for g in 0..groups {
            let mut p = (self.factory)();
            let carried = (channels - g * lanes).min(lanes);
            p.prepare(&spec.with_channels(carried), &self.state)?;
            processors.push(p);
        }
        let len = if lanes == 1 { 0 } else { spec.max_block_size() };
        let mut scratch = vec_with_capacity("lane-packing scratch", len)?;
        scratch.resize(len, P::Lanes::default());
        self.processors = processors;
        self.scratch = scratch;
        self.channels = channels;
        Ok(())
    }

    fn reset(&mut self) {
        self.processors.iter_mut().for_each(LaneProcessor::reset);
    }

    fn process(&mut self, ctx: &mut ProcessContext<'_, Elem<P, St>>) {
        ctx.copy_input_to_output();
        let bypassed = ctx.is_bypassed();
        let mut out = ctx.output_block();
        let channels = out.channels().min(self.channels);
        debug_assert!(
            out.channels() <= self.channels,
            "block has {} channels, prepared for {}",
            out.channels(),
            self.channels
        );
        if <P::Lanes as syrp_simd::Sample>::LANES == 1 {
            for (c, p) in self.processors.iter_mut().enumerate().take(channels) {
                let samples = P::Lanes::scalar_slice_mut(out.channel_mut(c))
                    .expect("a one-lane DspSample is its element type");
                p.process_lanes(&self.state, samples, bypassed);
            }
        } else {
            let groups = channels.div_ceil(<P::Lanes as syrp_simd::Sample>::LANES);
            for g in 0..groups {
                self.process_packed(&mut out, g, channels, bypassed);
            }
        }
    }
}

impl<P: LaneProcessor<St>, St: std::fmt::Debug> std::fmt::Debug for ProcessorDuplicator<P, St> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessorDuplicator")
            .field("state", &self.state)
            .field("instances", &self.processors.len())
            .field("channels", &self.channels)
            .finish()
    }
}

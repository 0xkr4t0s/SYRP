// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`ProcessorChain`]: a tuple of processors run in order.

use crate::{DspError, DspProcessor, DspSample, DspSpec, ProcessContext};

/// A tuple of 1 to 8 processors that a [`ProcessorChain`] can run. Implemented for every
/// tuple `(P0,)` .. `(P0, .., P7)` whose members are all `DspProcessor<S>`.
///
/// ```
/// use syrp_dsp_core::{ChainProcessors, ProcessContext, ProcessorWrapper};
/// type Nop = ProcessorWrapper<(), fn(&mut (), &mut ProcessContext<'_, f32>)>;
/// assert_eq!(<(Nop, Nop, Nop) as ChainProcessors<f32>>::LEN, 3);
/// ```
pub trait ChainProcessors<S: DspSample>: Send {
    /// The number of processors.
    const LEN: usize;

    /// Prepares every processor in order, stopping at the first error.
    ///
    /// ```
    /// use syrp_audio_api::ProcessConfig;
    /// use syrp_dsp_core::{ChainProcessors, DspSpec, ProcessContext, ProcessorWrapper};
    /// let nop = |_: &mut (), _: &mut ProcessContext<'_, f32>| {};
    /// let mut t = (ProcessorWrapper::new((), nop),);
    /// t.prepare_all(&DspSpec::new(ProcessConfig::new(48_000.0, 8)?, 1))?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    fn prepare_all(&mut self, spec: &DspSpec) -> Result<(), DspError>;

    /// Resets every processor.
    ///
    /// ```
    /// use syrp_dsp_core::{ChainProcessors, DspProcessor, ProcessContext, ProcessorWrapper};
    /// let nop = |_: &mut u8, _: &mut ProcessContext<'_, f32>| {};
    /// let mut t = (ProcessorWrapper::new(9_u8, nop).with_reset(|n| *n = 0),);
    /// t.reset_all();
    /// assert_eq!(*t.0.get(), 0);
    /// ```
    fn reset_all(&mut self);

    /// Processes `ctx` with every processor whose bit in `bypassed` is clear. The first
    /// processor that runs gets `ctx` itself; later ones get a replacing context over its
    /// output block. If none runs, a non-replacing `ctx` has its input copied to its output.
    ///
    /// ```
    /// use syrp_audio_api::AudioBlockMut;
    /// use syrp_dsp_core::{ChainProcessors, ProcessContext, ProcessorWrapper};
    /// let add = |_: &mut (), ctx: &mut ProcessContext<'_, f32>| ctx.output_block().channel_mut(0)[0] += 1.0;
    /// let mut t = (ProcessorWrapper::new((), add), ProcessorWrapper::new((), add));
    /// let mut d = [0.0_f32];
    /// t.process_all(&mut ProcessContext::replacing(AudioBlockMut::new(&mut d, 1, 1, 1)?), 0b10);
    /// assert_eq!(d, [1.0]);
    /// # Ok::<(), syrp_audio_api::AudioError>(())
    /// ```
    fn process_all(&mut self, ctx: &mut ProcessContext<'_, S>, bypassed: u8);
}

/// Access to member `N` of a processor tuple; it is what bounds `N` at compile time in
/// [`ProcessorChain::get`] and friends.
///
/// ```
/// use syrp_dsp_core::ChainIndex;
/// let t = (1_u8, "two", 3.0_f32);
/// assert_eq!(*<_ as ChainIndex<1>>::get(&t), "two");
/// ```
pub trait ChainIndex<const N: usize> {
    /// The type of member `N`.
    type Processor;

    /// Member `N`.
    ///
    /// ```
    /// use syrp_dsp_core::ChainIndex;
    /// assert_eq!(*ChainIndex::<0>::get(&(7_i32,)), 7);
    /// ```
    fn get(&self) -> &Self::Processor;

    /// Member `N`, mutably.
    ///
    /// ```
    /// use syrp_dsp_core::ChainIndex;
    /// let mut t = (7_i32, 8_i32);
    /// *ChainIndex::<1>::get_mut(&mut t) = 0;
    /// assert_eq!(t, (7, 0));
    /// ```
    fn get_mut(&mut self) -> &mut Self::Processor;
}

/// Runs a tuple of processors in order as one processor (JUCE's `dsp::ProcessorChain`).
///
/// Each member can be bypassed with [`set_bypassed`](Self::set_bypassed); a bypassed member
/// is skipped (its `process` is not called), not run with the bypass flag set. `prepare` and
/// `reset` reach every member, bypassed or not. Members are reached by index with
/// [`get`](Self::get), checked at compile time. A chain is itself a processor, so chains
/// nest.
///
/// With a non-replacing context, the first member that runs reads the input block and
/// writes the output block, and the rest run in place on the output block; when every
/// member is bypassed the input is copied to the output (Syrp's choice: the JUCE
/// documentation does not say).
///
/// ```
/// use syrp_audio_api::{AudioBlockMut, ProcessConfig};
/// use syrp_dsp_core::{DspProcessor, DspSpec, ProcessContext, ProcessorChain, ProcessorWrapper};
/// let double = |_: &mut (), ctx: &mut ProcessContext<'_, f32>| {
///     ctx.copy_input_to_output();
///     ctx.output_block().channel_mut(0).iter_mut().for_each(|s| *s *= 2.0);
/// };
/// let mut chain = ProcessorChain::new((ProcessorWrapper::new((), double), ProcessorWrapper::new((), double)));
/// chain.prepare(&DspSpec::new(ProcessConfig::new(48_000.0, 4)?, 1))?;
/// chain.set_bypassed::<1>(true);
/// let mut d = [1.0_f32];
/// chain.process(&mut ProcessContext::replacing(AudioBlockMut::new(&mut d, 1, 1, 1)?));
/// assert_eq!(d, [2.0]);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[doc(alias = "dsp::ProcessorChain")]
#[derive(Debug, Clone, Default)]
pub struct ProcessorChain<T> {
    processors: T,
    /// Bit `i` set: member `i` is bypassed.
    bypassed: u8,
}

impl<T> ProcessorChain<T> {
    /// A chain of `processors`, none bypassed.
    ///
    /// ```
    /// let chain = syrp_dsp_core::ProcessorChain::new((1_u8, 2_u8));
    /// assert!(!chain.is_bypassed::<0>());
    /// ```
    #[must_use]
    pub fn new(processors: T) -> Self {
        Self {
            processors,
            bypassed: 0,
        }
    }

    /// Member `N`.
    ///
    /// ```
    /// let chain = syrp_dsp_core::ProcessorChain::new((1_u8, 2_u8));
    /// assert_eq!(*chain.get::<1>(), 2);
    /// ```
    #[must_use]
    pub fn get<const N: usize>(&self) -> &T::Processor
    where
        T: ChainIndex<N>,
    {
        self.processors.get()
    }

    /// Member `N`, mutably.
    ///
    /// ```
    /// let mut chain = syrp_dsp_core::ProcessorChain::new((1_u8, 2_u8));
    /// *chain.get_mut::<0>() = 5;
    /// assert_eq!(*chain.get::<0>(), 5);
    /// ```
    pub fn get_mut<const N: usize>(&mut self) -> &mut T::Processor
    where
        T: ChainIndex<N>,
    {
        self.processors.get_mut()
    }

    /// Bypasses (`true`) or enables member `N`.
    ///
    /// ```
    /// let mut chain = syrp_dsp_core::ProcessorChain::new((1_u8, 2_u8));
    /// chain.set_bypassed::<1>(true);
    /// assert!(chain.is_bypassed::<1>() && !chain.is_bypassed::<0>());
    /// ```
    pub fn set_bypassed<const N: usize>(&mut self, bypassed: bool)
    where
        T: ChainIndex<N>,
    {
        let bit = 1_u8 << N;
        if bypassed {
            self.bypassed |= bit;
        } else {
            self.bypassed &= !bit;
        }
    }

    /// Whether member `N` is bypassed.
    ///
    /// ```
    /// let chain = syrp_dsp_core::ProcessorChain::new((1_u8,));
    /// assert!(!chain.is_bypassed::<0>());
    /// ```
    #[must_use]
    pub fn is_bypassed<const N: usize>(&self) -> bool
    where
        T: ChainIndex<N>,
    {
        self.bypassed & (1_u8 << N) != 0
    }

    /// The member tuple.
    ///
    /// ```
    /// let chain = syrp_dsp_core::ProcessorChain::new((1_u8, 2_u8));
    /// assert_eq!(*chain.processors(), (1, 2));
    /// ```
    #[must_use]
    pub fn processors(&self) -> &T {
        &self.processors
    }

    /// The member tuple, mutably.
    ///
    /// ```
    /// let mut chain = syrp_dsp_core::ProcessorChain::new((1_u8, 2_u8));
    /// chain.processors_mut().1 = 3;
    /// assert_eq!(*chain.get::<1>(), 3);
    /// ```
    pub fn processors_mut(&mut self) -> &mut T {
        &mut self.processors
    }

    /// Unwraps the member tuple.
    ///
    /// ```
    /// let chain = syrp_dsp_core::ProcessorChain::new((1_u8, 2_u8));
    /// assert_eq!(chain.into_inner(), (1, 2));
    /// ```
    #[must_use]
    pub fn into_inner(self) -> T {
        self.processors
    }
}

impl<S: DspSample, T: ChainProcessors<S>> DspProcessor<S> for ProcessorChain<T> {
    fn prepare(&mut self, spec: &DspSpec) -> Result<(), DspError> {
        self.processors.prepare_all(spec)
    }

    fn reset(&mut self) {
        self.processors.reset_all();
    }

    fn process(&mut self, ctx: &mut ProcessContext<'_, S>) {
        self.processors.process_all(ctx, self.bypassed);
    }
}

// Macro justification: `ChainProcessors` needs one impl per tuple arity (1 to 8) and
// `ChainIndex` one per arity and index (36); writing them out would be ~400 lines of
// identical code. `chain_tuple!` takes the arity's `index Type` pairs.
macro_rules! chain_tuple {
    ($len:literal; $($idx:tt $P:ident),+) => {
        impl<S: DspSample, $($P: DspProcessor<S>),+> ChainProcessors<S> for ($($P,)+) {
            const LEN: usize = $len;

            fn prepare_all(&mut self, spec: &DspSpec) -> Result<(), DspError> {
                $(self.$idx.prepare(spec)?;)+
                Ok(())
            }

            fn reset_all(&mut self) {
                $(self.$idx.reset();)+
            }

            fn process_all(&mut self, ctx: &mut ProcessContext<'_, S>, bypassed: u8) {
                let mut routed = false;
                $(
                    if bypassed & (1_u8 << $idx) == 0 {
                        if routed {
                            self.$idx.process(&mut ctx.as_replacing());
                        } else {
                            self.$idx.process(ctx);
                            routed = true;
                        }
                    }
                )+
                if !routed {
                    ctx.copy_input_to_output();
                }
            }
        }

        chain_index!([$($P),+]; $($idx $P)+);
    };
}

/// `ChainIndex` impls for one tuple type: `[all type params]; index Type ...`.
macro_rules! chain_index {
    ($tys:tt; $($idx:tt $Out:ident)+) => {
        $(chain_index_one!($tys; $idx $Out);)+
    };
}

/// One `ChainIndex<idx>` impl for the tuple of `[all type params]`.
macro_rules! chain_index_one {
    ([$($P:ident),+]; $idx:tt $Out:ident) => {
        impl<$($P),+> ChainIndex<$idx> for ($($P,)+) {
            type Processor = $Out;
            fn get(&self) -> &$Out {
                &self.$idx
            }
            fn get_mut(&mut self) -> &mut $Out {
                &mut self.$idx
            }
        }
    };
}

chain_tuple!(1; 0 A);
chain_tuple!(2; 0 A, 1 B);
chain_tuple!(3; 0 A, 1 B, 2 C);
chain_tuple!(4; 0 A, 1 B, 2 C, 3 D);
chain_tuple!(5; 0 A, 1 B, 2 C, 3 D, 4 E);
chain_tuple!(6; 0 A, 1 B, 2 C, 3 D, 4 E, 5 F);
chain_tuple!(7; 0 A, 1 B, 2 C, 3 D, 4 E, 5 F, 6 G);
chain_tuple!(8; 0 A, 1 B, 2 C, 3 D, 4 E, 5 F, 6 G, 7 H);

// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Tests for work/specs/WP-2.1-10.md B2: ProcessorChain, ProcessorDuplicator and
//! ProcessorWrapper.

mod common;

use common::{Gain, assert_f32_arith, noise, spec};
use syrp_audio_api::{AudioBlock, AudioBlockMut};
use syrp_dsp_core::{
    DspError, DspProcessor, DspSample, DspSpec, LaneProcessor, ProcessContext, ProcessorChain,
    ProcessorDuplicator, ProcessorWrapper,
};
use syrp_error::Diagnostic;
use syrp_rt::no_alloc;
use syrp_simd::{F32x4, F32x8, F64x2};

#[global_allocator]
static ALLOC: syrp_rt::GuardedAllocator = syrp_rt::GuardedAllocator;

#[test]
fn b2_chain_of_gains_equals_the_product() {
    let mut chain = ProcessorChain::new((
        Gain::new(0.5),
        Gain::new(1.5),
        Gain::new(-2.0),
        Gain::new(0.75),
    ));
    chain.prepare(&spec(2, 256)).unwrap();
    let mut data = noise(2, 512);
    let product = 0.5_f32 * 1.5 * -2.0 * 0.75;
    let expected: Vec<f32> = data.iter().map(|s| s * product).collect();
    no_alloc(|| {
        let block = AudioBlockMut::new(&mut data, 2, 256, 256).unwrap();
        chain.process(&mut ProcessContext::replacing(block));
    });
    assert_f32_arith(&data, &expected);
    assert_eq!(chain.get::<3>().prepared, Some(spec(2, 256)));
}

#[test]
fn b2_bypassed_members_are_skipped_not_flagged() {
    let mut chain = ProcessorChain::new((Gain::new(2.0), Gain::new(3.0), Gain::new(5.0)));
    chain.prepare(&spec(1, 4)).unwrap();
    chain.set_bypassed::<1>(true);
    assert!(chain.is_bypassed::<1>() && !chain.is_bypassed::<0>() && !chain.is_bypassed::<2>());
    let mut d = [1.0_f32; 4];
    chain.process(&mut ProcessContext::replacing(
        AudioBlockMut::new(&mut d, 1, 4, 4).unwrap(),
    ));
    assert_eq!(d, [10.0; 4]);
    assert_eq!(chain.get::<1>().calls, 0, "a bypassed member is not called");
    assert_eq!((chain.get::<0>().calls, chain.get::<2>().calls), (1, 1));
    chain.set_bypassed::<1>(false);
    assert!(!chain.is_bypassed::<1>());
    chain.reset();
    assert_eq!(chain.get::<0>().calls, 0, "reset reaches every member");
}

#[test]
fn b2_non_replacing_chain_routes_input_through_the_first_running_member() {
    let input = [1.0_f32, 2.0];
    let mut output = [0.0_f32; 2];
    let mut chain = ProcessorChain::new((Gain::new(3.0), Gain::new(5.0), Gain::new(7.0)));
    chain.set_bypassed::<0>(true);
    chain.process(&mut ProcessContext::non_replacing(
        AudioBlock::new(&input, 1, 2, 2).unwrap(),
        AudioBlockMut::new(&mut output, 1, 2, 2).unwrap(),
    ));
    assert_eq!(output, [35.0, 70.0]);
    assert_eq!(input, [1.0, 2.0]);

    // Every member bypassed: the input is passed through.
    chain.set_bypassed::<1>(true);
    chain.set_bypassed::<2>(true);
    let mut output = [0.0_f32; 2];
    chain.process(&mut ProcessContext::non_replacing(
        AudioBlock::new(&input, 1, 2, 2).unwrap(),
        AudioBlockMut::new(&mut output, 1, 2, 2).unwrap(),
    ));
    assert_eq!(output, [1.0, 2.0]);
}

#[test]
fn b2_chains_nest_and_hold_up_to_eight_members() {
    let g = Gain::new;
    let mut chain = ProcessorChain::new((
        g(1.0),
        g(2.0),
        g(1.0),
        g(2.0),
        g(1.0),
        g(2.0),
        g(1.0),
        ProcessorChain::new((g(0.5), g(0.25))),
    ));
    chain.prepare(&spec(1, 1)).unwrap();
    chain.get_mut::<7>().set_bypassed::<1>(true);
    chain.set_bypassed::<6>(true);
    let mut d = [1.0_f32];
    chain.process(&mut ProcessContext::replacing(
        AudioBlockMut::new(&mut d, 1, 1, 1).unwrap(),
    ));
    assert_eq!(d, [4.0]);
    assert_eq!(chain.get::<7>().get::<0>().prepared, Some(spec(1, 1)));
}

#[test]
fn b2_chain_passes_the_context_bypass_flag_to_members() {
    let mut chain = ProcessorChain::new((Gain::new(2.0), Gain::new(3.0)));
    let mut d = [1.0_f32];
    let mut ctx = ProcessContext::replacing(AudioBlockMut::new(&mut d, 1, 1, 1).unwrap());
    ctx.set_bypassed(true);
    chain.process(&mut ctx);
    assert_eq!(d, [1.0]);
    assert_eq!((chain.get::<0>().calls, chain.get::<1>().calls), (1, 1));
}

#[test]
fn b2_processor_wrapper_adapts_foreign_state() {
    /// A type with no `DspProcessor` impl.
    struct Offset {
        by: f32,
        rate: f64,
    }
    let mut chain = ProcessorChain::new((
        ProcessorWrapper::new(
            Offset { by: 0.5, rate: 0.0 },
            |o: &mut Offset, ctx: &mut ProcessContext<'_, f32>| {
                ctx.copy_input_to_output();
                for ch in ctx.output_block().split_channels_mut() {
                    ch.iter_mut().for_each(|s| *s += o.by);
                }
            },
        )
        .with_prepare(|o, spec| {
            o.rate = spec.sample_rate();
            Ok(())
        })
        .with_reset(|o| o.by = 0.0),
        Gain::new(2.0),
    ));
    chain.prepare(&spec(1, 2)).unwrap();
    assert_eq!(chain.get::<0>().get().rate, 48_000.0);
    let mut d = [1.0_f32, 2.0];
    no_alloc(|| {
        chain.process(&mut ProcessContext::replacing(
            AudioBlockMut::new(&mut d, 1, 2, 2).unwrap(),
        ));
    });
    assert_eq!(d, [3.0, 5.0]);
    chain.reset();
    assert_eq!(chain.get::<0>().get().by, 0.0);
}

/// A one-pole low-pass, `y += (x - y) * coeff`, written once for every sample type; the
/// coefficient is the shared state.
struct OnePole<S> {
    y: S,
}

impl<S: DspSample> OnePole<S> {
    fn new() -> Self {
        Self { y: S::zero() }
    }
}

impl<S: DspSample> LaneProcessor<S::Element> for OnePole<S> {
    type Lanes = S;
    fn prepare(&mut self, _: &DspSpec, _: &S::Element) -> Result<(), DspError> {
        Ok(())
    }
    fn reset(&mut self) {
        self.y = S::zero();
    }
    fn process_lanes(&mut self, coeff: &S::Element, samples: &mut [S], bypassed: bool) {
        if bypassed {
            return;
        }
        let c = S::splat(*coeff);
        for x in samples {
            self.y += (*x - self.y) * c;
            *x = self.y;
        }
    }
}

/// Runs `channels` channels of noise through a duplicated `OnePole<S>` in blocks of
/// `block`, and returns the output.
fn run_duplicated<S: DspSample<Element = f32>>(
    channels: usize,
    frames: usize,
    block: usize,
) -> Vec<f32> {
    let mut dup = ProcessorDuplicator::new(0.2_f32, OnePole::<S>::new);
    dup.prepare(&spec(channels, block)).unwrap();
    assert_eq!(dup.processors().len(), channels.div_ceil(S::LANES));
    let mut data = noise(3, channels * frames);
    let mut start = 0;
    while start < frames {
        let len = block.min(frames - start);
        // Planar data, stride `frames`: slice the frame range of every channel.
        let full = AudioBlockMut::new(&mut data, channels, frames, frames).unwrap();
        let mut ctx = ProcessContext::replacing(full.into_slice_frames(start..start + len));
        no_alloc(|| dup.process(&mut ctx));
        start += len;
    }
    data
}

/// One scalar `OnePole<f32>` per channel, run directly.
fn run_reference(channels: usize, frames: usize) -> Vec<f32> {
    let mut data = noise(3, channels * frames);
    for ch in data.chunks_mut(frames) {
        let mut p = OnePole::<f32>::new();
        p.process_lanes(&0.2, ch, false);
    }
    data
}

#[test]
fn b2_duplicated_mono_processors_equal_per_channel_instances() {
    let (channels, frames) = (6, 300);
    let reference = run_reference(channels, frames);
    assert_f32_arith(&run_duplicated::<f32>(channels, frames, 64), &reference);
    assert_f32_arith(&run_duplicated::<F32x4>(channels, frames, 64), &reference);
    assert_f32_arith(&run_duplicated::<F32x8>(channels, frames, 37), &reference);
}

#[test]
fn b2_duplicator_handles_f64_packs_and_longer_blocks_than_prepared() {
    let mut dup = ProcessorDuplicator::new(0.5_f64, OnePole::<F64x2>::new);
    dup.prepare(&spec(3, 4)).unwrap();
    let mut data = vec![1.0_f64; 3 * 10];
    let mut ctx = ProcessContext::replacing(AudioBlockMut::new(&mut data, 3, 10, 10).unwrap());
    no_alloc(|| dup.process(&mut ctx));
    let mut expected = vec![1.0_f64; 10];
    OnePole::<f64>::new().process_lanes(&0.5, &mut expected, false);
    for ch in data.chunks(10) {
        assert_eq!(ch, expected.as_slice());
    }
}

#[test]
fn b2_duplicator_shares_state_resets_and_honours_bypass() {
    let input = [1.0_f32; 4];
    let mut output = [0.0_f32; 4];
    let mut dup = ProcessorDuplicator::new(1.0_f32, OnePole::<F32x4>::new);
    dup.prepare(&spec(2, 2)).unwrap();
    *dup.state_mut() = 0.5;
    let mut ctx = ProcessContext::non_replacing(
        AudioBlock::new(&input, 2, 2, 2).unwrap(),
        AudioBlockMut::new(&mut output, 2, 2, 2).unwrap(),
    );
    dup.process(&mut ctx);
    assert_eq!(output, [0.5, 0.75, 0.5, 0.75]);

    dup.reset();
    let mut ctx = ProcessContext::non_replacing(
        AudioBlock::new(&input, 2, 2, 2).unwrap(),
        AudioBlockMut::new(&mut output, 2, 2, 2).unwrap(),
    );
    dup.process(&mut ctx);
    assert_eq!(
        output,
        [0.5, 0.75, 0.5, 0.75],
        "reset cleared every instance"
    );

    let mut output = [0.0_f32; 4];
    let mut ctx = ProcessContext::non_replacing(
        AudioBlock::new(&input, 2, 2, 2).unwrap(),
        AudioBlockMut::new(&mut output, 2, 2, 2).unwrap(),
    );
    ctx.set_bypassed(true);
    dup.process(&mut ctx);
    assert_eq!(output, input, "bypassed: input copied, processors leave it");
}

#[test]
fn b2_duplicator_prepare_reports_allocation_failure() {
    let mut dup = ProcessorDuplicator::new(0.0_f32, OnePole::<F32x4>::new);
    let err = dup.prepare(&spec(usize::MAX, 4)).unwrap_err();
    assert!(matches!(err, DspError::CannotAllocate { .. }));
    assert_eq!(err.code().as_str(), "SYRP-DSP-0001");
    let mut dup = ProcessorDuplicator::new(0.0_f32, OnePole::<F32x8>::new);
    let err = dup.prepare(&spec(1, usize::MAX / 2)).unwrap_err();
    assert_eq!(err.code().as_str(), "SYRP-DSP-0001");
    assert!(
        dup.processors().is_empty(),
        "a failed prepare changes nothing"
    );
}

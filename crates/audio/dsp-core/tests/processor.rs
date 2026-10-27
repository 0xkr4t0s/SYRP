// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Tests for work/specs/WP-2.1-10.md B1: the processor model.

mod common;

use common::{Gain, noise, spec};
use syrp_audio_api::{AudioBlock, AudioBlockMut, ProcessConfig};
use syrp_dsp_core::{DspProcessor, DspSample, DspSpec, ProcessContext};
use syrp_rt::no_alloc;
use syrp_simd::{F32x4, F32x8, F64x2};
use syrp_testkit::{Tolerance, assert_close};

#[global_allocator]
static ALLOC: syrp_rt::GuardedAllocator = syrp_rt::GuardedAllocator;

#[test]
fn b1_spec_comes_from_process_config_and_channels() {
    let cfg = ProcessConfig::new(44_100.0, 128).unwrap();
    let s = DspSpec::new(cfg, 3);
    assert_eq!(s.sample_rate(), 44_100.0);
    assert_eq!(s.max_block_size(), 128);
    assert_eq!(s.channels(), 3);
    assert_eq!(s.with_channels(1).channels(), 1);
    assert_eq!(s.with_channels(1).max_block_size(), 128);
}

#[test]
fn b1_replacing_context_shares_one_block() {
    let mut data = [1.0_f32, 2.0, 3.0, 4.0];
    let mut ctx = ProcessContext::<f32>::replacing(AudioBlockMut::new(&mut data, 2, 2, 2).unwrap());
    assert!(!ctx.uses_separate_input_and_output_blocks());
    assert!(ctx.separate_blocks().is_none());
    ctx.output_block().channel_mut(1)[0] = 9.0;
    assert_eq!(ctx.input_block().channel(1), &[9.0, 4.0]);
    ctx.copy_input_to_output(); // a no-op
    assert_eq!(data, [1.0, 2.0, 9.0, 4.0]);
}

#[test]
fn b1_non_replacing_context_keeps_input_and_output_apart() {
    let input = [1.0_f64, 2.0, 3.0, 4.0];
    let mut output = [0.0_f64; 6];
    let mut ctx = ProcessContext::<f64>::non_replacing(
        AudioBlock::new(&input, 2, 2, 2).unwrap(),
        AudioBlockMut::new(&mut output, 3, 2, 2).unwrap(),
    );
    assert!(ctx.uses_separate_input_and_output_blocks());
    assert_eq!(ctx.input_block().channels(), 2);
    assert_eq!(ctx.output_block().channels(), 3);
    ctx.copy_input_to_output();
    assert_eq!(output, [1.0, 2.0, 3.0, 4.0, 0.0, 0.0]);
}

#[test]
fn b1_bypass_flag_round_trips_and_reaches_replacing_views() {
    let input = [1.0_f32];
    let mut output = [0.0_f32];
    let mut ctx = ProcessContext::<f32>::non_replacing(
        AudioBlock::new(&input, 1, 1, 1).unwrap(),
        AudioBlockMut::new(&mut output, 1, 1, 1).unwrap(),
    );
    assert!(!ctx.is_bypassed());
    ctx.set_bypassed(true);
    let inner = ctx.as_replacing();
    assert!(inner.is_bypassed() && !inner.uses_separate_input_and_output_blocks());
}

#[test]
fn b1_processor_lifecycle_without_allocation() {
    let mut g = Gain::new(0.5);
    g.prepare(&spec(2, 64)).unwrap();
    assert_eq!(g.prepared, Some(spec(2, 64)));
    let mut data = noise(1, 128);
    let expected: Vec<f32> = data.iter().map(|s| s * 0.5).collect();
    no_alloc(|| {
        let block = AudioBlockMut::new(&mut data, 2, 64, 64).unwrap();
        g.process(&mut ProcessContext::replacing(block));
        g.reset();
    });
    assert_eq!(g.calls, 0);
    assert_close(&data, &expected, Tolerance::exact());
}

#[test]
fn b1_boxed_dyn_processor_is_a_processor() {
    let mut p: Box<dyn DspProcessor<f32>> = Box::new(Gain::new(4.0));
    p.prepare(&spec(1, 4)).unwrap();
    let mut d = [0.25_f32; 4];
    p.process(&mut ProcessContext::replacing(
        AudioBlockMut::new(&mut d, 1, 4, 4).unwrap(),
    ));
    assert_eq!(d, [1.0; 4]);
}

#[test]
fn b1_dsp_sample_lanes_round_trip() {
    fn check<S: DspSample>()
    where
        S::Element: From<u8>,
    {
        let v = S::from_fn(|i| S::Element::from(u8::try_from(i).unwrap() + 1));
        for i in 0..S::LANES {
            assert_eq!(v.extract(i), S::Element::from(u8::try_from(i).unwrap() + 1));
        }
        let mut buf = [S::Element::default(); 2];
        assert_eq!(S::scalar_slice_mut(&mut buf).is_some(), S::LANES == 1);
    }
    check::<f32>();
    check::<f64>();
    check::<F32x4>();
    check::<F32x8>();
    check::<F64x2>();
    check::<syrp_simd::F64x4>();
}

// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Helpers shared by the integration tests.
#![allow(dead_code)]

use syrp_audio_api::ProcessConfig;
use syrp_dsp_core::{DspError, DspProcessor, DspSpec, ProcessContext};
use syrp_testkit::NoiseRng;

/// The `f32-arith` profile (master plan §9.3): every sample within 4 ULP of the reference, or
/// within 1e-7 absolute near zero.
#[track_caller]
pub fn assert_f32_arith(actual: &[f32], expected: &[f32]) {
    assert_eq!(actual.len(), expected.len());
    for (i, (&a, &e)) in actual.iter().zip(expected).enumerate() {
        let ulp = f32::from_bits(e.abs().to_bits() + 1) - e.abs();
        let err = (a - e).abs();
        assert!(
            err <= 4.0 * ulp || err <= 1e-7,
            "sample {i}: {a} vs {e} (f32-arith)"
        );
    }
}

pub fn spec(channels: usize, max_block: usize) -> DspSpec {
    DspSpec::new(
        ProcessConfig::new(48_000.0, max_block).expect("valid config"),
        channels,
    )
}

/// Multiplies by a constant and counts its calls.
pub struct Gain {
    pub gain: f32,
    pub calls: usize,
    pub prepared: Option<DspSpec>,
}

impl Gain {
    pub fn new(gain: f32) -> Self {
        Self {
            gain,
            calls: 0,
            prepared: None,
        }
    }
}

impl DspProcessor<f32> for Gain {
    fn prepare(&mut self, spec: &DspSpec) -> Result<(), DspError> {
        self.prepared = Some(*spec);
        Ok(())
    }
    fn reset(&mut self) {
        self.calls = 0;
    }
    fn process(&mut self, ctx: &mut ProcessContext<'_, f32>) {
        self.calls += 1;
        if ctx.is_bypassed() {
            ctx.copy_input_to_output();
            return;
        }
        let gain = self.gain;
        if let Some((input, mut output)) = ctx.separate_blocks() {
            for ch in 0..output.channels() {
                for (o, i) in output.channel_mut(ch).iter_mut().zip(input.channel(ch)) {
                    *o = *i * gain;
                }
            }
        } else {
            for ch in ctx.output_block().split_channels_mut() {
                ch.iter_mut().for_each(|s| *s *= gain);
            }
        }
    }
}

pub fn noise(seed: u64, len: usize) -> Vec<f32> {
    let mut v = vec![0.0; len];
    NoiseRng::new(seed).fill_noise(&mut v, 1.0);
    v
}

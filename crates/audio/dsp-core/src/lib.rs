// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Syrp DSP core (master plan §4.9 and §5.2, layer L3): the base of JUCE's `juce_dsp`.
//!
//! - The processor model: [`DspSpec`], [`ProcessContext`] and the [`DspProcessor`] trait that
//!   every `syrp-dsp-*` processor implements.
//!
//! Per-instance DSP is written against [`syrp_simd::Sample`] (ADR 0003 D5); [`DspSample`]
//! adds the lane access the processor model needs. Audio arrives as
//! [`syrp_audio_api::AudioBlockMut`] views of `f32` or `f64`.
//!
//! ```
//! use syrp_audio_api::{AudioBlockMut, ProcessConfig};
//! use syrp_dsp_core::{DspError, DspProcessor, DspSpec, ProcessContext};
//!
//! /// Multiplies every sample by a constant.
//! struct Gain(f32);
//! impl DspProcessor<f32> for Gain {
//!     fn prepare(&mut self, _: &DspSpec) -> Result<(), DspError> { Ok(()) }
//!     fn reset(&mut self) {}
//!     fn process(&mut self, ctx: &mut ProcessContext<'_, f32>) {
//!         ctx.copy_input_to_output();
//!         for ch in ctx.output_block().split_channels_mut() {
//!             ch.iter_mut().for_each(|s| *s *= self.0);
//!         }
//!     }
//! }
//!
//! let mut gain = Gain(2.0);
//! gain.prepare(&DspSpec::new(ProcessConfig::new(48_000.0, 64)?, 1))?;
//! let mut data = [1.0_f32; 4];
//! gain.process(&mut ProcessContext::replacing(AudioBlockMut::new(&mut data, 1, 4, 4)?));
//! assert_eq!(data, [2.0; 4]);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod context;
mod error;
mod processor;
mod sample;
mod spec;

pub use context::ProcessContext;
pub use error::DspError;
pub use processor::DspProcessor;
pub use sample::DspSample;
pub use spec::DspSpec;

// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Syrp audio basics (master plan §5.2, layer L3).
//!
//! - [`buffer`]: [`AudioBuffer`], the owned planar audio buffer. Its storage follows ADR 0003
//!   D2 (one allocation, every channel on a 64-byte boundary) and it lends
//!   [`syrp_audio_api::AudioBlock`] and [`syrp_audio_api::AudioBlockMut`] views.
//! - [`vector_ops`]: slice operations in the role of JUCE's `FloatVectorOperations`.
//! - [`control`]: control-rate DSP (WP-2.1-03).
//! - [`voice`]: the `VoicePool` (WP-2.1-05).
//!
//! ```
//! use syrp_audio_basics::AudioBuffer;
//! let mut buf = AudioBuffer::<f32>::new(2, 512)?;
//! buf.channel_mut(1)[0] = 0.5;
//! assert_eq!(buf.as_block().channel(1)[0], 0.5);
//! # Ok::<(), syrp_audio_basics::buffer::BufferError>(())
//! ```

pub mod buffer;
pub mod control;
mod sealed;
pub mod vector_ops;
pub mod voice;

pub use buffer::{AudioBuffer, BufferError, BufferSample};

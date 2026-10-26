// SPDX-License-Identifier: MIT OR Apache-2.0
//! The Syrp audio data contract (master plan §5.8, ADR 0003).
//!
//! - [`AudioBlock`] and [`AudioBlockMut`]: non-owning views over planar channels stored in one
//!   region with a fixed channel stride, for `f32` and `f64` ([`AudioSample`]).
//! - [`ChannelSet`], [`Speaker`], [`Bus`], [`BusLayout`] and [`LayoutResponse`]: channel
//!   layouts and bus negotiation.
//! - [`ProcessConfig`]: what a processor is prepared with.
//! - [`PlayHeadInfo`] and [`PlayHead`]: transport position as reported by a host.
//!
//! Types used on the audio thread implement [`syrp_rt::RtSafe`] and have no public fields
//! (ADR 0003 D3), so their storage can change without a contract change.
//!
//! ```
//! use syrp_audio_api::{AudioBlockMut, ProcessConfig};
//! let cfg = ProcessConfig::new(48_000.0, 64)?;
//! let mut storage = vec![0.0_f32; 2 * cfg.max_block_size()];
//! let mut block = AudioBlockMut::new(&mut storage, 2, 64, 64)?;
//! block.channel_mut(1)[0] = 1.0;
//! assert_eq!(block.as_block().channel(1)[0], 1.0);
//! # Ok::<(), syrp_audio_api::AudioError>(())
//! ```

use std::fmt;

use syrp_error::{Diagnostic, ErrorCode};

mod block;
mod channels;
mod config;
mod playhead;

pub use block::{AudioBlock, AudioBlockMut, AudioSample, ChannelsMut, SIMD_ALIGN};
pub use channels::{Bus, BusLayout, ChannelSet, LayoutResponse, Speaker};
pub use config::{Precision, ProcessConfig, ProcessMode};
pub use playhead::{LoopPoints, PlayHead, PlayHeadInfo, TimeSignature};

/// Errors from building audio views and configurations, catalogued in `errors/audio.toml`.
/// ```
/// use syrp_error::Diagnostic;
/// let err = syrp_audio_api::ProcessConfig::new(0.0, 64).unwrap_err();
/// assert_eq!(err.code().as_str(), "SYRP-AUDIO-0003");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AudioError {
    /// A view's `frames` is larger than its channel `stride` (`SYRP-AUDIO-0001`).
    FramesExceedStride {
        /// Requested frames per channel.
        frames: usize,
        /// Requested channel stride, in samples.
        stride: usize,
    },
    /// The slice is shorter than `channels * stride` samples (`SYRP-AUDIO-0002`).
    /// `needed` is `usize::MAX` when that product overflows.
    BufferTooShort {
        /// Samples the layout needs.
        needed: usize,
        /// Samples the slice has.
        len: usize,
    },
    /// The sample rate is not finite and positive (`SYRP-AUDIO-0003`).
    InvalidSampleRate,
    /// The maximum block size is zero (`SYRP-AUDIO-0004`).
    InvalidBlockSize,
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            AudioError::FramesExceedStride { frames, stride } => {
                write!(
                    f,
                    "{frames} frames do not fit in a channel stride of {stride}"
                )
            }
            AudioError::BufferTooShort { needed, len } => {
                write!(f, "buffer has {len} samples but the layout needs {needed}")
            }
            AudioError::InvalidSampleRate => f.write_str("sample rate must be finite and > 0"),
            AudioError::InvalidBlockSize => f.write_str("maximum block size must be > 0"),
        }
    }
}

impl std::error::Error for AudioError {}

impl Diagnostic for AudioError {
    fn code(&self) -> ErrorCode {
        ErrorCode::new(match self {
            AudioError::FramesExceedStride { .. } => "SYRP-AUDIO-0001",
            AudioError::BufferTooShort { .. } => "SYRP-AUDIO-0002",
            AudioError::InvalidSampleRate => "SYRP-AUDIO-0003",
            AudioError::InvalidBlockSize => "SYRP-AUDIO-0004",
        })
    }
}

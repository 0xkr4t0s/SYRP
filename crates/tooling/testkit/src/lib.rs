// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Deterministic offline test helpers for Syrp audio code (master plan §9, WP-0.1-09).
//!
//! - [`render_offline`] runs a block-processing closure over planar input in fixed blocks and
//!   returns a [`Rendered`] buffer; [`render_offline_no_alloc`] does the same with every block
//!   inside [`syrp_rt::no_alloc`], and [`assert_no_alloc`] guards any other closure.
//! - [`write_wav`] and [`Rendered::write_wav_file`] write 32-bit float or 16/24-bit PCM WAV.
//! - [`NoiseRng`] and [`white_noise`] give reproducible noise inputs from a seed.
//!
//! ```
//! use syrp_testkit::{render_offline, white_noise};
//!
//! let input = white_noise(7, 1_000, 0.5)?;
//! let out = render_offline(
//!     |_cfg, mut block| {
//!         for ch in block.split_channels_mut() {
//!             ch.iter_mut().for_each(|s| *s *= 0.5);
//!         }
//!     },
//!     &[&input],
//!     64,
//!     48_000.0,
//! )?;
//! let expected: Vec<f32> = input.iter().map(|s| s * 0.5).collect();
//! assert_eq!(out.channel(0), expected.as_slice());
//! # Ok::<(), syrp_testkit::TestkitError>(())
//! ```

use std::fmt;

use syrp_audio_api::AudioError;
use syrp_error::{Diagnostic, ErrorCode};

mod render;
mod rng;
mod wav;

pub use render::{Rendered, assert_no_alloc, render_offline, render_offline_no_alloc};
pub use rng::{NoiseRng, white_noise};
pub use wav::{WavFormat, write_wav};

/// Errors from the test kit, catalogued in `errors/testkit.toml`.
/// ```
/// use syrp_error::Diagnostic;
/// let err = syrp_testkit::render_offline(|_, _| {}, &[&[0.0; 4], &[0.0; 3]], 2, 48_000.0)
///     .unwrap_err();
/// assert_eq!(err.code().as_str(), "SYRP-TESTKIT-0001");
/// ```
#[derive(Debug)]
#[non_exhaustive]
pub enum TestkitError {
    /// Input channels have different lengths (`SYRP-TESTKIT-0001`).
    #[non_exhaustive]
    RaggedInput {
        /// Index of the first channel whose length differs from channel 0.
        channel: usize,
        /// That channel's length.
        len: usize,
        /// Channel 0's length.
        expected: usize,
    },
    /// A buffer sized from the input could not be allocated (`SYRP-TESTKIT-0002`).
    #[non_exhaustive]
    OutOfMemory {
        /// Samples requested; `usize::MAX` when the size overflowed.
        samples: usize,
    },
    /// Writing a WAV file failed (`SYRP-TESTKIT-0003`).
    Io(std::io::Error),
    /// The render does not fit a WAV file: data over 4 GiB, more than 65,535 channels, or a
    /// sample rate that does not round to 1..=`u32::MAX` Hz (`SYRP-TESTKIT-0004`).
    WavLimit,
    /// The block size or sample rate was rejected by [`syrp_audio_api::ProcessConfig`]; the
    /// code is the inner `SYRP-AUDIO-*` one.
    Audio(AudioError),
}

impl fmt::Display for TestkitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TestkitError::RaggedInput {
                channel,
                len,
                expected,
            } => write!(
                f,
                "input channel {channel} has {len} samples but channel 0 has {expected}"
            ),
            TestkitError::OutOfMemory { samples } => {
                write!(f, "could not allocate a buffer of {samples} samples")
            }
            TestkitError::Io(e) => write!(f, "writing WAV failed: {e}"),
            TestkitError::WavLimit => f.write_str("render exceeds the limits of a WAV file"),
            TestkitError::Audio(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for TestkitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TestkitError::Io(e) => Some(e),
            TestkitError::Audio(e) => Some(e),
            _ => None,
        }
    }
}

impl Diagnostic for TestkitError {
    fn code(&self) -> ErrorCode {
        match self {
            TestkitError::RaggedInput { .. } => ErrorCode::new("SYRP-TESTKIT-0001"),
            TestkitError::OutOfMemory { .. } => ErrorCode::new("SYRP-TESTKIT-0002"),
            TestkitError::Io(_) => ErrorCode::new("SYRP-TESTKIT-0003"),
            TestkitError::WavLimit => ErrorCode::new("SYRP-TESTKIT-0004"),
            TestkitError::Audio(e) => e.code(),
        }
    }
}

impl From<AudioError> for TestkitError {
    fn from(e: AudioError) -> Self {
        TestkitError::Audio(e)
    }
}

impl From<std::io::Error> for TestkitError {
    fn from(e: std::io::Error) -> Self {
        TestkitError::Io(e)
    }
}

/// A zeroed `Vec<f32>` of `len` samples, allocated fallibly.
pub(crate) fn zeroed(len: usize) -> Result<Vec<f32>, TestkitError> {
    let mut v = Vec::new();
    v.try_reserve_exact(len)
        .map_err(|_| TestkitError::OutOfMemory { samples: len })?;
    v.resize(len, 0.0);
    Ok(v)
}

// SPDX-License-Identifier: MIT OR Apache-2.0
//! What a processor is prepared with (spec B3).

use syrp_rt::RtSafe;

use crate::AudioError;

/// Floating-point precision of the audio path.
/// ```
/// assert_eq!(syrp_audio_api::Precision::default(), syrp_audio_api::Precision::Single);
/// ```
#[doc(alias = "ProcessingPrecision")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Precision {
    /// `f32` samples.
    #[default]
    Single,
    /// `f64` samples.
    Double,
}

/// Whether processing runs against a real-time deadline or offline (bouncing, rendering).
/// ```
/// assert_eq!(syrp_audio_api::ProcessMode::default(), syrp_audio_api::ProcessMode::Realtime);
/// ```
#[doc(alias = "isNonRealtime")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ProcessMode {
    /// Live: each block must be done before the device needs it.
    #[default]
    Realtime,
    /// Offline: blocks may take longer or shorter than real time.
    Offline,
}

/// Sample rate, maximum block size, precision and processing mode, passed to `prepare`.
/// ```
/// use syrp_audio_api::{Precision, ProcessConfig, ProcessMode};
/// let cfg = ProcessConfig::new(44_100.0, 512)?
///     .with_precision(Precision::Double)
///     .with_mode(ProcessMode::Offline);
/// assert_eq!(cfg.max_block_size(), 512);
/// assert!(cfg.is_offline());
/// # Ok::<(), syrp_audio_api::AudioError>(())
/// ```
#[doc(alias = "dsp::ProcessSpec")]
#[doc(alias = "prepareToPlay")]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProcessConfig {
    sample_rate: f64,
    max_block_size: usize,
    precision: Precision,
    mode: ProcessMode,
}

impl ProcessConfig {
    /// A single-precision, real-time configuration. Fails with `SYRP-AUDIO-0003` unless
    /// `sample_rate` is finite and positive, and with `SYRP-AUDIO-0004` if `max_block_size`
    /// is zero.
    pub fn new(sample_rate: f64, max_block_size: usize) -> Result<Self, AudioError> {
        if !(sample_rate.is_finite() && sample_rate > 0.0) {
            return Err(AudioError::InvalidSampleRate);
        }
        if max_block_size == 0 {
            return Err(AudioError::InvalidBlockSize);
        }
        Ok(ProcessConfig {
            sample_rate,
            max_block_size,
            precision: Precision::Single,
            mode: ProcessMode::Realtime,
        })
    }

    /// The same configuration with `precision`.
    #[must_use]
    pub fn with_precision(self, precision: Precision) -> Self {
        ProcessConfig { precision, ..self }
    }

    /// The same configuration with `mode`.
    #[must_use]
    pub fn with_mode(self, mode: ProcessMode) -> Self {
        ProcessConfig { mode, ..self }
    }

    /// Samples per second.
    #[must_use]
    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    /// The largest number of frames any `process` call will be given.
    #[must_use]
    pub fn max_block_size(&self) -> usize {
        self.max_block_size
    }

    /// Sample precision.
    #[must_use]
    pub fn precision(&self) -> Precision {
        self.precision
    }

    /// Real-time or offline.
    #[must_use]
    pub fn mode(&self) -> ProcessMode {
        self.mode
    }

    /// True when processing offline.
    #[must_use]
    pub fn is_offline(&self) -> bool {
        self.mode == ProcessMode::Offline
    }
}

// SAFETY: plain `Copy` data with no destructor; no method allocates, locks or blocks.
unsafe impl RtSafe for Precision {}
// SAFETY: as above.
unsafe impl RtSafe for ProcessMode {}
// SAFETY: as above.
unsafe impl RtSafe for ProcessConfig {}

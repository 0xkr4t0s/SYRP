// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`DspSpec`]: what a processor is prepared for.

use syrp_audio_api::ProcessConfig;

/// The sample rate, largest block and channel count a processor is prepared for (JUCE's
/// `dsp::ProcessSpec`).
///
/// It is built from a validated [`ProcessConfig`], so the sample rate is finite and positive
/// and the maximum block size is at least one frame. Fields are private (ADR 0003 D3).
///
/// ```
/// use syrp_audio_api::ProcessConfig;
/// use syrp_dsp_core::DspSpec;
/// let spec = DspSpec::new(ProcessConfig::new(44_100.0, 256)?, 2);
/// assert_eq!((spec.sample_rate(), spec.max_block_size(), spec.channels()), (44_100.0, 256, 2));
/// # Ok::<(), syrp_audio_api::AudioError>(())
/// ```
#[doc(alias = "dsp::ProcessSpec")]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DspSpec {
    sample_rate: f64,
    max_block_size: usize,
    channels: usize,
}

impl DspSpec {
    /// A spec with `config`'s sample rate and maximum block size and `channels` channels.
    ///
    /// ```
    /// use syrp_audio_api::ProcessConfig;
    /// let spec = syrp_dsp_core::DspSpec::new(ProcessConfig::new(48_000.0, 32)?, 6);
    /// assert_eq!(spec.channels(), 6);
    /// # Ok::<(), syrp_audio_api::AudioError>(())
    /// ```
    #[must_use]
    pub fn new(config: ProcessConfig, channels: usize) -> Self {
        Self {
            sample_rate: config.sample_rate(),
            max_block_size: config.max_block_size(),
            channels,
        }
    }

    /// The same spec with `channels` channels.
    ///
    /// ```
    /// use syrp_audio_api::ProcessConfig;
    /// let spec = syrp_dsp_core::DspSpec::new(ProcessConfig::new(48_000.0, 32)?, 6);
    /// assert_eq!(spec.with_channels(1).channels(), 1);
    /// # Ok::<(), syrp_audio_api::AudioError>(())
    /// ```
    #[must_use]
    pub fn with_channels(self, channels: usize) -> Self {
        Self { channels, ..self }
    }

    /// Samples per second, finite and positive.
    ///
    /// ```
    /// use syrp_audio_api::ProcessConfig;
    /// let spec = syrp_dsp_core::DspSpec::new(ProcessConfig::new(96_000.0, 8)?, 1);
    /// assert_eq!(spec.sample_rate(), 96_000.0);
    /// # Ok::<(), syrp_audio_api::AudioError>(())
    /// ```
    #[must_use]
    pub fn sample_rate(self) -> f64 {
        self.sample_rate
    }

    /// The most frames one `process` call will receive (at least 1).
    ///
    /// ```
    /// use syrp_audio_api::ProcessConfig;
    /// let spec = syrp_dsp_core::DspSpec::new(ProcessConfig::new(96_000.0, 8)?, 1);
    /// assert_eq!(spec.max_block_size(), 8);
    /// # Ok::<(), syrp_audio_api::AudioError>(())
    /// ```
    #[must_use]
    pub fn max_block_size(self) -> usize {
        self.max_block_size
    }

    /// The number of channels `process` will receive.
    ///
    /// ```
    /// use syrp_audio_api::ProcessConfig;
    /// let spec = syrp_dsp_core::DspSpec::new(ProcessConfig::new(96_000.0, 8)?, 2);
    /// assert_eq!(spec.channels(), 2);
    /// # Ok::<(), syrp_audio_api::AudioError>(())
    /// ```
    #[must_use]
    pub fn channels(self) -> usize {
        self.channels
    }
}

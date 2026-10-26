// SPDX-License-Identifier: MIT OR Apache-2.0
//! Stream descriptions shared by readers and writers.

use syrp_audio_api::ChannelSet;

use crate::{FormatError, Metadata};

/// How a file stores its samples.
///
/// ```
/// use syrp_format_api::SampleKind;
/// assert_ne!(SampleKind::Int, SampleKind::Float);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SampleKind {
    /// Signed or offset integer PCM.
    Int,
    /// IEEE floating point.
    Float,
}

/// The shape of an audio stream: sample rate, channel layout, bit depth and sample kind.
///
/// It is always valid: [`AudioSpec::new`] checks every value.
///
/// ```
/// use syrp_audio_api::ChannelSet;
/// use syrp_format_api::{AudioSpec, SampleKind};
/// let spec = AudioSpec::new(44_100.0, ChannelSet::mono(), 16, SampleKind::Int)?;
/// assert_eq!(spec.bits_per_sample(), 16);
/// assert!(!spec.is_float());
/// # Ok::<(), syrp_format_api::FormatError>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioSpec {
    sample_rate: f64,
    channels: ChannelSet,
    bits_per_sample: u32,
    kind: SampleKind,
}

impl AudioSpec {
    /// Describes a stream, or returns `SYRP-FORMAT-0004` when the rate is not finite and
    /// positive, `channels` holds no channel, or `bits_per_sample` is outside 1 to 32 for
    /// [`SampleKind::Int`] or not 32 or 64 for [`SampleKind::Float`].
    ///
    /// ```
    /// use syrp_audio_api::ChannelSet;
    /// use syrp_format_api::{AudioSpec, SampleKind};
    /// assert!(AudioSpec::new(48_000.0, ChannelSet::stereo(), 32, SampleKind::Float).is_ok());
    /// assert!(AudioSpec::new(48_000.0, ChannelSet::stereo(), 24, SampleKind::Float).is_err());
    /// assert!(AudioSpec::new(0.0, ChannelSet::stereo(), 16, SampleKind::Int).is_err());
    /// assert!(AudioSpec::new(48_000.0, ChannelSet::disabled(), 16, SampleKind::Int).is_err());
    /// ```
    pub fn new(
        sample_rate: f64,
        channels: ChannelSet,
        bits_per_sample: u32,
        kind: SampleKind,
    ) -> Result<Self, FormatError> {
        let reason = if !(sample_rate.is_finite() && sample_rate > 0.0) {
            Some("sample rate must be finite and > 0")
        } else if channels.channel_count() == 0 {
            Some("a stream needs at least one channel")
        } else if !match kind {
            SampleKind::Int => (1..=32).contains(&bits_per_sample),
            SampleKind::Float => bits_per_sample == 32 || bits_per_sample == 64,
        } {
            Some("bit depth does not fit the sample kind")
        } else {
            None
        };
        match reason {
            Some(reason) => Err(FormatError::InvalidSpec { reason }),
            None => Ok(Self {
                sample_rate,
                channels,
                bits_per_sample,
                kind,
            }),
        }
    }

    /// Sample rate in Hz.
    ///
    /// ```
    /// # let spec = syrp_format_api::AudioSpec::new(96_000.0, syrp_audio_api::ChannelSet::mono(), 24, syrp_format_api::SampleKind::Int).unwrap();
    /// assert_eq!(spec.sample_rate(), 96_000.0);
    /// ```
    #[must_use]
    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    /// The channel layout, in canonical speaker order.
    ///
    /// ```
    /// # let spec = syrp_format_api::AudioSpec::new(48_000.0, syrp_audio_api::ChannelSet::surround_5_1(), 24, syrp_format_api::SampleKind::Int).unwrap();
    /// assert_eq!(spec.channels(), syrp_audio_api::ChannelSet::surround_5_1());
    /// ```
    #[must_use]
    pub fn channels(&self) -> ChannelSet {
        self.channels
    }

    /// Number of channels, always at least 1.
    ///
    /// ```
    /// # let spec = syrp_format_api::AudioSpec::new(48_000.0, syrp_audio_api::ChannelSet::discrete(3), 16, syrp_format_api::SampleKind::Int).unwrap();
    /// assert_eq!(spec.channel_count(), 3);
    /// ```
    #[must_use]
    pub fn channel_count(&self) -> usize {
        self.channels.channel_count()
    }

    /// Bits per stored sample.
    ///
    /// ```
    /// # let spec = syrp_format_api::AudioSpec::new(48_000.0, syrp_audio_api::ChannelSet::mono(), 64, syrp_format_api::SampleKind::Float).unwrap();
    /// assert_eq!(spec.bits_per_sample(), 64);
    /// ```
    #[must_use]
    pub fn bits_per_sample(&self) -> u32 {
        self.bits_per_sample
    }

    /// Whether samples are stored as integers or floats.
    ///
    /// ```
    /// # let spec = syrp_format_api::AudioSpec::new(48_000.0, syrp_audio_api::ChannelSet::mono(), 8, syrp_format_api::SampleKind::Int).unwrap();
    /// assert_eq!(spec.sample_kind(), syrp_format_api::SampleKind::Int);
    /// ```
    #[must_use]
    pub fn sample_kind(&self) -> SampleKind {
        self.kind
    }

    /// Shorthand for `sample_kind() == SampleKind::Float`.
    ///
    /// ```
    /// # let spec = syrp_format_api::AudioSpec::new(48_000.0, syrp_audio_api::ChannelSet::mono(), 32, syrp_format_api::SampleKind::Float).unwrap();
    /// assert!(spec.is_float());
    /// ```
    #[must_use]
    #[doc(alias = "usesFloatingPointData")]
    pub fn is_float(&self) -> bool {
        self.kind == SampleKind::Float
    }
}

/// What [`AudioFormat::create_writer`](crate::AudioFormat::create_writer) is asked to write:
/// the stream, the metadata to embed, and an optional encoder quality index.
///
/// ```
/// use syrp_audio_api::ChannelSet;
/// use syrp_format_api::{AudioSpec, Metadata, SampleKind, Tag, WriterOptions};
/// let spec = AudioSpec::new(48_000.0, ChannelSet::stereo(), 16, SampleKind::Int)?;
/// let mut tags = Metadata::new();
/// tags.set_tag(Tag::Title, "Take 3");
/// let options = WriterOptions::new(spec).with_metadata(tags).with_quality(5);
/// assert_eq!(options.quality(), Some(5));
/// assert_eq!(options.metadata().tag(&Tag::Title), Some("Take 3"));
/// # Ok::<(), syrp_format_api::FormatError>(())
/// ```
#[derive(Debug, Clone, PartialEq)]
#[doc(alias = "AudioFormatWriterOptions")]
pub struct WriterOptions {
    spec: AudioSpec,
    metadata: Metadata,
    quality: Option<u32>,
}

impl WriterOptions {
    /// Options for `spec`, with no metadata and the format's default quality.
    ///
    /// ```
    /// # use syrp_format_api::WriterOptions;
    /// # let spec = syrp_format_api::AudioSpec::new(48_000.0, syrp_audio_api::ChannelSet::mono(), 16, syrp_format_api::SampleKind::Int).unwrap();
    /// let options = WriterOptions::new(spec);
    /// assert!(options.metadata().is_empty());
    /// assert_eq!(options.quality(), None);
    /// ```
    #[must_use]
    pub fn new(spec: AudioSpec) -> Self {
        Self {
            spec,
            metadata: Metadata::new(),
            quality: None,
        }
    }

    /// Replaces the metadata to embed.
    ///
    /// ```
    /// # use syrp_format_api::{WriterOptions, Metadata};
    /// # let spec = syrp_format_api::AudioSpec::new(48_000.0, syrp_audio_api::ChannelSet::mono(), 16, syrp_format_api::SampleKind::Int).unwrap();
    /// let options = WriterOptions::new(spec).with_metadata(Metadata::new());
    /// assert!(options.metadata().is_empty());
    /// ```
    #[must_use]
    pub fn with_metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = metadata;
        self
    }

    /// Sets the encoder quality, an index into the format's own quality list (lossy codecs).
    ///
    /// ```
    /// # use syrp_format_api::WriterOptions;
    /// # let spec = syrp_format_api::AudioSpec::new(48_000.0, syrp_audio_api::ChannelSet::mono(), 16, syrp_format_api::SampleKind::Int).unwrap();
    /// assert_eq!(WriterOptions::new(spec).with_quality(2).quality(), Some(2));
    /// ```
    #[must_use]
    pub fn with_quality(mut self, quality: u32) -> Self {
        self.quality = Some(quality);
        self
    }

    /// The stream to write.
    ///
    /// ```
    /// # use syrp_format_api::WriterOptions;
    /// # let spec = syrp_format_api::AudioSpec::new(48_000.0, syrp_audio_api::ChannelSet::mono(), 16, syrp_format_api::SampleKind::Int).unwrap();
    /// assert_eq!(WriterOptions::new(spec).spec(), &spec);
    /// ```
    #[must_use]
    pub fn spec(&self) -> &AudioSpec {
        &self.spec
    }

    /// The metadata to embed; formats skip items they cannot store.
    ///
    /// ```
    /// # use syrp_format_api::WriterOptions;
    /// # let spec = syrp_format_api::AudioSpec::new(48_000.0, syrp_audio_api::ChannelSet::mono(), 16, syrp_format_api::SampleKind::Int).unwrap();
    /// assert_eq!(WriterOptions::new(spec).metadata().len(), 0);
    /// ```
    #[must_use]
    pub fn metadata(&self) -> &crate::Metadata {
        &self.metadata
    }

    /// The requested quality index, or `None` for the format's default.
    ///
    /// ```
    /// # use syrp_format_api::WriterOptions;
    /// # let spec = syrp_format_api::AudioSpec::new(48_000.0, syrp_audio_api::ChannelSet::mono(), 16, syrp_format_api::SampleKind::Int).unwrap();
    /// assert_eq!(WriterOptions::new(spec).quality(), None);
    /// ```
    #[must_use]
    pub fn quality(&self) -> Option<u32> {
        self.quality
    }
}

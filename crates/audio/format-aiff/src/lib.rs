// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! AIFF and AIFF-C for the Syrp audio file format contract (`syrp-format-api`).
//!
//! [`AiffAudioFormat`] reads uncompressed AIFF and AIFF-C (`NONE`, `sowt`, `fl32`, `fl64`)
//! with their markers, instrument loops and text chunks, and writes AIFF (integers) and
//! AIFF-C `fl32` (32-bit float), built from Apple's public *Audio Interchange File Format* 1.3
//! and AIFF-C specifications. [`extended_to_f64`] and
//! [`f64_to_extended`] convert the 80-bit extended sample rate.
//!
//! ```
//! use syrp_format_aiff::AiffAudioFormat;
//! use syrp_format_api::AudioFormat;
//! let aiff = AiffAudioFormat::new();
//! assert!(aiff.matches_extension("take.aif"));
//! assert!(aiff.can_read(b"FORM\0\0\0\x04AIFC"));
//! ```

mod extended;
mod metadata;
mod reader;
mod writer;

pub use extended::{extended_to_f64, f64_to_extended};

use syrp_format_api::{
    AudioFormat, AudioReader, AudioWriter, FormatError, Metadata, MetadataItem, ReadSeek,
    WriteSeek, WriterOptions,
};

/// The AIFF and AIFF-C format: big-endian chunks in a `FORM` container.
///
/// Reading accepts plain AIFF integer PCM (1 to 32 bits) and AIFF-C with compression `NONE`,
/// `sowt`, `fl32` or `fl64`; other compression types are `SYRP-FORMAT-0003`. Writing produces
/// AIFF for 8, 16, 24 and 32-bit integers and AIFF-C `fl32` for 32-bit float.
///
/// ```
/// use syrp_format_aiff::AiffAudioFormat;
/// use syrp_format_api::AudioFormat;
/// let aiff = AiffAudioFormat::new();
/// assert_eq!(aiff.name(), "AIFF file");
/// assert_eq!(aiff.bit_depths(), &[8, 16, 24, 32]);
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct AiffAudioFormat;

impl AiffAudioFormat {
    /// The format description; it holds no state.
    ///
    /// ```
    /// let aiff = syrp_format_aiff::AiffAudioFormat::new();
    /// # drop(aiff);
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// The items of `metadata` a writer would drop, in order. Syrp's choice of what AIFF
    /// cannot hold: `Bext`; tags other than title, artist, copyright and comment; cues whose
    /// id is not 1 to 32,767, whose position exceeds 32 bits, whose label exceeds 255 bytes,
    /// or whose id repeats; loops beyond the first two writable ones, backward loops, loops
    /// with a play count, or ends beyond 32 bits; `INST` items after the first or shorter
    /// than 20 bytes; and `Other` items whose id is not four bytes or is one the writer makes
    /// itself (`FORM`, `COMM`, `SSND`, `FVER`, `MARK`).
    ///
    /// ```
    /// use syrp_format_aiff::AiffAudioFormat;
    /// use syrp_format_api::{LoopKind, Metadata, MetadataItem, SampleLoop, Tag};
    /// let mut meta = Metadata::new();
    /// meta.set_tag(Tag::Title, "Pad");
    /// meta.set_tag(Tag::Album, "Sketches");
    /// meta.push(MetadataItem::Loop(SampleLoop::new(LoopKind::Backward, 0, 9).unwrap()));
    /// let dropped = AiffAudioFormat::unwritable_items(&meta);
    /// assert_eq!(dropped.len(), 2);
    /// assert!(matches!(dropped[0], MetadataItem::Tag(Tag::Album, _)));
    /// ```
    #[must_use]
    pub fn unwritable_items(metadata: &Metadata) -> Vec<&MetadataItem> {
        let items = metadata.items();
        writer::plan(metadata)
            .dropped
            .into_iter()
            .map(|i| &items[i])
            .collect()
    }
}

impl AudioFormat for AiffAudioFormat {
    fn name(&self) -> &str {
        "AIFF file"
    }

    fn file_extensions(&self) -> &[&str] {
        &["aiff", "aif", "aifc"]
    }

    /// AIFF stores any rate as an extended value, so every rate is writable (Syrp's choice:
    /// the public documentation does not list JUCE's rates).
    fn sample_rates(&self) -> &[u32] {
        &[]
    }

    /// 8, 16, 24 and 32-bit integers; 32 also covers 32-bit float (AIFF-C `fl32`).
    fn bit_depths(&self) -> &[u32] {
        &[8, 16, 24, 32]
    }

    fn can_read(&self, header: &[u8]) -> bool {
        header.len() >= 12 && &header[..4] == b"FORM" && matches!(&header[8..12], b"AIFF" | b"AIFC")
    }

    fn create_reader(&self, input: Box<dyn ReadSeek>) -> Result<Box<dyn AudioReader>, FormatError> {
        Ok(Box::new(reader::AiffReader::open(input)?))
    }

    /// Integer specs write plain AIFF; a 32-bit float spec writes AIFF-C `fl32` with `FVER`.
    /// Metadata goes before `SSND`; items in
    /// [`unwritable_items`](AiffAudioFormat::unwritable_items) are skipped.
    fn create_writer(
        &self,
        output: Box<dyn WriteSeek>,
        options: &WriterOptions,
    ) -> Result<Box<dyn AudioWriter>, FormatError> {
        self.check_spec(options.spec())?;
        let writer = writer::AiffWriter::create(output, *options.spec(), options.metadata())?;
        Ok(Box::new(writer))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syrp_audio_api::ChannelSet;
    use syrp_error::Diagnostic;
    use syrp_format_api::{AudioSpec, SampleKind};

    #[test]
    fn b1_names_extensions_and_depths() {
        let f = AiffAudioFormat::new();
        assert_eq!(f.name(), "AIFF file");
        assert_eq!(f.file_extensions(), &["aiff", "aif", "aifc"]);
        assert!(f.sample_rates().is_empty());
        assert_eq!(f.bit_depths(), &[8, 16, 24, 32]);
        for p in ["a.aiff", "b.AIF", "c.aifc"] {
            assert!(f.matches_extension(p), "{p}");
        }
        assert!(!f.matches_extension("d.wav"));
        let float = AudioSpec::new(48_000.0, ChannelSet::stereo(), 32, SampleKind::Float);
        assert!(f.check_spec(&float.unwrap()).is_ok());
        let twelve = AudioSpec::new(48_000.0, ChannelSet::mono(), 12, SampleKind::Int).unwrap();
        assert_eq!(
            f.check_spec(&twelve).unwrap_err().code().as_str(),
            "SYRP-FORMAT-0003"
        );
    }

    #[test]
    fn b1_can_read_sniffs_form_type() {
        let f = AiffAudioFormat::new();
        assert!(f.can_read(b"FORM\0\0\0\x04AIFF"));
        assert!(f.can_read(b"FORM\x7f\xff\xff\xffAIFCmore"));
        assert!(!f.can_read(b"FORM\0\0\0\x04WAVE"));
        assert!(!f.can_read(b"RIFF\0\0\0\x04AIFF"));
        assert!(!f.can_read(b"FORM\0\0\0\x04AIF"));
        assert!(!f.can_read(b""));
    }
}

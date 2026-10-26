// SPDX-License-Identifier: MIT OR Apache-2.0
//! The codec traits.

use std::io::{Read, Seek, Write};

use crate::{AnyBlock, AnyBlockMut, AudioSpec, FormatError, Metadata, WriterOptions};

/// A seekable byte source a reader decodes from. Implemented for every `Read + Seek + Send`.
///
/// ```
/// let input: Box<dyn syrp_format_api::ReadSeek> = Box::new(std::io::Cursor::new(vec![0u8; 4]));
/// # drop(input);
/// ```
pub trait ReadSeek: Read + Seek + Send {}
impl<T: Read + Seek + Send + ?Sized> ReadSeek for T {}

/// A seekable byte sink a writer encodes into (formats patch headers on `finish`).
/// Implemented for every `Write + Seek + Send`.
///
/// ```
/// let output: Box<dyn syrp_format_api::WriteSeek> = Box::new(std::io::Cursor::new(Vec::new()));
/// # drop(output);
/// ```
pub trait WriteSeek: Write + Seek + Send {}
impl<T: Write + Seek + Send + ?Sized> WriteSeek for T {}

/// One audio file format: what it is called, what it supports, and how to read and write it.
///
/// Implementations are stateless descriptions shared between threads; readers and writers
/// carry the per-file state. A format manager (`syrp-formats`) picks a format by extension or
/// by asking each one [`can_read`](AudioFormat::can_read) on the first bytes of a file.
///
/// ```
/// use syrp_format_api::{AudioFormat, AudioReader, AudioWriter, FormatError, ReadSeek,
///     WriteSeek, WriterOptions};
///
/// struct Nothing;
/// impl AudioFormat for Nothing {
///     fn name(&self) -> &str { "Nothing" }
///     fn file_extensions(&self) -> &[&str] { &["nul"] }
///     fn sample_rates(&self) -> &[u32] { &[] }
///     fn bit_depths(&self) -> &[u32] { &[16] }
///     fn can_read(&self, header: &[u8]) -> bool { header.starts_with(b"NUL") }
///     fn create_reader(&self, _: Box<dyn ReadSeek>) -> Result<Box<dyn AudioReader>, FormatError> {
///         Err(FormatError::unsupported("reading"))
///     }
///     fn create_writer(&self, _: Box<dyn WriteSeek>, _: &WriterOptions)
///         -> Result<Box<dyn AudioWriter>, FormatError> {
///         Err(FormatError::unsupported("writing"))
///     }
/// }
/// assert!(Nothing.matches_extension("take.NUL"));
/// assert!(Nothing.can_read(b"NUL\0"));
/// ```
pub trait AudioFormat: Send + Sync {
    /// Human-readable name, for example `"WAV file"`.
    ///
    /// ```
    /// fn label(f: &dyn syrp_format_api::AudioFormat) -> String { f.name().to_owned() }
    /// ```
    #[doc(alias = "getFormatName")]
    fn name(&self) -> &str;

    /// File extensions this format uses, lower case and without the dot, most usual first.
    ///
    /// ```
    /// fn first(f: &dyn syrp_format_api::AudioFormat) -> Option<&str> {
    ///     f.file_extensions().first().copied()
    /// }
    /// ```
    #[doc(alias = "getFileExtensions")]
    fn file_extensions(&self) -> &[&str];

    /// Sample rates in Hz the writer accepts, ascending; empty means any rate.
    ///
    /// ```
    /// fn any_rate(f: &dyn syrp_format_api::AudioFormat) -> bool { f.sample_rates().is_empty() }
    /// ```
    #[doc(alias = "getPossibleSampleRates")]
    fn sample_rates(&self) -> &[u32];

    /// Bit depths the writer accepts, ascending.
    ///
    /// ```
    /// fn deepest(f: &dyn syrp_format_api::AudioFormat) -> Option<u32> {
    ///     f.bit_depths().last().copied()
    /// }
    /// ```
    #[doc(alias = "getPossibleBitDepths")]
    fn bit_depths(&self) -> &[u32];

    /// Whether `header`, the first bytes of a file (at least 64 when the file is that long),
    /// looks like this format. Must not panic on any input, including an empty slice.
    ///
    /// ```
    /// fn sniff(f: &dyn syrp_format_api::AudioFormat, bytes: &[u8]) -> bool { f.can_read(bytes) }
    /// ```
    fn can_read(&self, header: &[u8]) -> bool;

    /// Opens a reader over `input`, positioned anywhere. Returns `SYRP-FORMAT-0002` when the
    /// stream is not this format and `SYRP-FORMAT-0001` when it cannot be read.
    ///
    /// ```
    /// use syrp_format_api::{AudioFormat, AudioReader, FormatError};
    /// fn open(f: &dyn AudioFormat, bytes: Vec<u8>) -> Result<Box<dyn AudioReader>, FormatError> {
    ///     f.create_reader(Box::new(std::io::Cursor::new(bytes)))
    /// }
    /// ```
    #[doc(alias = "createReaderFor")]
    fn create_reader(&self, input: Box<dyn ReadSeek>) -> Result<Box<dyn AudioReader>, FormatError>;

    /// Opens a writer that encodes `options` into `output`. Returns `SYRP-FORMAT-0003` when
    /// the format cannot store the stream ([`check_spec`](AudioFormat::check_spec) is the
    /// usual first step).
    ///
    /// ```
    /// use syrp_format_api::{AudioFormat, AudioWriter, FormatError, WriteSeek, WriterOptions};
    /// fn create(f: &dyn AudioFormat, out: Box<dyn WriteSeek>, o: &WriterOptions)
    ///     -> Result<Box<dyn AudioWriter>, FormatError> {
    ///     f.create_writer(out, o)
    /// }
    /// ```
    #[doc(alias = "createWriterFor")]
    fn create_writer(
        &self,
        output: Box<dyn WriteSeek>,
        options: &WriterOptions,
    ) -> Result<Box<dyn AudioWriter>, FormatError>;

    /// Whether `path` ends in one of [`file_extensions`](AudioFormat::file_extensions),
    /// ignoring ASCII case.
    ///
    /// ```
    /// fn is_mine(f: &dyn syrp_format_api::AudioFormat) -> bool { f.matches_extension("a.WAV") }
    /// ```
    #[doc(alias = "canHandleFile")]
    fn matches_extension(&self, path: &str) -> bool {
        path.rsplit_once('.').is_some_and(|(_, ext)| {
            self.file_extensions()
                .iter()
                .any(|e| e.eq_ignore_ascii_case(ext))
        })
    }

    /// Returns `SYRP-FORMAT-0003` unless `spec`'s rate is in
    /// [`sample_rates`](AudioFormat::sample_rates) (or that list is empty) and its bit depth is
    /// in [`bit_depths`](AudioFormat::bit_depths).
    ///
    /// ```
    /// use syrp_format_api::{AudioFormat, AudioSpec, FormatError};
    /// fn check(f: &dyn AudioFormat, s: &AudioSpec) -> Result<(), FormatError> { f.check_spec(s) }
    /// ```
    fn check_spec(&self, spec: &AudioSpec) -> Result<(), FormatError> {
        let rates = self.sample_rates();
        if !rates.is_empty() && !rates.iter().any(|&r| f64::from(r) == spec.sample_rate()) {
            return Err(FormatError::unsupported(format!(
                "sample rate {} Hz",
                spec.sample_rate()
            )));
        }
        if !self.bit_depths().contains(&spec.bits_per_sample()) {
            return Err(FormatError::unsupported(format!(
                "{}-bit samples",
                spec.bits_per_sample()
            )));
        }
        Ok(())
    }
}

/// Decodes one file with random access.
///
/// Implementors provide [`read_samples`](AudioReader::read_samples) for ranges inside the
/// file; callers use [`read`](AudioReader::read), which accepts any range and channel count
/// and zero-fills whatever the file does not cover. Readers are `Send` so a background thread
/// can own one; they are not for the audio thread (they do I/O).
///
/// ```
/// use syrp_audio_api::AudioBlockMut;
/// use syrp_format_api::{AudioReader, FormatError};
/// fn first_second(r: &mut dyn AudioReader, out: &mut [f32]) -> Result<(), FormatError> {
///     let frames = r.spec().sample_rate() as usize;
///     let block = AudioBlockMut::new(out, 1, frames, frames).expect("out holds one second");
///     r.read(block.into(), 0)
/// }
/// ```
#[doc(alias = "AudioFormatReader")]
pub trait AudioReader: Send {
    /// The stream: rate, channel layout, bit depth and sample kind.
    ///
    /// ```
    /// fn is_float(r: &dyn syrp_format_api::AudioReader) -> bool { r.spec().is_float() }
    /// ```
    fn spec(&self) -> &AudioSpec;

    /// Length of the file in frames (samples per channel).
    ///
    /// ```
    /// fn seconds(r: &dyn syrp_format_api::AudioReader) -> f64 {
    ///     r.length() as f64 / r.spec().sample_rate()
    /// }
    /// ```
    #[doc(alias = "lengthInSamples")]
    fn length(&self) -> u64;

    /// Metadata found in the file, in file order.
    ///
    /// ```
    /// use syrp_format_api::{AudioReader, Tag};
    /// fn title(r: &dyn AudioReader) -> Option<&str> { r.metadata().tag(&Tag::Title) }
    /// ```
    #[doc(alias = "metadataValues")]
    fn metadata(&self) -> &Metadata;

    /// Decodes frames `start .. start + dest.frames()` of the first `dest.channels()` channels
    /// into `dest`, converting to its sample type (integers scale to -1.0..1.0).
    ///
    /// Only [`read`](AudioReader::read) calls this, and it guarantees that the range lies
    /// inside `0..length()`, that `dest` has at least one frame, and that
    /// `dest.channels() <= spec().channel_count()`.
    ///
    /// ```
    /// use syrp_format_api::{AnyBlockMut, AudioReader, FormatError};
    /// fn raw(r: &mut dyn AudioReader, d: AnyBlockMut<'_>) -> Result<(), FormatError> {
    ///     r.read_samples(d, 0)
    /// }
    /// ```
    #[doc(alias = "readSamples")]
    fn read_samples(&mut self, dest: AnyBlockMut<'_>, start: u64) -> Result<(), FormatError>;

    /// Fills `dest` with frames `start .. start + dest.frames()` of the file. `start` may be
    /// negative or past the end: frames outside the file are zero, as are channels of `dest`
    /// beyond the file's channel count. On error the contents of `dest` are unspecified.
    ///
    /// ```
    /// use syrp_audio_api::AudioBlockMut;
    /// use syrp_format_api::{AudioReader, FormatError};
    /// fn pre_roll(r: &mut dyn AudioReader, out: &mut [f64]) -> Result<(), FormatError> {
    ///     let n = out.len();
    ///     // The first half of `out` is silence, the rest is the start of the file.
    ///     r.read(AudioBlockMut::new(out, 1, n, n).expect("mono").into(), -(n as i64 / 2))
    /// }
    /// ```
    fn read(&mut self, mut dest: AnyBlockMut<'_>, start: i64) -> Result<(), FormatError> {
        let frames = dest.frames();
        // Index in `dest` where file data begins, and where in the file that is.
        let lead = usize::try_from(start.min(0).unsigned_abs())
            .unwrap_or(usize::MAX)
            .min(frames);
        let file_start = start.max(0).unsigned_abs();
        let avail = self.length().saturating_sub(file_start);
        let n = usize::try_from(avail)
            .unwrap_or(usize::MAX)
            .min(frames - lead);
        dest.slice_frames_mut(0..lead).clear();
        dest.slice_frames_mut(lead + n..frames).clear();
        let mut mid = dest.slice_frames_mut(lead..lead + n);
        let channels = mid.channels().min(self.spec().channel_count());
        let all = mid.channels();
        mid.slice_channels_mut(channels..all).clear();
        if n == 0 || channels == 0 {
            return Ok(());
        }
        self.read_samples(mid.slice_channels_mut(0..channels), file_start)
    }
}

/// Encodes one file sequentially.
///
/// Every method returns a [`FormatError`] on I/O failure and never panics because of it.
/// Call [`finish`](AudioWriter::finish) to complete the file (headers, lengths, trailing
/// chunks) and learn whether that worked; dropping a writer without `finish` makes a
/// best-effort attempt and swallows any error.
///
/// ```
/// use syrp_audio_api::AudioBlock;
/// use syrp_format_api::{AudioWriter, FormatError};
/// fn save(mut w: Box<dyn AudioWriter>, mono: &[f32]) -> Result<(), FormatError> {
///     w.write(AudioBlock::new(mono, 1, mono.len(), mono.len()).expect("mono").into())?;
///     w.finish()
/// }
/// ```
#[doc(alias = "AudioFormatWriter")]
pub trait AudioWriter: Send {
    /// The stream being written, as given in [`WriterOptions`].
    ///
    /// ```
    /// fn channels(w: &dyn syrp_format_api::AudioWriter) -> usize { w.spec().channel_count() }
    /// ```
    fn spec(&self) -> &AudioSpec;

    /// Encodes `block`, which [`write`](AudioWriter::write) has checked has exactly
    /// `spec().channel_count()` channels and at least one frame. Samples outside -1.0..1.0
    /// are clipped for integer formats.
    ///
    /// ```
    /// use syrp_format_api::{AnyBlock, AudioWriter, FormatError};
    /// fn raw(w: &mut dyn AudioWriter, b: AnyBlock<'_>) -> Result<(), FormatError> {
    ///     w.write_samples(b)
    /// }
    /// ```
    fn write_samples(&mut self, block: AnyBlock<'_>) -> Result<(), FormatError>;

    /// Appends `block` to the file. Returns `SYRP-FORMAT-0005` when its channel count differs
    /// from the writer's, and `SYRP-FORMAT-0001` when the output fails. An empty block is a
    /// no-op.
    ///
    /// ```
    /// use syrp_audio_api::AudioBlock;
    /// use syrp_format_api::{AudioWriter, FormatError};
    /// fn stereo(w: &mut dyn AudioWriter, data: &[f32; 8]) -> Result<(), FormatError> {
    ///     w.write(AudioBlock::new(data, 2, 4, 4).expect("2 x 4").into())
    /// }
    /// ```
    #[doc(alias = "writeFromAudioSampleBuffer")]
    fn write(&mut self, block: AnyBlock<'_>) -> Result<(), FormatError> {
        let expected = self.spec().channel_count();
        if block.channels() != expected {
            return Err(FormatError::ChannelMismatch {
                expected,
                actual: block.channels(),
            });
        }
        if block.frames() == 0 {
            return Ok(());
        }
        self.write_samples(block)
    }

    /// Pushes buffered data to the output so that a reader of the file so far sees it, where
    /// the format allows.
    ///
    /// ```
    /// use syrp_format_api::{AudioWriter, FormatError};
    /// fn sync(w: &mut dyn AudioWriter) -> Result<(), FormatError> { w.flush() }
    /// ```
    fn flush(&mut self) -> Result<(), FormatError>;

    /// Completes the file and releases the output, reporting any failure.
    ///
    /// ```
    /// use syrp_format_api::{AudioWriter, FormatError};
    /// fn close(w: Box<dyn AudioWriter>) -> Result<(), FormatError> { w.finish() }
    /// ```
    fn finish(self: Box<Self>) -> Result<(), FormatError>;
}

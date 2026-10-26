// SPDX-License-Identifier: MIT OR Apache-2.0
//! Behaviour tests for WP-0.2-04, driven through a toy format that implements the traits.

use std::io::{self, Read, Seek, SeekFrom, Write};

use syrp_audio_api::{AudioBlock, AudioBlockMut, ChannelSet};
use syrp_error::Diagnostic;
use syrp_format_api::{
    AnyBlock, AnyBlockMut, AudioFormat, AudioReader, AudioSpec, AudioWriter, Bext, CuePoint,
    FormatError, LoopKind, Metadata, MetadataItem, ReadSeek, SampleKind, SampleLoop, Tag,
    WriteSeek, WriterOptions,
};

/// Toy format: `SYRAW`, channels (u16), rate (u32), frames (u64), then interleaved f32 LE.
struct Raw;
const MAGIC: &[u8] = b"SYRAW";
const HEADER: u64 = 19;

struct RawReader {
    input: Box<dyn ReadSeek>,
    spec: AudioSpec,
    length: u64,
    meta: Metadata,
}

struct RawWriter {
    output: Box<dyn WriteSeek>,
    spec: AudioSpec,
    frames: u64,
}

impl AudioFormat for Raw {
    fn name(&self) -> &str {
        "Raw float"
    }
    fn file_extensions(&self) -> &[&str] {
        &["syraw", "raw"]
    }
    fn sample_rates(&self) -> &[u32] {
        &[44_100, 48_000]
    }
    fn bit_depths(&self) -> &[u32] {
        &[32]
    }
    fn can_read(&self, header: &[u8]) -> bool {
        header.starts_with(MAGIC)
    }
    fn create_reader(
        &self,
        mut input: Box<dyn ReadSeek>,
    ) -> Result<Box<dyn AudioReader>, FormatError> {
        let mut h = [0u8; HEADER as usize];
        input.seek(SeekFrom::Start(0))?;
        input
            .read_exact(&mut h)
            .map_err(|_| FormatError::invalid_data("short header"))?;
        if !self.can_read(&h) {
            return Err(FormatError::invalid_data("bad magic"));
        }
        let ch = u16::from_le_bytes([h[5], h[6]]);
        let rate = u32::from_le_bytes(h[7..11].try_into().unwrap());
        let length = u64::from_le_bytes(h[11..19].try_into().unwrap());
        let spec = AudioSpec::new(
            f64::from(rate),
            ChannelSet::discrete(u32::from(ch)),
            32,
            SampleKind::Float,
        )?;
        Ok(Box::new(RawReader {
            input,
            spec,
            length,
            meta: Metadata::new(),
        }))
    }
    fn create_writer(
        &self,
        mut output: Box<dyn WriteSeek>,
        options: &WriterOptions,
    ) -> Result<Box<dyn AudioWriter>, FormatError> {
        let spec = *options.spec();
        self.check_spec(&spec)?;
        output.write_all(MAGIC)?;
        output.write_all(&(spec.channel_count() as u16).to_le_bytes())?;
        output.write_all(&(spec.sample_rate() as u32).to_le_bytes())?;
        output.write_all(&0u64.to_le_bytes())?;
        Ok(Box::new(RawWriter {
            output,
            spec,
            frames: 0,
        }))
    }
}

impl AudioReader for RawReader {
    fn spec(&self) -> &AudioSpec {
        &self.spec
    }
    fn length(&self) -> u64 {
        self.length
    }
    fn metadata(&self) -> &Metadata {
        &self.meta
    }
    fn read_samples(&mut self, mut dest: AnyBlockMut<'_>, start: u64) -> Result<(), FormatError> {
        let n_ch = self.spec.channel_count();
        assert!(dest.channels() <= n_ch && dest.frames() > 0);
        assert!(start + dest.frames() as u64 <= self.length);
        let frame_bytes = 4 * n_ch;
        self.input
            .seek(SeekFrom::Start(HEADER + start * frame_bytes as u64))?;
        let mut bytes = vec![0u8; dest.frames() * frame_bytes];
        self.input.read_exact(&mut bytes)?;
        let sample = |f: usize, c: usize| {
            let i = (f * n_ch + c) * 4;
            f32::from_le_bytes(bytes[i..i + 4].try_into().unwrap())
        };
        for c in 0..dest.channels() {
            match &mut dest {
                AnyBlockMut::F32(b) => {
                    for (f, s) in b.channel_mut(c).iter_mut().enumerate() {
                        *s = sample(f, c);
                    }
                }
                AnyBlockMut::F64(b) => {
                    for (f, s) in b.channel_mut(c).iter_mut().enumerate() {
                        *s = f64::from(sample(f, c));
                    }
                }
                _ => unreachable!(),
            }
        }
        Ok(())
    }
}

impl AudioWriter for RawWriter {
    fn spec(&self) -> &AudioSpec {
        &self.spec
    }
    fn write_samples(&mut self, block: AnyBlock<'_>) -> Result<(), FormatError> {
        let AnyBlock::F32(b) = block else {
            return Err(FormatError::unsupported("only f32 input in this toy"));
        };
        let mut bytes = Vec::with_capacity(b.frames() * b.channels() * 4);
        for f in 0..b.frames() {
            for c in 0..b.channels() {
                bytes.extend_from_slice(&b.channel(c)[f].to_le_bytes());
            }
        }
        self.output.write_all(&bytes)?;
        self.frames += b.frames() as u64;
        Ok(())
    }
    fn flush(&mut self) -> Result<(), FormatError> {
        Ok(self.output.flush()?)
    }
    fn finish(mut self: Box<Self>) -> Result<(), FormatError> {
        self.output.seek(SeekFrom::Start(11))?;
        self.output.write_all(&self.frames.to_le_bytes())?;
        Ok(self.output.flush()?)
    }
}

/// An output whose writes and flushes always fail.
struct Broken;
impl Write for Broken {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("disk full"))
    }
    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::other("disk full"))
    }
}
impl Seek for Broken {
    fn seek(&mut self, _: SeekFrom) -> io::Result<u64> {
        Ok(0)
    }
}

/// A `Write + Seek` that keeps its bytes visible after the writer is gone.
#[derive(Clone, Default)]
struct Shared(std::sync::Arc<std::sync::Mutex<io::Cursor<Vec<u8>>>>);
impl Write for Shared {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().write(b)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Seek for Shared {
    fn seek(&mut self, p: SeekFrom) -> io::Result<u64> {
        self.0.lock().unwrap().seek(p)
    }
}

fn stereo_spec() -> AudioSpec {
    AudioSpec::new(48_000.0, ChannelSet::stereo(), 32, SampleKind::Float).unwrap()
}

/// Writes 2 channels x 4 frames: left 1..=4, right -1..=-4.
fn sample_file() -> Vec<u8> {
    let out = Shared::default();
    let mut w = Raw
        .create_writer(Box::new(out.clone()), &WriterOptions::new(stereo_spec()))
        .unwrap();
    let data = [1.0_f32, 2.0, 3.0, 4.0, -1.0, -2.0, -3.0, -4.0];
    w.write(AudioBlock::new(&data, 2, 2, 4).unwrap().into())
        .unwrap();
    w.write(
        AudioBlock::new(&data, 2, 4, 4)
            .unwrap()
            .slice_frames(2..4)
            .into(),
    )
    .unwrap();
    w.finish().unwrap();
    out.0.lock().unwrap().get_ref().clone()
}

fn open(bytes: Vec<u8>) -> Box<dyn AudioReader> {
    Raw.create_reader(Box::new(io::Cursor::new(bytes))).unwrap()
}

#[test]
fn b1_format_describes_itself_and_sniffs_headers() {
    let f: Box<dyn AudioFormat> = Box::new(Raw);
    assert_eq!(f.name(), "Raw float");
    assert_eq!(f.file_extensions(), ["syraw", "raw"]);
    assert_eq!(f.sample_rates(), [44_100, 48_000]);
    assert_eq!(f.bit_depths(), [32]);
    assert!(f.matches_extension("dir.v2/take.SYRAW"));
    assert!(!f.matches_extension("take.wav") && !f.matches_extension("syraw"));
    assert!(f.can_read(&sample_file()[..8]));
    assert!(!f.can_read(b"RIFF....WAVE") && !f.can_read(&[]));
    let err = f
        .create_reader(Box::new(io::Cursor::new(b"RIFF0000000000000000".to_vec())))
        .err()
        .unwrap();
    assert_eq!(err.code().as_str(), "SYRP-FORMAT-0002");
    let reader = f.create_reader(Box::new(io::Cursor::new(sample_file())));
    assert_eq!(reader.unwrap().length(), 4);
}

#[test]
fn b1_check_spec_rejects_unsupported_rate_and_depth() {
    let rate = AudioSpec::new(22_050.0, ChannelSet::mono(), 32, SampleKind::Float).unwrap();
    let depth = AudioSpec::new(48_000.0, ChannelSet::mono(), 16, SampleKind::Int).unwrap();
    assert!(Raw.check_spec(&stereo_spec()).is_ok());
    for spec in [rate, depth] {
        let out: Box<dyn WriteSeek> = Box::new(io::Cursor::new(Vec::new()));
        let err = Raw
            .create_writer(out, &WriterOptions::new(spec))
            .err()
            .unwrap();
        assert_eq!(err.code().as_str(), "SYRP-FORMAT-0003");
    }
}

#[test]
fn b2_spec_reports_stream_shape() {
    let spec = AudioSpec::new(96_000.0, ChannelSet::surround_5_1(), 24, SampleKind::Int).unwrap();
    assert_eq!(spec.sample_rate(), 96_000.0);
    assert_eq!(spec.channels(), ChannelSet::surround_5_1());
    assert_eq!(spec.channel_count(), 6);
    assert_eq!(spec.bits_per_sample(), 24);
    assert_eq!(spec.sample_kind(), SampleKind::Int);
    assert!(!spec.is_float());
    let opts = WriterOptions::new(spec).with_quality(3);
    assert_eq!((opts.spec(), opts.quality()), (&spec, Some(3)));
}

#[test]
fn b2_reader_reports_stream_shape() {
    let r = open(sample_file());
    assert_eq!(r.length(), 4);
    assert_eq!(r.spec().channel_count(), 2);
    assert_eq!(r.spec().sample_rate(), 48_000.0);
    assert_eq!(r.spec().bits_per_sample(), 32);
    assert!(r.spec().is_float());
    assert_eq!(r.spec().sample_kind(), SampleKind::Float);
    assert!(r.metadata().is_empty());
}

#[test]
fn b2_read_zero_fills_before_start_after_end_and_extra_channels() {
    let mut r = open(sample_file());
    // 3 channels x 8 frames from frame -2: 2 frames lead-in, 4 frames of file, 2 past the end.
    let mut out = [9.0_f64; 24];
    r.read(AudioBlockMut::new(&mut out, 3, 8, 8).unwrap().into(), -2)
        .unwrap();
    assert_eq!(out[..8], [0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 0.0, 0.0]);
    assert_eq!(out[8..16], [0.0, 0.0, -1.0, -2.0, -3.0, -4.0, 0.0, 0.0]);
    assert_eq!(out[16..], [0.0; 8]);
}

#[test]
fn b2_read_inside_and_entirely_outside_the_file() {
    let mut r = open(sample_file());
    let mut one = [0.0_f32; 2];
    r.read(AudioBlockMut::new(&mut one, 1, 2, 2).unwrap().into(), 1)
        .unwrap();
    assert_eq!(one, [2.0, 3.0]);
    for start in [4, 1_000, i64::MAX, i64::MIN, -2] {
        let mut out = [5.0_f32; 4];
        r.read(AudioBlockMut::new(&mut out, 2, 2, 2).unwrap().into(), start)
            .unwrap();
        assert_eq!(out, [0.0; 4], "start {start}");
    }
    let mut empty: [f32; 0] = [];
    r.read(AudioBlockMut::new(&mut empty, 2, 0, 0).unwrap().into(), 0)
        .unwrap();
}

#[test]
fn b3_write_round_trips_after_finish() {
    let mut r = open(sample_file());
    let mut out = [0.0_f32; 8];
    r.read(AudioBlockMut::new(&mut out, 2, 4, 4).unwrap().into(), 0)
        .unwrap();
    assert_eq!(out, [1.0, 2.0, 3.0, 4.0, -1.0, -2.0, -3.0, -4.0]);
}

#[test]
fn b3_io_failure_is_an_error_not_a_panic() {
    let err = Raw
        .create_writer(Box::new(Broken), &WriterOptions::new(stereo_spec()))
        .err()
        .unwrap();
    assert_eq!(err.code().as_str(), "SYRP-FORMAT-0001");
    assert!(std::error::Error::source(&err).is_some());

    // A writer whose output breaks after creation.
    let mut w = RawWriter {
        output: Box::new(Broken),
        spec: stereo_spec(),
        frames: 0,
    };
    let data = [0.0_f32; 2];
    let block = AudioBlock::new(&data, 2, 1, 1).unwrap();
    assert_eq!(
        w.write(block.into()).unwrap_err().code().as_str(),
        "SYRP-FORMAT-0001"
    );
    assert_eq!(w.flush().unwrap_err().code().as_str(), "SYRP-FORMAT-0001");
    let w: Box<dyn AudioWriter> = Box::new(w);
    assert_eq!(w.finish().unwrap_err().code().as_str(), "SYRP-FORMAT-0001");
}

#[test]
fn b3_write_checks_channels_and_skips_empty_blocks() {
    let mut w = RawWriter {
        output: Box::new(Broken),
        spec: stereo_spec(),
        frames: 0,
    };
    let data = [0.0_f32; 3];
    let err = w
        .write(AudioBlock::new(&data, 3, 1, 1).unwrap().into())
        .unwrap_err();
    assert!(matches!(
        err,
        FormatError::ChannelMismatch {
            expected: 2,
            actual: 3,
            ..
        }
    ));
    assert_eq!(err.code().as_str(), "SYRP-FORMAT-0005");
    // An empty block never reaches the (broken) output.
    w.write(AudioBlock::new(&data, 2, 0, 1).unwrap().into())
        .unwrap();
}

#[test]
fn b4_metadata_holds_typed_chunks_and_an_escape() {
    let mut meta = Metadata::new();
    let bext = Bext::new()
        .with_description("dawn chorus")
        .with_originator("rec")
        .with_originator_reference("ref-1")
        .with_origination_date("2026-10-26")
        .with_origination_time("05:30:00")
        .with_time_reference(1_000)
        .with_coding_history("A=PCM,F=48000");
    meta.push(MetadataItem::Bext(bext.clone()));
    meta.push(MetadataItem::Cue(CuePoint::new(1, 10).with_label("in")));
    let lp = SampleLoop::new(LoopKind::Forward, 10, 20).unwrap();
    meta.push(MetadataItem::Loop(lp.with_play_count(3)));
    meta.set_tag(Tag::Title, "first");
    meta.set_tag(Tag::Custom("ISRC".into()), "XX0000000000");
    meta.push(MetadataItem::Other("junk".into(), vec![1, 2, 3]));
    meta.push(MetadataItem::Tag(Tag::Title, "duplicate".into()));
    meta.set_tag(Tag::Title, "second");

    assert_eq!(meta.len(), 6, "set_tag collapses duplicate titles");
    assert_eq!(meta.tag(&Tag::Title), Some("second"));
    assert!(matches!(meta.items()[3], MetadataItem::Tag(Tag::Title, _)));
    assert_eq!(meta.tag(&Tag::Custom("ISRC".into())), Some("XX0000000000"));
    assert_eq!(meta.bext(), Some(&bext));
    assert_eq!(bext.description(), "dawn chorus");
    assert_eq!(bext.originator_reference(), "ref-1");
    assert_eq!(
        (bext.origination_date(), bext.origination_time()),
        ("2026-10-26", "05:30:00")
    );
    assert_eq!(bext.coding_history(), "A=PCM,F=48000");
    assert!(
        matches!(&meta.items()[5], MetadataItem::Other(id, b) if id == "junk" && b == &[1, 2, 3])
    );
    assert!(SampleLoop::new(LoopKind::Backward, 2, 1).is_none());
    let opts = WriterOptions::new(stereo_spec()).with_metadata(meta.clone());
    assert_eq!(opts.metadata(), &meta);
}

#[test]
fn b5_every_error_code_is_catalogued() {
    let catalogue = include_str!("../../../../errors/format.toml");
    let errors = [
        FormatError::from(io::Error::other("x")),
        FormatError::invalid_data("x"),
        FormatError::unsupported("x"),
        AudioSpec::new(f64::NAN, ChannelSet::mono(), 16, SampleKind::Int).unwrap_err(),
        RawWriter {
            output: Box::new(Broken),
            spec: stereo_spec(),
            frames: 0,
        }
        .write(AudioBlock::new(&[0.0_f32], 1, 1, 1).unwrap().into())
        .unwrap_err(),
    ];
    for (i, err) in errors.iter().enumerate() {
        let code = err.code();
        assert_eq!(code.domain(), "FORMAT");
        assert_eq!(code.as_str(), format!("SYRP-FORMAT-{:04}", i + 1));
        assert!(catalogue.contains(&format!("code = \"{code}\"")), "{code}");
        assert!(!err.to_string().is_empty());
    }
    assert_eq!(catalogue.matches("[[error]]").count(), errors.len());
    assert!(std::error::Error::source(&errors[0]).is_some());
    for (bits, kind) in [
        (0, SampleKind::Int),
        (33, SampleKind::Int),
        (16, SampleKind::Float),
    ] {
        assert!(AudioSpec::new(48_000.0, ChannelSet::mono(), bits, kind).is_err());
    }
}

#[test]
fn b6_contract_types_have_no_public_fields_and_traits_are_object_safe() {
    let src = [
        include_str!("../src/lib.rs"),
        include_str!("../src/block.rs"),
        include_str!("../src/format.rs"),
        include_str!("../src/metadata.rs"),
        include_str!("../src/spec.rs"),
    ];
    let mut in_struct = false;
    for line in src.iter().flat_map(|s| s.lines()) {
        if line.starts_with("pub struct") {
            assert!(!line.contains("(pub"), "public tuple field: {line}");
            in_struct = line.ends_with('{');
        } else if line.starts_with('}') {
            in_struct = false;
        } else if in_struct {
            assert!(
                !line.trim_start().starts_with("pub"),
                "public field: {line}"
            );
        }
    }
    fn send_sync<T: Send + Sync + ?Sized>() {}
    fn send<T: Send + ?Sized>() {}
    send_sync::<dyn AudioFormat>();
    send::<dyn AudioReader>();
    send::<dyn AudioWriter>();
    send_sync::<AudioSpec>();
    send_sync::<Metadata>();
}

// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The FLAC writer: frames from `flacenc`, encoded one block at a time as samples arrive, then
//! `STREAMINFO` patched on `finish`.

use std::io::{Seek, SeekFrom, Write};

use flacenc::bitsink::ByteSink;
use flacenc::component::{BitRepr, StreamInfo};
use flacenc::config::Encoder;
use flacenc::error::{Verified, Verify};
use flacenc::source::{Context, Fill, FrameBuf};
use syrp_format_api::{AnyBlock, AudioSpec, AudioWriter, FormatError, Tag, WriteSeek};
use syrp_format_api::{MetadataItem, WriterOptions};

/// The vendor string written into every `VORBIS_COMMENT` block.
pub(crate) const VENDOR: &str = concat!("Syrp syrp-format-flac ", env!("CARGO_PKG_VERSION"));

/// Compression level used when `WriterOptions::quality` is `None`.
pub(crate) const DEFAULT_LEVEL: u32 = 5;

/// One compression level: block size, stereo decorrelation, and the LPC order (0 = no LPC).
struct Level {
    block: usize,
    stereo: bool,
    lpc: usize,
}

/// Levels 0 (fastest) to 8 (smallest), Syrp's choice of `flacenc` settings.
const LEVELS: [Level; 9] = [
    Level {
        block: 1152,
        stereo: false,
        lpc: 0,
    },
    Level {
        block: 1152,
        stereo: true,
        lpc: 0,
    },
    Level {
        block: 4096,
        stereo: true,
        lpc: 0,
    },
    Level {
        block: 4096,
        stereo: true,
        lpc: 4,
    },
    Level {
        block: 4096,
        stereo: true,
        lpc: 6,
    },
    Level {
        block: 4096,
        stereo: true,
        lpc: 8,
    },
    Level {
        block: 4096,
        stereo: true,
        lpc: 10,
    },
    Level {
        block: 4096,
        stereo: true,
        lpc: 12,
    },
    Level {
        block: 4096,
        stereo: true,
        lpc: 16,
    },
];

/// The number of compression levels.
pub(crate) const LEVEL_COUNT: usize = LEVELS.len();

fn encode_error(what: impl std::fmt::Display) -> FormatError {
    FormatError::unsupported(format!("FLAC encoder: {what}"))
}

/// The Vorbis comment field name for `tag`, or `None` for a custom name Vorbis cannot hold.
fn field_name(tag: &Tag) -> Option<&str> {
    Some(match tag {
        Tag::Title => "TITLE",
        Tag::Artist => "ARTIST",
        Tag::Album => "ALBUM",
        Tag::Genre => "GENRE",
        Tag::Date => "DATE",
        Tag::Comment => "COMMENT",
        Tag::TrackNumber => "TRACKNUMBER",
        Tag::Copyright => "COPYRIGHT",
        Tag::Encoder => "ENCODER",
        Tag::Custom(name) => name.as_str(),
        _ => return None,
    })
    .filter(|n| !n.is_empty() && n.bytes().all(|c| (0x20..=0x7D).contains(&c) && c != b'='))
}

/// Appends a little-endian length and the bytes of `s`.
fn push_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u32).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

/// The `VORBIS_COMMENT` body for the tags in `options`, in their order.
fn vorbis_body(options: &WriterOptions) -> Result<Vec<u8>, FormatError> {
    let mut fields = Vec::new();
    for item in options.metadata().items() {
        if let MetadataItem::Tag(tag, value) = item {
            let name = field_name(tag)
                .ok_or_else(|| FormatError::unsupported(format!("FLAC: tag name {tag:?}")))?;
            fields.push(format!("{name}={value}"));
        }
    }
    let mut out = Vec::new();
    push_str(&mut out, VENDOR);
    out.extend_from_slice(&(fields.len() as u32).to_le_bytes());
    for field in &fields {
        push_str(&mut out, field);
    }
    if out.len() >= 1 << 24 {
        return Err(FormatError::unsupported(
            "FLAC: tags exceed one metadata block (16 MiB)",
        ));
    }
    Ok(out)
}

pub(crate) struct FlacWriter {
    out: Option<Box<dyn WriteSeek>>,
    spec: AudioSpec,
    bits: u32,
    config: Verified<Encoder>,
    info: StreamInfo,
    frame: FrameBuf,
    context: Context,
    /// Interleaved integer samples of the block being filled.
    pending: Vec<i32>,
    block: usize,
    sink: ByteSink,
    /// Absolute offset of the `STREAMINFO` body, patched by `finish`.
    streaminfo_at: u64,
    frame_bytes: (u32, u32),
}

impl FlacWriter {
    pub(crate) fn create(
        mut out: Box<dyn WriteSeek>,
        options: &WriterOptions,
    ) -> Result<Self, FormatError> {
        let spec = *options.spec();
        let channels = spec.channel_count();
        if spec.is_float() || !(1..=8).contains(&channels) {
            return Err(FormatError::unsupported(
                "FLAC stores 1 to 8 channels of integer samples",
            ));
        }
        let level = options.quality().unwrap_or(DEFAULT_LEVEL);
        let level = usize::try_from(level)
            .ok()
            .and_then(|l| LEVELS.get(l))
            .ok_or_else(|| FormatError::unsupported(format!("FLAC quality {level} (0 to 8)")))?;
        let mut config = Encoder::default();
        config.stereo_coding.use_leftside = level.stereo;
        config.stereo_coding.use_rightside = level.stereo;
        config.stereo_coding.use_midside = level.stereo;
        config.subframe_coding.use_lpc = level.lpc > 0;
        if level.lpc > 0 {
            config.subframe_coding.qlpc.lpc_order = level.lpc;
        }
        let config = config.into_verified().map_err(|(_, e)| encode_error(e))?;
        let bits = spec.bits_per_sample();
        let rate = spec.sample_rate() as usize;
        let info = StreamInfo::new(rate, channels, bits as usize).map_err(encode_error)?;
        let frame = FrameBuf::with_size(channels, level.block).map_err(encode_error)?;
        let vorbis = vorbis_body(options)?;
        let start = out.stream_position()?;
        let mut head = b"fLaC\x00\x00\x00\x22".to_vec();
        head.extend_from_slice(&[0; 34]);
        head.push(0x84); // Last block: VORBIS_COMMENT.
        head.extend_from_slice(&(vorbis.len() as u32).to_be_bytes()[1..]);
        out.write_all(&head)?;
        out.write_all(&vorbis)?;
        let mut pending = Vec::new();
        pending
            .try_reserve_exact(level.block * channels)
            .map_err(|_| encode_error("out of memory"))?;
        Ok(Self {
            out: Some(out),
            spec,
            bits,
            config,
            info,
            frame,
            context: Context::new(bits as usize, channels),
            pending,
            block: level.block,
            sink: ByteSink::new(),
            streaminfo_at: start + 8,
            frame_bytes: (u32::MAX, 0),
        })
    }

    fn output(&mut self) -> Result<&mut Box<dyn WriteSeek>, FormatError> {
        self.out
            .as_mut()
            .ok_or_else(|| FormatError::unsupported("FLAC: the writer has finished"))
    }

    /// Encodes the pending samples (a full block, or the shorter last one) as one frame.
    fn encode_pending(&mut self) -> Result<(), FormatError> {
        (&mut self.frame, &mut self.context)
            .fill_interleaved(&self.pending)
            .map_err(encode_error)?;
        self.pending.clear();
        let number = self.context.current_frame_number().unwrap_or(0);
        let frame = flacenc::encode_fixed_size_frame(&self.config, &self.frame, number, &self.info)
            .map_err(encode_error)?;
        self.sink.clear();
        frame.write(&mut self.sink).map_err(encode_error)?;
        let size = u32::try_from(self.sink.as_slice().len()).unwrap_or(u32::MAX);
        self.frame_bytes = (self.frame_bytes.0.min(size), self.frame_bytes.1.max(size));
        let bytes = std::mem::take(&mut self.sink);
        let result = self.output()?.write_all(bytes.as_slice());
        self.sink = bytes;
        Ok(result?)
    }

    /// The 34-byte `STREAMINFO` body for what has been written.
    fn streaminfo(&self) -> Result<[u8; 34], FormatError> {
        let total = self.context.total_samples() as u64;
        if total >= 1 << 36 {
            return Err(FormatError::unsupported("FLAC: more than 2^36 frames"));
        }
        let channels = self.spec.channel_count() as u64;
        let packed = (self.spec.sample_rate() as u64) << 44
            | (channels - 1) << 41
            | u64::from(self.bits - 1) << 36
            | total;
        let block = (self.block as u16).to_be_bytes();
        let (min, max) = match self.frame_bytes {
            (u32::MAX, _) => (0, 0),
            sizes => sizes,
        };
        let mut b = [0u8; 34];
        b[..2].copy_from_slice(&block);
        b[2..4].copy_from_slice(&block);
        b[4..7].copy_from_slice(&min.to_be_bytes()[1..]);
        b[7..10].copy_from_slice(&max.to_be_bytes()[1..]);
        b[10..18].copy_from_slice(&packed.to_be_bytes());
        b[18..].copy_from_slice(&self.context.md5_digest());
        Ok(b)
    }

    fn finish_inner(&mut self) -> Result<(), FormatError> {
        if self.out.is_none() {
            return Ok(());
        }
        if !self.pending.is_empty() {
            self.encode_pending()?;
        }
        let info = self.streaminfo()?;
        let at = self.streaminfo_at;
        let mut out = self.out.take().ok_or_else(|| encode_error("no output"))?;
        let end = out.stream_position()?;
        out.seek(SeekFrom::Start(at))?;
        out.write_all(&info)?;
        out.seek(SeekFrom::Start(end))?;
        out.flush()?;
        Ok(())
    }
}

/// Rounds `x * 2^(bits - 1)` to nearest (ties away from zero), clips it to the integer range,
/// and maps NaN to 0: WP-2.1-02 B2's rule.
fn quantise(x: f64, scale: f64, max: f64) -> i32 {
    if x.is_nan() {
        0
    } else {
        (x * scale).round().clamp(-max - 1.0, max) as i32
    }
}

impl AudioWriter for FlacWriter {
    fn spec(&self) -> &AudioSpec {
        &self.spec
    }

    fn write_samples(&mut self, block: AnyBlock<'_>) -> Result<(), FormatError> {
        self.output()?;
        let scale = (1u64 << (self.bits - 1)) as f64;
        let max = scale - 1.0;
        let channels = self.spec.channel_count();
        for i in 0..block.frames() {
            for ch in 0..channels {
                let x = match block {
                    AnyBlock::F32(b) => f64::from(b.channel(ch)[i]),
                    AnyBlock::F64(b) => b.channel(ch)[i],
                    _ => return Err(FormatError::unsupported("FLAC: this sample type")),
                };
                self.pending.push(quantise(x, scale, max));
            }
            if self.pending.len() == self.block * channels {
                self.encode_pending()?;
            }
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), FormatError> {
        Ok(self.output()?.flush()?)
    }

    fn finish(mut self: Box<Self>) -> Result<(), FormatError> {
        self.finish_inner()
    }
}

impl Drop for FlacWriter {
    fn drop(&mut self) {
        let _ = self.finish_inner();
    }
}

// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The stream marker and metadata blocks, parsed from the layout in RFC 9639 (section 8).
//!
//! `claxon` 0.4 skips `SEEKTABLE`, `CUESHEET` and `PICTURE`, so this module reads every block
//! itself and hands `claxon` only the frames.

use claxon::input::ReadBytes;
use syrp_audio_api::ChannelSet;
use syrp_format_api::{AudioSpec, FormatError, Metadata, SampleKind};

use crate::source::Source;
use crate::tags;

/// The four bytes every native FLAC stream starts with.
pub(crate) const MAGIC: &[u8; 4] = b"fLaC";

/// Bit depths whose frames `claxon` 0.4 decodes: those a frame header can name itself.
pub(crate) const DECODABLE_DEPTHS: [u32; 5] = [8, 12, 16, 20, 24];

const STREAMINFO: u8 = 0;
const PADDING: u8 = 1;
const APPLICATION: u8 = 2;
const SEEKTABLE: u8 = 3;
const VORBIS_COMMENT: u8 = 4;
const CUESHEET: u8 = 5;
const PICTURE: u8 = 6;
const FORBIDDEN: u8 = 127;

/// Everything read before the first frame.
pub(crate) struct Header {
    pub(crate) spec: AudioSpec,
    pub(crate) length: u64,
    pub(crate) max_block: u32,
    pub(crate) metadata: Metadata,
    /// `(first sample, absolute byte offset)` of frames named by a `SEEKTABLE`, ascending.
    pub(crate) seek_points: Vec<(u64, u64)>,
    /// Absolute byte offset of the first frame.
    pub(crate) frames_start: u64,
}

pub(crate) fn invalid(reason: &str) -> FormatError {
    FormatError::invalid_data(format!("FLAC: {reason}"))
}

/// Maps a `claxon` or I/O failure to a coded error. Running out of bytes inside the stream
/// means the file is truncated, which is bad data rather than a failing device.
pub(crate) fn decode_error(err: claxon::Error) -> FormatError {
    match err {
        claxon::Error::IoError(e) => io_error(e),
        claxon::Error::FormatError(reason) => invalid(reason),
        claxon::Error::Unsupported(what) => FormatError::unsupported(format!("FLAC: {what}")),
    }
}

pub(crate) fn io_error(err: std::io::Error) -> FormatError {
    if err.kind() == std::io::ErrorKind::UnexpectedEof {
        invalid("the stream is truncated")
    } else {
        FormatError::Io(err)
    }
}

/// Length in bytes of an ID3v2 tag that starts `bytes`, header and footer included, or
/// `None` when `bytes` does not start with a well-formed ID3v2 header.
pub(crate) fn id3v2_len(bytes: &[u8]) -> Option<u64> {
    let h = bytes.get(..10)?;
    if &h[..3] != b"ID3" || h[3] == 0xFF || h[4] == 0xFF || h[6..].iter().any(|&b| b >= 0x80) {
        return None;
    }
    let size = h[6..].iter().fold(0u64, |acc, &b| acc << 7 | u64::from(b));
    let footer = if h[5] & 0x10 != 0 { 10 } else { 0 };
    Some(10 + size + footer)
}

/// Reads `len` bytes into a new buffer, refusing lengths that run past the stream's end.
fn read_body(src: &mut Source, len: u64) -> Result<Vec<u8>, FormatError> {
    if len > src.remaining() {
        return Err(invalid("a metadata block runs past the end of the stream"));
    }
    let len = usize::try_from(len).map_err(|_| invalid("metadata block too large"))?;
    let mut body = Vec::new();
    body.try_reserve_exact(len)
        .map_err(|_| FormatError::unsupported("FLAC: metadata block too large for memory"))?;
    body.resize(len, 0);
    src.read_into(&mut body).map_err(io_error)?;
    Ok(body)
}

/// Parses the stream marker (after an optional ID3v2 tag) and every metadata block, leaving
/// `src` at the first frame.
pub(crate) fn read_header(src: &mut Source) -> Result<Header, FormatError> {
    let mut lead = [0u8; 10];
    src.read_into(&mut lead[..4]).map_err(io_error)?;
    if lead.starts_with(b"ID3") {
        src.read_into(&mut lead[4..]).map_err(io_error)?;
        let len = id3v2_len(&lead).ok_or_else(|| invalid("malformed ID3v2 tag"))?;
        let rest = len - 10;
        if rest > src.remaining() {
            return Err(invalid("the ID3v2 tag runs past the end of the stream"));
        }
        src.seek_to(src.pos() + rest).map_err(FormatError::Io)?;
        src.read_into(&mut lead[..4]).map_err(io_error)?;
    }
    if &lead[..4] != MAGIC {
        return Err(invalid("the stream does not start with fLaC"));
    }
    let mut info: Option<(AudioSpec, u64, u32)> = None;
    let mut metadata = Metadata::new();
    let mut table = Vec::new();
    loop {
        let mut head = [0u8; 4];
        src.read_into(&mut head).map_err(io_error)?;
        let last = head[0] & 0x80 != 0;
        let kind = head[0] & 0x7F;
        let len = u64::from(u32::from_be_bytes([0, head[1], head[2], head[3]]));
        if (kind == STREAMINFO) != info.is_none() {
            return Err(invalid(
                "STREAMINFO must be the first block and appear once",
            ));
        }
        match kind {
            STREAMINFO => info = Some(parse_streaminfo(&read_body(src, len)?)?),
            PADDING => src.skip(len as u32).map_err(io_error)?,
            SEEKTABLE => table = read_body(src, len)?,
            VORBIS_COMMENT => tags::parse_vorbis(&read_body(src, len)?, &mut metadata)?,
            CUESHEET => tags::parse_cuesheet(&read_body(src, len)?, &mut metadata)?,
            APPLICATION | PICTURE => {
                tags::push_other(kind == APPLICATION, read_body(src, len)?, &mut metadata)?;
            }
            FORBIDDEN => return Err(invalid("metadata block type 127 is forbidden")),
            _ => src.skip(len as u32).map_err(io_error)?,
        }
        if last {
            break;
        }
    }
    let (spec, length, max_block) = info.ok_or_else(|| invalid("no STREAMINFO block"))?;
    let frames_start = src.pos();
    let seek_points = parse_seektable(&table, length, frames_start, src.len())?;
    Ok(Header {
        spec,
        length,
        max_block,
        metadata,
        seek_points,
        frames_start,
    })
}

/// Returns the stream description, the total length in frames and the largest block size.
fn parse_streaminfo(b: &[u8]) -> Result<(AudioSpec, u64, u32), FormatError> {
    if b.len() != 34 {
        return Err(invalid("STREAMINFO must be 34 bytes"));
    }
    let min_block = u32::from(u16::from_be_bytes([b[0], b[1]]));
    let max_block = u32::from(u16::from_be_bytes([b[2], b[3]]));
    if min_block < 16 || max_block < min_block {
        return Err(invalid("STREAMINFO block sizes are out of range"));
    }
    let mut packed = [0u8; 8];
    packed.copy_from_slice(&b[10..18]);
    let packed = u64::from_be_bytes(packed);
    let rate = (packed >> 44) as u32;
    let channels = ((packed >> 41) & 0x7) as u32 + 1;
    let bits = ((packed >> 36) & 0x1F) as u32 + 1;
    let length = packed & 0xF_FFFF_FFFF;
    if rate == 0 {
        return Err(invalid("STREAMINFO sample rate is 0"));
    }
    if bits < 4 {
        return Err(invalid("STREAMINFO bit depth is below 4"));
    }
    if !DECODABLE_DEPTHS.contains(&bits) {
        return Err(FormatError::unsupported(format!(
            "FLAC: {bits}-bit streams (claxon decodes 8, 12, 16, 20 and 24-bit)"
        )));
    }
    if length == 0 {
        return Err(FormatError::unsupported(
            "FLAC: STREAMINFO gives no total length (Syrp needs it for random access)",
        ));
    }
    let set = match channels {
        1 => ChannelSet::mono(),
        2 => ChannelSet::stereo(),
        n => ChannelSet::discrete(n),
    };
    let spec = AudioSpec::new(f64::from(rate), set, bits, SampleKind::Int)?;
    Ok((spec, length, max_block))
}

/// Checks a `SEEKTABLE` body and returns its usable points with absolute offsets.
fn parse_seektable(
    b: &[u8],
    length: u64,
    frames_start: u64,
    file_len: u64,
) -> Result<Vec<(u64, u64)>, FormatError> {
    if !b.len().is_multiple_of(18) {
        return Err(invalid("SEEKTABLE length is not a multiple of 18"));
    }
    let mut points: Vec<(u64, u64)> = Vec::new();
    points
        .try_reserve_exact(b.len() / 18)
        .map_err(|_| FormatError::unsupported("FLAC: SEEKTABLE too large for memory"))?;
    for p in b.chunks_exact(18) {
        let mut cur = Bytes(p);
        let sample = cur.u64_be()?;
        let offset = cur.u64_be()?;
        if sample == u64::MAX {
            continue; // A placeholder point.
        }
        let abs = frames_start.checked_add(offset).filter(|&a| a < file_len);
        let Some(abs) = abs.filter(|_| sample < length) else {
            return Err(invalid("a SEEKTABLE point lies outside the stream"));
        };
        if points.last().is_some_and(|&(s, o)| s >= sample || o >= abs) {
            return Err(invalid("SEEKTABLE points are not in ascending order"));
        }
        points.push((sample, abs));
    }
    Ok(points)
}

/// A bounds-checked cursor over a block body.
pub(crate) struct Bytes<'a>(pub(crate) &'a [u8]);

impl<'a> Bytes<'a> {
    pub(crate) fn take(&mut self, n: usize) -> Result<&'a [u8], FormatError> {
        if n > self.0.len() {
            return Err(invalid("a metadata block is shorter than its contents"));
        }
        let (head, tail) = self.0.split_at(n);
        self.0 = tail;
        Ok(head)
    }

    pub(crate) fn u8(&mut self) -> Result<u8, FormatError> {
        Ok(self.take(1)?[0])
    }

    pub(crate) fn u32_le(&mut self) -> Result<u32, FormatError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub(crate) fn u64_be(&mut self) -> Result<u64, FormatError> {
        let mut out = [0u8; 8];
        out.copy_from_slice(self.take(8)?);
        Ok(u64::from_be_bytes(out))
    }

    /// Rejects bytes left over after the last field.
    pub(crate) fn finish(self) -> Result<(), FormatError> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(invalid("a metadata block has bytes after its contents"))
        }
    }
}

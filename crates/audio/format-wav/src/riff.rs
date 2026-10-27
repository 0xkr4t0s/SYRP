// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The RIFF container: the chunk walk and the `fmt ` chunk.
//!
//! Layouts follow Microsoft's RIFF documentation. Every multi-byte field is little-endian.

use std::io::SeekFrom;

use syrp_format_api::ReadSeek;

use crate::WavError;

pub(crate) const RIFF: [u8; 4] = *b"RIFF";
pub(crate) const WAVE: [u8; 4] = *b"WAVE";
pub(crate) const FMT: [u8; 4] = *b"fmt ";
pub(crate) const DATA: [u8; 4] = *b"data";

pub(crate) const TAG_PCM: u16 = 1;
pub(crate) const TAG_FLOAT: u16 = 3;

/// A 32-bit size field that means "unknown" in a streamed RIFF.
const SIZE_UNSET: u32 = u32::MAX;

/// One chunk: its id, where its payload starts, and its payload size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Chunk {
    pub(crate) id: [u8; 4],
    pub(crate) offset: u64,
    pub(crate) size: u64,
}

/// The decoded `fmt ` chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Fmt {
    pub(crate) channels: u16,
    pub(crate) sample_rate: u32,
    pub(crate) block_align: u16,
    /// Bytes per sample in the frame.
    pub(crate) container_bytes: u16,
    /// Significant bits, left-justified in the container.
    pub(crate) valid_bits: u16,
    pub(crate) float: bool,
}

/// The walked file: its chunks in file order, plus the decoded format and sample data.
#[derive(Debug)]
pub(crate) struct Riff {
    #[allow(dead_code)] // Read by the metadata parser (B3, the next increment).
    pub(crate) chunks: Vec<Chunk>,
    pub(crate) fmt: Fmt,
    pub(crate) data: Chunk,
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn id_at(b: &[u8], at: usize) -> [u8; 4] {
    [b[at], b[at + 1], b[at + 2], b[at + 3]]
}

/// Reads exactly `size` payload bytes at `offset`, which the caller has checked lie inside
/// the input, reserving the buffer fallibly.
pub(crate) fn read_payload(
    input: &mut dyn ReadSeek,
    offset: u64,
    size: u64,
) -> Result<Vec<u8>, WavError> {
    let len = usize::try_from(size).map_err(|_| WavError::OutOfMemory)?;
    let mut buf = Vec::new();
    buf.try_reserve_exact(len)
        .map_err(|_| WavError::OutOfMemory)?;
    buf.resize(len, 0);
    input.seek(SeekFrom::Start(offset))?;
    input.read_exact(&mut buf)?;
    Ok(buf)
}

/// Walks the chunk list of `input` and decodes its `fmt ` chunk.
///
/// Syrp's choices where the specifications leave a case open:
/// - the walk ends at the end of the input or of the RIFF size, whichever comes first, and
///   a RIFF size of 0 or `0xFFFFFFFF` (a streamed file) means "to the end of the input";
/// - a `data` size of `0xFFFFFFFF`, or of 0 in a file whose RIFF size is also unset, means the
///   data runs to the end of the input, and the walk stops there;
/// - a `data` chunk that runs past the end of the input is cut to the bytes present;
/// - a chunk other than `fmt ` that runs past the end ends the walk without an error.
pub(crate) fn walk(input: &mut dyn ReadSeek) -> Result<Riff, WavError> {
    let len = input.seek(SeekFrom::End(0))?;
    let mut head = [0; 12];
    if len < 12 {
        return Err(WavError::NotWave);
    }
    input.seek(SeekFrom::Start(0))?;
    input.read_exact(&mut head)?;
    let magic = id_at(&head, 0);
    if magic != RIFF || id_at(&head, 8) != WAVE {
        return Err(WavError::NotWave);
    }
    let riff32 = u32_at(&head, 4);
    let streamed = riff32 == 0 || riff32 == SIZE_UNSET;
    let riff_size = u64::from(riff32);
    let end = if streamed {
        len
    } else {
        riff_size.saturating_add(8).min(len)
    };

    let mut chunks = Vec::new();
    let mut pos = 12u64;
    while pos.checked_add(8).is_some_and(|p| p <= end) {
        let mut ch = [0; 8];
        input.seek(SeekFrom::Start(pos))?;
        input.read_exact(&mut ch)?;
        let id = id_at(&ch, 0);
        let size32 = u32_at(&ch, 4);
        let offset = pos + 8;
        let mut size = u64::from(size32);
        let to_end = id == DATA && (size32 == SIZE_UNSET || (size32 == 0 && streamed));
        let remaining = len - offset;
        if to_end || (id == DATA && size > remaining) {
            size = remaining;
        }
        if chunks.try_reserve(1).is_err() {
            return Err(WavError::OutOfMemory);
        }
        chunks.push(Chunk { id, offset, size });
        if to_end {
            break;
        }
        match offset
            .checked_add(size)
            .and_then(|e| e.checked_add(size & 1))
        {
            Some(next) if next <= end => pos = next,
            _ => break,
        }
    }

    let fmt_chunk = *chunks
        .iter()
        .find(|c| c.id == FMT)
        .ok_or(WavError::MissingChunk { chunk: FMT })?;
    let data = *chunks
        .iter()
        .find(|c| c.id == DATA)
        .ok_or(WavError::MissingChunk { chunk: DATA })?;
    if fmt_chunk.size > len - fmt_chunk.offset {
        return Err(WavError::truncated(FMT));
    }
    let fmt = parse_fmt(input, fmt_chunk)?;
    Ok(Riff { chunks, fmt, data })
}

fn parse_fmt(input: &mut dyn ReadSeek, chunk: Chunk) -> Result<Fmt, WavError> {
    if chunk.size < 16 {
        return Err(WavError::bad_format("shorter than 16 bytes"));
    }
    // Only the first 16 bytes carry anything this reader uses.
    let b = read_payload(input, chunk.offset, 16)?;
    let tag = u16_at(&b, 0);
    let channels = u16_at(&b, 2);
    let sample_rate = u32_at(&b, 4);
    let block_align = u16_at(&b, 12);
    let bits = u16_at(&b, 14);
    let float = match tag {
        TAG_PCM => false,
        TAG_FLOAT => true,
        other => return Err(WavError::UnsupportedEncoding { tag: other }),
    };
    if channels == 0 {
        return Err(WavError::bad_format("zero channels"));
    }
    if sample_rate == 0 {
        return Err(WavError::bad_format("zero sample rate"));
    }
    if bits == 0 {
        return Err(WavError::bad_format("zero bits per sample"));
    }
    let container_bytes = bits.div_ceil(8);
    if float && bits != 32 && bits != 64 {
        return Err(WavError::bad_format("float samples must be 32 or 64 bits"));
    }
    if !float && container_bytes > 4 {
        return Err(WavError::bad_format("integer samples wider than 32 bits"));
    }
    if u32::from(block_align) != u32::from(channels) * u32::from(container_bytes) {
        return Err(WavError::bad_format(
            "block alignment does not match channels and sample size",
        ));
    }
    Ok(Fmt {
        channels,
        sample_rate,
        block_align,
        container_bytes,
        valid_bits: bits,
        float,
    })
}

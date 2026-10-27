// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The AIFF / AIFF-C writer and the metadata plan shared with
//! [`AiffAudioFormat::unwritable_items`](crate::AiffAudioFormat::unwritable_items).

use std::io::{self, SeekFrom};

use syrp_audio_api::{AudioBlock, AudioSample};
use syrp_format_api::{
    AnyBlock, AudioSpec, AudioWriter, FormatError, LoopKind, Metadata, MetadataItem, Tag, WriteSeek,
};

use crate::f64_to_extended;
use crate::metadata::INST_LEN;

/// AIFF-C format version 1 timestamp, from the AIFF-C specification.
const AIFC_VERSION_1: u32 = 0xA280_5140;
/// Instrument defaults when no raw `INST` is given: middle C, full key and velocity ranges.
const INST_DEFAULT: [u8; 8] = [60, 0, 0, 127, 1, 127, 0, 0];
/// Chunk ids the writer produces itself; `Other` items with these ids are dropped.
const RESERVED: [&[u8; 4]; 5] = [b"FORM", b"COMM", b"SSND", b"FVER", b"MARK"];

/// The chunks to write for some metadata, and the items that do not fit.
pub(crate) struct Plan {
    pub(crate) chunks: Vec<([u8; 4], Vec<u8>)>,
    pub(crate) dropped: Vec<usize>,
}

/// An `Other` id as four bytes: its UTF-8 bytes, or one byte per character up to U+00FF.
fn chunk_id(id: &str) -> Option<[u8; 4]> {
    if let Ok(b) = <[u8; 4]>::try_from(id.as_bytes()) {
        return Some(b);
    }
    let bytes: Vec<u8> = id
        .chars()
        .map(u8::try_from)
        .collect::<Result<_, _>>()
        .ok()?;
    <[u8; 4]>::try_from(bytes.as_slice()).ok()
}

fn push_loop(inst: &mut Vec<u8>, mode: i16, begin: i16, end: i16) {
    for v in [mode, begin, end] {
        inst.extend_from_slice(&v.to_be_bytes());
    }
}

/// Decides how `meta` is stored: `MARK` (cues, then loop markers), `INST`, then text tags
/// and `Other` items in their order. Syrp's choice for what does not fit: see the crate
/// `AGENTS.md` ("Open questions").
pub(crate) fn plan(meta: &Metadata) -> Plan {
    let mut dropped = Vec::new();
    let mut markers: Vec<(i16, u32, &[u8])> = Vec::new();
    let mut loops = Vec::new();
    let mut inst_head: Option<&[u8]> = None;
    for (i, item) in meta.items().iter().enumerate() {
        let keep = match item {
            MetadataItem::Cue(c) => {
                let label = c.label().unwrap_or("").as_bytes();
                let fits = (
                    i16::try_from(c.id()).ok().filter(|&id| id > 0),
                    u32::try_from(c.position()).ok(),
                );
                match fits {
                    (Some(id), Some(pos))
                        if label.len() <= 255
                            && markers.len() < usize::from(u16::MAX)
                            && !markers.iter().any(|m| m.0 == id) =>
                    {
                        markers.push((id, pos, label));
                        true
                    }
                    _ => false,
                }
            }
            MetadataItem::Loop(l) => {
                let mode = match l.kind() {
                    LoopKind::Forward => 1,
                    LoopKind::PingPong => 2,
                    _ => 0,
                };
                let ends = (u32::try_from(l.start()), u32::try_from(l.end()));
                if let (true, true, Ok(s), Ok(e)) = (mode > 0, l.play_count() == 0, ends.0, ends.1)
                    && loops.len() < 2
                {
                    loops.push((i, mode, s, e));
                }
                true // Decided below, once all markers are known.
            }
            MetadataItem::Other(id, bytes) if id == "INST" => {
                let first = inst_head.is_none() && bytes.len() >= INST_LEN;
                if first {
                    inst_head = Some(&bytes[..8]);
                }
                first
            }
            MetadataItem::Tag(Tag::Title | Tag::Artist | Tag::Copyright | Tag::Comment, _) => true,
            MetadataItem::Other(id, _) => chunk_id(id).is_some_and(|b| !RESERVED.contains(&&b)),
            _ => false,
        };
        if !keep {
            dropped.push(i);
        }
    }
    // Loop ends reuse a marker at the same position, or get a new unnamed one.
    let mut inst = inst_head.unwrap_or(&INST_DEFAULT).to_vec();
    let mut written = 0;
    for (i, item) in meta.items().iter().enumerate() {
        let MetadataItem::Loop(_) = item else {
            continue;
        };
        let Some(&(_, mode, s, e)) = loops.iter().find(|l| l.0 == i) else {
            dropped.push(i);
            continue;
        };
        let mut id_for = |pos: u32| {
            if let Some(m) = markers.iter().find(|m| m.1 == pos) {
                return Some(m.0);
            }
            let id = markers
                .iter()
                .map(|m| m.0)
                .max()
                .unwrap_or(0)
                .checked_add(1)?;
            markers.push((id, pos, b""));
            Some(id)
        };
        match (id_for(s), id_for(e)) {
            (Some(b), Some(en)) => {
                push_loop(&mut inst, mode, b, en);
                written += 1;
            }
            _ => dropped.push(i),
        }
    }
    dropped.sort_unstable();
    let mut chunks = Vec::new();
    if !markers.is_empty() {
        let mut body = u16::try_from(markers.len())
            .unwrap_or(u16::MAX)
            .to_be_bytes()
            .to_vec();
        for (id, pos, name) in &markers {
            body.extend_from_slice(&id.to_be_bytes());
            body.extend_from_slice(&pos.to_be_bytes());
            body.push(u8::try_from(name.len()).unwrap_or(0));
            body.extend_from_slice(name);
            if name.len() % 2 == 0 {
                body.push(0);
            }
        }
        chunks.push((*b"MARK", body));
    }
    if written > 0 || inst_head.is_some() {
        for _ in written..2 {
            push_loop(&mut inst, 0, 0, 0);
        }
        chunks.push((*b"INST", inst));
    }
    for (i, item) in meta.items().iter().enumerate() {
        if dropped.binary_search(&i).is_ok() {
            continue;
        }
        let (id, body) = match item {
            MetadataItem::Tag(tag, text) => {
                let id = match tag {
                    Tag::Title => b"NAME",
                    Tag::Artist => b"AUTH",
                    Tag::Copyright => b"(c) ",
                    _ => b"ANNO",
                };
                (*id, text.as_bytes())
            }
            MetadataItem::Other(id, bytes) if id != "INST" => {
                (chunk_id(id).unwrap_or(*b"    "), bytes.as_slice())
            }
            _ => continue,
        };
        chunks.push((id, body.to_vec()));
    }
    Plan { chunks, dropped }
}

/// How the writer stores a sample.
#[derive(Clone, Copy)]
enum Store {
    /// Big-endian two's complement of `bits` (8, 16, 24 or 32).
    Int(u32),
    /// AIFF-C `fl32`.
    F32,
}

/// Float to integer with WP-2.1-02 B2's rule: scale by 2^(bits - 1), round half away from
/// zero, clip to the integer range, NaN to 0.
fn to_int(x: f64, bits: u32) -> i32 {
    if x.is_nan() {
        return 0;
    }
    let full = f64::powi(2.0, bits.cast_signed() - 1);
    // In range after the clamp, so the cast is exact.
    #[allow(clippy::cast_possible_truncation)]
    let v = (x * full).round().clamp(-full, full - 1.0) as i32;
    v
}

/// A source sample type.
trait ToF64: AudioSample {
    fn to_f64(self) -> f64;
}

impl ToF64 for f32 {
    fn to_f64(self) -> f64 {
        f64::from(self)
    }
}

impl ToF64 for f64 {
    fn to_f64(self) -> f64 {
        self
    }
}

/// An AIFF or AIFF-C file being written.
pub(crate) struct AiffWriter {
    out: Box<dyn WriteSeek>,
    spec: AudioSpec,
    store: Store,
    /// Byte offset of the `COMM` frame count, of the `SSND` size, and of the first sample.
    comm_frames_at: u64,
    ssnd_size_at: u64,
    data_start: u64,
    frames: u64,
    data_bytes: u64,
    scratch: Vec<u8>,
    done: bool,
}

impl AiffWriter {
    /// Writes every chunk before the samples; sizes are placeholders until `patch`.
    pub(crate) fn create(
        mut out: Box<dyn WriteSeek>,
        spec: AudioSpec,
        meta: &Metadata,
    ) -> Result<Self, FormatError> {
        let channels = i16::try_from(spec.channel_count())
            .map_err(|_| FormatError::unsupported("more than 32,767 AIFF channels"))?;
        let store = if spec.is_float() {
            Store::F32
        } else {
            Store::Int(spec.bits_per_sample())
        };
        let float = matches!(store, Store::F32);
        let mut head = b"FORM\0\0\0\0".to_vec();
        head.extend_from_slice(if float { b"AIFC" } else { b"AIFF" });
        if float {
            head.extend_from_slice(b"FVER\0\0\0\x04");
            head.extend_from_slice(&AIFC_VERSION_1.to_be_bytes());
        }
        let comm_at = head.len() as u64;
        head.extend_from_slice(if float {
            b"COMM\0\0\0\x24"
        } else {
            b"COMM\0\0\0\x12"
        });
        head.extend_from_slice(&channels.to_be_bytes());
        head.extend_from_slice(&[0; 4]);
        // 8, 16, 24 or 32 (`create_writer` checked it), so the cast is exact.
        head.extend_from_slice(&(spec.bits_per_sample() as i16).to_be_bytes());
        head.extend_from_slice(&f64_to_extended(spec.sample_rate())?);
        if float {
            // Compression type and its Pascal-string name, padded to an even length.
            head.extend_from_slice(b"fl32\x0c32-bit float\0");
        }
        for (id, body) in plan(meta).chunks {
            let size = u32::try_from(body.len())
                .map_err(|_| FormatError::unsupported("a metadata chunk over 4 GiB"))?;
            head.extend_from_slice(&id);
            head.extend_from_slice(&size.to_be_bytes());
            head.extend_from_slice(&body);
            if body.len() % 2 == 1 {
                head.push(0);
            }
        }
        let ssnd_at = head.len() as u64;
        head.extend_from_slice(b"SSND\0\0\0\0\0\0\0\0\0\0\0\0");
        out.write_all(&head)?;
        Ok(Self {
            out,
            spec,
            store,
            comm_frames_at: comm_at + 10,
            ssnd_size_at: ssnd_at + 4,
            data_start: head.len() as u64,
            frames: 0,
            data_bytes: 0,
            scratch: Vec::new(),
            done: false,
        })
    }

    fn encode<T: ToF64>(&mut self, block: AudioBlock<'_, T>) -> Result<(), FormatError> {
        let bytes = match self.store {
            Store::Int(bits) => bits as usize / 8,
            Store::F32 => 4,
        };
        let len = block.frames() * block.channels() * bytes;
        let frames = self.frames + block.frames() as u64;
        // The FORM size (everything after its first 8 bytes, plus a pad byte) must fit 32 bits.
        let end = self.data_start + self.data_bytes + len as u64 + 1;
        if end - 8 > u64::from(u32::MAX) || frames > u64::from(u32::MAX) {
            return Err(FormatError::unsupported("an AIFF file over 4 GiB"));
        }
        self.scratch.clear();
        self.scratch
            .try_reserve_exact(len)
            .map_err(|_| FormatError::Io(io::ErrorKind::OutOfMemory.into()))?;
        for f in 0..block.frames() {
            for ch in 0..block.channels() {
                let x = block.channel(ch)[f].to_f64();
                match self.store {
                    Store::Int(bits) => {
                        let be = to_int(x, bits).to_be_bytes();
                        self.scratch.extend_from_slice(&be[4 - bytes..]);
                    }
                    #[allow(clippy::cast_possible_truncation)] // fl32 stores f32.
                    Store::F32 => self.scratch.extend_from_slice(&(x as f32).to_be_bytes()),
                }
            }
        }
        self.out.write_all(&self.scratch)?;
        self.frames = frames;
        self.data_bytes += len as u64;
        Ok(())
    }

    /// Writes the current sizes into the header and returns to the end of the data.
    fn patch(&mut self, pad: bool) -> Result<(), FormatError> {
        let end = self.data_start + self.data_bytes;
        let padded = end + u64::from(pad && self.data_bytes % 2 == 1);
        if padded > end {
            self.out.seek(SeekFrom::Start(end))?;
            self.out.write_all(&[0])?;
        }
        let sizes = [
            (4, padded - 8),
            (self.comm_frames_at, self.frames),
            (self.ssnd_size_at, self.data_bytes + 8),
        ];
        for (at, value) in sizes {
            self.out.seek(SeekFrom::Start(at))?;
            // `encode` keeps every size within 32 bits.
            self.out
                .write_all(&u32::try_from(value).unwrap_or(u32::MAX).to_be_bytes())?;
        }
        self.out.seek(SeekFrom::Start(end))?;
        self.out.flush()?;
        Ok(())
    }
}

impl AudioWriter for AiffWriter {
    fn spec(&self) -> &AudioSpec {
        &self.spec
    }

    fn write_samples(&mut self, block: AnyBlock<'_>) -> Result<(), FormatError> {
        match block {
            AnyBlock::F32(b) => self.encode(b),
            AnyBlock::F64(b) => self.encode(b),
            _ => Err(FormatError::unsupported("this source sample type")),
        }
    }

    /// Patches the sizes so a reader of the file so far sees every frame written.
    fn flush(&mut self) -> Result<(), FormatError> {
        self.patch(false)
    }

    fn finish(mut self: Box<Self>) -> Result<(), FormatError> {
        self.done = true;
        self.patch(true)
    }
}

impl Drop for AiffWriter {
    fn drop(&mut self) {
        if !self.done {
            let _ = self.patch(true);
        }
    }
}

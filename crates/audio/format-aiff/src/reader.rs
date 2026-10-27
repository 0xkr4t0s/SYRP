// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The AIFF / AIFF-C reader: chunk walk, `COMM` and `SSND`, and sample decoding.

use std::io::{ErrorKind, SeekFrom};

use syrp_audio_api::{AudioBlockMut, AudioSample, ChannelSet};
use syrp_format_api::{
    AnyBlockMut, AudioReader, AudioSpec, FormatError, Metadata, ReadSeek, SampleKind,
};

use crate::extended_to_f64;
use crate::metadata::{MetaBuilder, fourcc_name};

/// Bytes decoded per pass of `read_samples` (at least one frame).
const SCRATCH_BYTES: usize = 16 * 1024;

/// How one stored sample is laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Encoding {
    /// Two's complement, left-justified in `bytes`, big-endian (`bits` significant).
    IntBe { bits: u32, bytes: usize },
    /// The same, little-endian (AIFF-C `sowt`).
    IntLe { bits: u32, bytes: usize },
    /// IEEE 754 single, big-endian (`fl32`).
    F32,
    /// IEEE 754 double, big-endian (`fl64`).
    F64,
}

impl Encoding {
    fn bytes(self) -> usize {
        match self {
            Self::IntBe { bytes, .. } | Self::IntLe { bytes, .. } => bytes,
            Self::F32 => 4,
            Self::F64 => 8,
        }
    }

    fn decode(self, b: &[u8]) -> f64 {
        let int = |bits: u32, bytes: usize, le: bool| {
            let mut acc = 0u32;
            for i in 0..bytes {
                acc = (acc << 8) | u32::from(b[if le { bytes - 1 - i } else { i }]);
            }
            // Left-justify in 32 bits, then shift the significant bits down with sign.
            let wide = (acc << (32 - 8 * bytes as u32)).cast_signed();
            f64::from(wide >> (32 - bits)) * f64::powi(2.0, 1 - bits.cast_signed())
        };
        match self {
            Self::IntBe { bits, bytes } => int(bits, bytes, false),
            Self::IntLe { bits, bytes } => int(bits, bytes, true),
            Self::F32 => f64::from(f32::from_be_bytes([b[0], b[1], b[2], b[3]])),
            Self::F64 => f64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]),
        }
    }
}

/// A destination sample type.
trait FromF64: AudioSample {
    fn from_f64(v: f64) -> Self;
}

impl FromF64 for f32 {
    #[allow(clippy::cast_possible_truncation)] // Rounding to f32 is the point.
    fn from_f64(v: f64) -> Self {
        v as f32
    }
}

impl FromF64 for f64 {
    fn from_f64(v: f64) -> Self {
        v
    }
}

/// What `COMM` says.
struct Comm {
    channels: u32,
    frames: u64,
    rate: f64,
    encoding: Encoding,
}

fn bad(reason: &str) -> FormatError {
    FormatError::invalid_data(format!("AIFF: {reason}"))
}

fn be16(b: &[u8], at: usize) -> i16 {
    i16::from_be_bytes([b[at], b[at + 1]])
}

fn be32(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn parse_comm(body: &[u8], aifc: bool) -> Result<Comm, FormatError> {
    if body.len() < if aifc { 23 } else { 18 } {
        return Err(bad("COMM chunk is too short"));
    }
    let channels = u32::try_from(be16(body, 0))
        .ok()
        .filter(|&c| c > 0)
        .ok_or_else(|| bad("COMM channel count is not positive"))?;
    let frames = u64::from(be32(body, 2));
    let bits = u32::try_from(be16(body, 6)).unwrap_or(0);
    let mut rate_bytes = [0u8; 10];
    rate_bytes.copy_from_slice(&body[8..18]);
    let rate = extended_to_f64(rate_bytes)?;
    if rate.is_nan() || rate <= 0.0 {
        return Err(bad("COMM sample rate is not positive"));
    }
    let int = |le: bool| {
        if !(1..=32).contains(&bits) {
            return Err(bad("COMM sample size is outside 1 to 32 bits"));
        }
        let bytes = bits.div_ceil(8) as usize;
        Ok(if le {
            Encoding::IntLe { bits, bytes }
        } else {
            Encoding::IntBe { bits, bytes }
        })
    };
    let encoding = if aifc {
        let kind: [u8; 4] = [body[18], body[19], body[20], body[21]];
        if body.len() < 23 + usize::from(body[22]) {
            return Err(bad("COMM compression name runs past the chunk"));
        }
        let float = |enc: Encoding, want: u32| {
            if bits == want {
                Ok(enc)
            } else {
                Err(bad(
                    "COMM sample size does not match the float compression type",
                ))
            }
        };
        match &kind {
            b"NONE" => int(false)?,
            b"sowt" => int(true)?,
            b"fl32" | b"FL32" => float(Encoding::F32, 32)?,
            b"fl64" | b"FL64" => float(Encoding::F64, 64)?,
            _ => {
                return Err(FormatError::unsupported(format!(
                    "AIFF-C compression type '{}'",
                    fourcc_name(kind)
                )));
            }
        }
    } else {
        int(false)?
    };
    Ok(Comm {
        channels,
        frames,
        rate,
        encoding,
    })
}

/// Reads `len` bytes from `input`, which the caller has checked are present.
fn read_body(input: &mut dyn ReadSeek, len: u64) -> Result<Vec<u8>, FormatError> {
    let len = usize::try_from(len).map_err(|_| bad("chunk is larger than memory"))?;
    let mut body = Vec::new();
    body.try_reserve_exact(len)
        .map_err(|_| bad("chunk is larger than memory"))?;
    body.resize(len, 0);
    read_full(input, &mut body)?;
    Ok(body)
}

/// `read_exact` that reports a short stream as invalid data rather than I/O failure.
fn read_full(input: &mut dyn ReadSeek, buf: &mut [u8]) -> Result<(), FormatError> {
    input.read_exact(buf).map_err(|e| match e.kind() {
        ErrorKind::UnexpectedEof => bad("stream ended inside a chunk"),
        _ => FormatError::Io(e),
    })
}

/// An open AIFF or AIFF-C file.
pub(crate) struct AiffReader {
    input: Box<dyn ReadSeek>,
    spec: AudioSpec,
    length: u64,
    metadata: Metadata,
    encoding: Encoding,
    data_start: u64,
    scratch: Vec<u8>,
}

impl AiffReader {
    /// Walks the `FORM` and decodes everything except the sample data.
    pub(crate) fn open(mut input: Box<dyn ReadSeek>) -> Result<Self, FormatError> {
        let total = input.seek(SeekFrom::End(0))?;
        input.seek(SeekFrom::Start(0))?;
        let mut head = [0u8; 12];
        if total < 12 {
            return Err(bad("shorter than a FORM header"));
        }
        read_full(&mut *input, &mut head)?;
        if &head[..4] != b"FORM" || !matches!(&head[8..12], b"AIFF" | b"AIFC") {
            return Err(bad("not a FORM of type AIFF or AIFC"));
        }
        let aifc = &head[8..12] == b"AIFC";
        let form_size = u64::from(be32(&head, 4));
        if form_size < 4 {
            return Err(bad("FORM size is smaller than its type"));
        }
        // Syrp's choice: a FORM that claims more than the stream holds is walked as far as
        // the stream goes, and bytes after the FORM are ignored.
        let end = (8 + form_size).min(total);
        let mut pos = 12u64;
        let mut comm = None;
        let mut ssnd: Option<(u64, u64)> = None;
        let mut meta = MetaBuilder::default();
        while end - pos >= 8 {
            input.seek(SeekFrom::Start(pos))?;
            let mut ck = [0u8; 8];
            read_full(&mut *input, &mut ck)?;
            let id = [ck[0], ck[1], ck[2], ck[3]];
            let size = u64::from(be32(&ck, 4));
            let body = pos + 8;
            let avail = end - body;
            if &id == b"SSND" {
                if ssnd.is_some() {
                    return Err(bad("more than one SSND chunk"));
                }
                // Syrp's choice: an SSND that runs past the end is cut to the bytes present.
                let len = size.min(avail);
                if len < 8 {
                    return Err(bad("SSND chunk is shorter than its header"));
                }
                let mut sh = [0u8; 8];
                read_full(&mut *input, &mut sh)?;
                let offset = u64::from(be32(&sh, 0));
                if offset > len - 8 {
                    return Err(bad("SSND offset points past its data"));
                }
                ssnd = Some((body + 8 + offset, len - 8 - offset));
            } else {
                if size > avail {
                    return Err(bad("chunk runs past the end of the FORM"));
                }
                let bytes = read_body(&mut *input, size)?;
                match &id {
                    b"COMM" if comm.is_some() => return Err(bad("more than one COMM chunk")),
                    b"COMM" => comm = Some(parse_comm(&bytes, aifc)?),
                    // The format version is structure, not metadata.
                    b"FVER" if aifc => {}
                    _ => meta.chunk(id, bytes)?,
                }
            }
            pos = body.saturating_add(size).saturating_add(size & 1);
            if pos >= end {
                break;
            }
        }
        let comm = comm.ok_or_else(|| bad("no COMM chunk"))?;
        let frame_bytes = u64::from(comm.channels) * comm.encoding.bytes() as u64;
        let (data_start, length) = match ssnd {
            // Syrp's choice: the length is the smaller of COMM's frame count and the whole
            // frames SSND holds.
            Some((start, len)) => (start, comm.frames.min(len / frame_bytes)),
            None if comm.frames == 0 => (0, 0),
            None => return Err(bad("COMM counts frames but there is no SSND chunk")),
        };
        let channels = match comm.channels {
            1 => ChannelSet::mono(),
            2 => ChannelSet::stereo(),
            n => ChannelSet::discrete(n),
        };
        let (bits, kind) = match comm.encoding {
            Encoding::IntBe { bits, .. } | Encoding::IntLe { bits, .. } => (bits, SampleKind::Int),
            Encoding::F32 => (32, SampleKind::Float),
            Encoding::F64 => (64, SampleKind::Float),
        };
        let spec = AudioSpec::new(comm.rate, channels, bits, kind)?;
        Ok(Self {
            input,
            spec,
            length,
            metadata: meta.finish(),
            encoding: comm.encoding,
            data_start,
            scratch: Vec::new(),
        })
    }

    fn fill<T: FromF64>(
        &mut self,
        dest: &mut AudioBlockMut<'_, T>,
        start: u64,
    ) -> Result<(), FormatError> {
        let sample_bytes = self.encoding.bytes();
        let frame_bytes = self.spec.channel_count() * sample_bytes;
        // Never more than the file's own sample data, so tiny files get tiny buffers.
        let per_pass = (SCRATCH_BYTES / frame_bytes)
            .max(1)
            .min(usize::try_from(self.length).unwrap_or(usize::MAX));
        if self.scratch.is_empty() {
            let len = per_pass * frame_bytes;
            self.scratch
                .try_reserve_exact(len)
                .map_err(|_| FormatError::unsupported("no memory for the read buffer"))?;
            self.scratch.resize(len, 0);
        }
        let at = (frame_bytes as u64)
            .checked_mul(start)
            .and_then(|o| o.checked_add(self.data_start))
            .ok_or_else(|| bad("read position overflows"))?;
        self.input.seek(SeekFrom::Start(at))?;
        let frames = dest.frames();
        let mut done = 0;
        while done < frames {
            let n = per_pass.min(frames - done);
            let buf = &mut self.scratch[..n * frame_bytes];
            read_full(&mut *self.input, buf)?;
            for ch in 0..dest.channels() {
                let out = &mut dest.channel_mut(ch)[done..done + n];
                for (f, slot) in out.iter_mut().enumerate() {
                    let i = f * frame_bytes + ch * sample_bytes;
                    *slot = T::from_f64(self.encoding.decode(&buf[i..i + sample_bytes]));
                }
            }
            done += n;
        }
        Ok(())
    }
}

impl AudioReader for AiffReader {
    fn spec(&self) -> &AudioSpec {
        &self.spec
    }

    fn length(&self) -> u64 {
        self.length
    }

    fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    fn read_samples(&mut self, dest: AnyBlockMut<'_>, start: u64) -> Result<(), FormatError> {
        match dest {
            AnyBlockMut::F32(mut b) => self.fill(&mut b, start),
            AnyBlockMut::F64(mut b) => self.fill(&mut b, start),
            _ => Err(FormatError::unsupported("this destination sample type")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b2_decode_maps_integers_by_two_to_the_bits_minus_one() {
        let be = |bits, bytes| Encoding::IntBe { bits, bytes };
        // AIFF 8-bit is signed.
        assert_eq!(be(8, 1).decode(&[0x80]), -1.0);
        assert_eq!(be(8, 1).decode(&[0x40]), 0.5);
        assert_eq!(be(16, 2).decode(&[0x7F, 0xFF]), 32_767.0 / 32_768.0);
        assert_eq!(be(24, 3).decode(&[0xFF, 0xFF, 0xFF]), -1.0 / 8_388_608.0);
        assert_eq!(be(32, 4).decode(&[0x80, 0, 0, 0]), -1.0);
        // 12 bits left-justified in two bytes.
        assert_eq!(be(12, 2).decode(&[0x80, 0x10]), -2_047.0 / 2_048.0);
        let le = Encoding::IntLe { bits: 16, bytes: 2 };
        assert_eq!(le.decode(&[0x00, 0x40]), 0.5);
        assert_eq!(Encoding::F32.decode(&0.25f32.to_be_bytes()), 0.25);
        assert_eq!(Encoding::F64.decode(&(-0.125f64).to_be_bytes()), -0.125);
    }
}

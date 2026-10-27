// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The FLAC reader: metadata from [`crate::meta`], frames from `claxon`, random access over
//! remembered frame positions.

use claxon::frame::FrameReader;
use syrp_format_api::{AnyBlockMut, AudioReader, AudioSpec, FormatError, Metadata, ReadSeek};

use crate::meta::{self, decode_error, invalid};
use crate::source::Source;

/// Frames passed while decoding are remembered at most once per this many samples.
pub(crate) const REMEMBER_SPACING: u64 = 1 << 16;

pub(crate) struct FlacReader {
    src: Source,
    spec: AudioSpec,
    length: u64,
    bits: u32,
    max_block: u32,
    metadata: Metadata,
    /// `(first sample, absolute byte offset)` of frames the reader can restart from, ascending
    /// by sample. The first entry is always the first frame.
    known: Vec<(u64, u64)>,
    /// The last decoded frame, channel after channel, `block_frames` samples each.
    block: Vec<i32>,
    block_start: u64,
    block_frames: usize,
    /// First sample of the frame at `src`'s position, or `None` when that is unknown.
    next: Option<u64>,
    /// Frames decoded so far (tests check that forward reads decode each frame once).
    decoded: u64,
}

impl FlacReader {
    pub(crate) fn open(input: Box<dyn ReadSeek>) -> Result<Self, FormatError> {
        let mut src = Source::new(input).map_err(FormatError::Io)?;
        let header = meta::read_header(&mut src)?;
        let mut known = Vec::new();
        known
            .try_reserve_exact(header.seek_points.len() + 1)
            .map_err(|_| FormatError::unsupported("FLAC: SEEKTABLE too large for memory"))?;
        known.push((0, header.frames_start));
        known.extend(header.seek_points.iter().filter(|p| p.0 > 0));
        Ok(Self {
            src,
            bits: header.spec.bits_per_sample(),
            spec: header.spec,
            length: header.length,
            max_block: header.max_block,
            metadata: header.metadata,
            known,
            block: Vec::new(),
            block_start: 0,
            block_frames: 0,
            next: Some(0),
            decoded: 0,
        })
    }

    /// Makes sure the next decoded frame moves towards sample `s`: keeps going forward when
    /// the stream is already between the best restart point and `s`, otherwise seeks to the
    /// nearest known frame at or before `s`.
    fn position_for(&mut self, s: u64) -> Result<(), FormatError> {
        let i = self.known.partition_point(|k| k.0 <= s).max(1);
        let (sample, offset) = self.known[i - 1];
        if self.next.is_some_and(|n| n <= s && n >= sample) {
            return Ok(());
        }
        self.src.seek_to(offset).map_err(FormatError::Io)?;
        self.next = Some(sample);
        Ok(())
    }

    /// Decodes the frame at the current position into `block`.
    fn decode_next(&mut self) -> Result<(), FormatError> {
        let expected = self
            .next
            .take()
            .ok_or_else(|| invalid("lost frame position"))?;
        let offset = self.src.pos();
        self.block_frames = 0;
        let buffer = std::mem::take(&mut self.block);
        let block = FrameReader::new(&mut self.src)
            .read_next_or_eof(buffer)
            .map_err(decode_error)?
            .ok_or_else(|| invalid("the stream ends before the length STREAMINFO gives"))?;
        if block.channels() as usize != self.spec.channel_count() {
            return Err(invalid("a frame's channel count differs from STREAMINFO"));
        }
        // claxon numbers a fixed-size stream's frames by their own block size, which is only
        // right for full-size frames; a short frame is the last one, so skip the check there.
        if block.duration() == self.max_block && block.time() != expected {
            return Err(invalid("a frame is out of sequence"));
        }
        if block.duration() > self.max_block {
            return Err(invalid("a frame is longer than STREAMINFO's largest block"));
        }
        let frames = block.duration() as usize;
        let buffer = block.into_buffer();
        let limit = 1i64 << (self.bits - 1);
        if buffer
            .iter()
            .any(|&v| !(-limit..limit).contains(&i64::from(v)))
        {
            return Err(invalid("a sample exceeds the stream's bit depth"));
        }
        self.block = buffer;
        self.block_start = expected;
        self.block_frames = frames;
        self.next = Some(expected + frames as u64);
        self.decoded += 1;
        self.remember(expected, offset);
        Ok(())
    }

    /// Records a frame start for later backward seeks, sparsely (see `REMEMBER_SPACING`).
    fn remember(&mut self, sample: u64, offset: u64) {
        let i = self.known.partition_point(|k| k.0 <= sample);
        let far_from_prev = i == 0 || sample - self.known[i - 1].0 >= REMEMBER_SPACING;
        let far_from_next = self
            .known
            .get(i)
            .is_none_or(|k| k.0 - sample >= REMEMBER_SPACING);
        if far_from_prev && far_from_next && self.known.try_reserve(1).is_ok() {
            self.known.insert(i, (sample, offset));
        }
    }

    /// Copies `n` frames starting at `from` in the decoded block to `dest[at..]`, dividing by
    /// 2^(bits - 1).
    fn copy(
        &self,
        dest: &mut AnyBlockMut<'_>,
        at: usize,
        from: usize,
        n: usize,
    ) -> Result<(), FormatError> {
        let frames = self.block_frames;
        let channel = |ch: usize| &self.block[ch * frames + from..ch * frames + from + n];
        let scale = 1.0 / (1u64 << (self.bits - 1)) as f64;
        match dest {
            AnyBlockMut::F32(b) => {
                let scale = scale as f32;
                for ch in 0..b.channels() {
                    let out = &mut b.channel_mut(ch)[at..at + n];
                    for (d, &v) in out.iter_mut().zip(channel(ch)) {
                        *d = v as f32 * scale;
                    }
                }
            }
            AnyBlockMut::F64(b) => {
                for ch in 0..b.channels() {
                    let out = &mut b.channel_mut(ch)[at..at + n];
                    for (d, &v) in out.iter_mut().zip(channel(ch)) {
                        *d = f64::from(v) * scale;
                    }
                }
            }
            _ => return Err(FormatError::unsupported("FLAC: this sample type")),
        }
        Ok(())
    }
}

impl AudioReader for FlacReader {
    fn spec(&self) -> &AudioSpec {
        &self.spec
    }

    fn length(&self) -> u64 {
        self.length
    }

    fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    fn read_samples(&mut self, mut dest: AnyBlockMut<'_>, start: u64) -> Result<(), FormatError> {
        let frames = dest.frames();
        let mut done = 0;
        while done < frames {
            let s = start + done as u64;
            let end = self.block_start + self.block_frames as u64;
            if (self.block_start..end).contains(&s) {
                let from = (s - self.block_start) as usize;
                let n = (self.block_frames - from).min(frames - done);
                self.copy(&mut dest, done, from, n)?;
                done += n;
                continue;
            }
            self.position_for(s)?;
            if let Err(e) = self.decode_next() {
                self.next = None;
                return Err(e);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reader(frames: usize, block: usize) -> FlacReader {
        use flacenc::component::BitRepr;
        use flacenc::error::Verify;
        let samples: Vec<i32> = (0..frames).map(|i| (i % 2000) as i32 - 1000).collect();
        let config = flacenc::config::Encoder::default().into_verified().unwrap();
        let src = flacenc::source::MemSource::from_samples(&samples, 1, 16, 48_000);
        let stream = flacenc::encode_with_fixed_block_size(&config, src, block).unwrap();
        let mut sink = flacenc::bitsink::ByteSink::new();
        stream.write(&mut sink).unwrap();
        FlacReader::open(Box::new(std::io::Cursor::new(sink.into_inner()))).unwrap()
    }

    fn read(r: &mut FlacReader, start: i64, n: usize) {
        let mut out = vec![0.0f32; n];
        let block = syrp_audio_api::AudioBlockMut::new(&mut out, 1, n, n).unwrap();
        r.read(block.into(), start).unwrap();
    }

    #[test]
    fn b2_forward_reads_decode_each_frame_once() {
        let mut r = reader(300_000, 1000);
        for start in (0..300_000).step_by(777) {
            read(&mut r, start, 777);
        }
        assert_eq!(r.decoded, 300);
        // Far ahead without a SEEKTABLE: decode forward from where the stream is.
        let mut r = reader(300_000, 1000);
        read(&mut r, 250_500, 10);
        assert_eq!(r.decoded, 251);
    }

    #[test]
    fn b2_backward_seeks_restart_from_a_remembered_frame() {
        let mut r = reader(300_000, 1000);
        read(&mut r, 0, 300_000);
        assert_eq!(r.decoded, 300);
        assert!(r.known.len() >= 300_000 / REMEMBER_SPACING as usize);
        read(&mut r, 200_000, 1);
        // At most one spacing's worth of frames, plus the target frame.
        assert!(
            r.decoded - 300 <= REMEMBER_SPACING / 1000 + 2,
            "{}",
            r.decoded
        );
        read(&mut r, 10, 1);
        assert!(r.decoded - 300 <= 2 * (REMEMBER_SPACING / 1000 + 2));
    }
}

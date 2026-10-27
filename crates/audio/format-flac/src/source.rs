// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! A buffered byte source that knows its absolute position, so the reader can remember where
//! frames start and seek back to them. It feeds both the metadata parser and `claxon`.

use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};

use claxon::input::ReadBytes;
use syrp_format_api::ReadSeek;

/// Buffer size for the underlying `BufReader`, in bytes.
const BUFFER: usize = 16 * 1024;

pub(crate) struct Source {
    inner: BufReader<Box<dyn ReadSeek>>,
    /// Absolute offset of the next byte `read_u8` returns.
    pos: u64,
    /// Total length of the stream in bytes, measured once when opened.
    len: u64,
}

impl Source {
    /// Measures `input` and positions it at its first byte, wherever it was before.
    pub(crate) fn new(mut input: Box<dyn ReadSeek>) -> io::Result<Self> {
        let len = input.seek(SeekFrom::End(0))?;
        input.seek(SeekFrom::Start(0))?;
        Ok(Self {
            inner: BufReader::with_capacity(BUFFER, input),
            pos: 0,
            len,
        })
    }

    pub(crate) fn pos(&self) -> u64 {
        self.pos
    }

    pub(crate) fn len(&self) -> u64 {
        self.len
    }

    /// Bytes between the current position and the end of the stream.
    pub(crate) fn remaining(&self) -> u64 {
        self.len.saturating_sub(self.pos)
    }

    /// Moves to absolute offset `pos`. Always seeks, so it also recovers from a failed read
    /// that left `pos` unknown.
    pub(crate) fn seek_to(&mut self, pos: u64) -> io::Result<()> {
        self.inner.seek(SeekFrom::Start(pos))?;
        self.pos = pos;
        Ok(())
    }
}

fn eof() -> io::Error {
    io::Error::new(io::ErrorKind::UnexpectedEof, "FLAC stream ends early")
}

impl ReadBytes for Source {
    fn read_u8(&mut self) -> io::Result<u8> {
        self.read_u8_or_eof()?.ok_or_else(eof)
    }

    fn read_u8_or_eof(&mut self) -> io::Result<Option<u8>> {
        let Some(&byte) = self.inner.fill_buf()?.first() else {
            return Ok(None);
        };
        self.inner.consume(1);
        self.pos += 1;
        Ok(Some(byte))
    }

    fn read_into(&mut self, buffer: &mut [u8]) -> io::Result<()> {
        self.inner.read_exact(buffer)?;
        self.pos += buffer.len() as u64;
        Ok(())
    }

    fn skip(&mut self, amount: u32) -> io::Result<()> {
        let mut left = amount as usize;
        while left > 0 {
            let available = self.inner.fill_buf()?.len();
            if available == 0 {
                return Err(eof());
            }
            let step = available.min(left);
            self.inner.consume(step);
            self.pos += step as u64;
            left -= step;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b2_source_tracks_its_position_across_reads_skips_and_seeks() {
        let bytes: Vec<u8> = (0..=255).collect();
        let mut src = Source::new(Box::new(io::Cursor::new(bytes))).unwrap();
        assert_eq!((src.len(), src.pos()), (256, 0));
        assert_eq!(src.read_u8().unwrap(), 0);
        src.skip(9).unwrap();
        let mut four = [0; 4];
        src.read_into(&mut four).unwrap();
        assert_eq!((four, src.pos()), ([10, 11, 12, 13], 14));
        src.seek_to(255).unwrap();
        assert_eq!(src.read_u8_or_eof().unwrap(), Some(255));
        assert_eq!(src.read_u8_or_eof().unwrap(), None);
        assert_eq!(src.remaining(), 0);
        assert!(src.skip(1).is_err());
    }
}

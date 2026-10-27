//! [`BufferedInputStream`]: a read-ahead buffer in front of another [`InputStream`].

use std::io::{self, Read};

use crate::{InputStream, IoError, IoErrorKind};

/// An [`InputStream`] that reads its source in blocks of a fixed size and serves smaller reads,
/// peeks and nearby seeks from that block: JUCE's `BufferedInputStream`.
///
/// Seeking anywhere inside the buffered block, backwards included, does not touch the source.
/// A read at least as large as the buffer goes straight to the source. The stream owns its
/// source (a `Box<dyn InputStream>` works too) and gives it back with
/// [`BufferedInputStream::into_inner`].
///
/// ```
/// use syrp_io::{BufferedInputStream, InputStream, MemoryInputStream};
/// let mut s = BufferedInputStream::new(MemoryInputStream::new(b"\x00\x05abc"), 4)?;
/// assert_eq!(s.peek_byte()?, Some(0));
/// assert_eq!(s.read_i16_be()?, 5);
/// s.set_position(0)?;
/// assert_eq!(s.read_u8()?, 0);
/// # Ok::<(), syrp_io::IoError>(())
/// ```
#[derive(Debug)]
#[doc(alias = "juce::BufferedInputStream")]
pub struct BufferedInputStream<S: InputStream> {
    source: S,
    buffer: Box<[u8]>,
    /// The number of valid bytes at the front of `buffer`.
    filled: usize,
    /// The source position of `buffer[0]`.
    buffer_start: u64,
    /// Where the source itself is now.
    source_position: u64,
    /// This stream's read position.
    position: u64,
}

impl<S: InputStream> BufferedInputStream<S> {
    /// Wraps `source`, starting at its current position, with a buffer of `buffer_size` bytes
    /// (at least one; `0` is treated as `1`). Fails with `SYRP-IO-0005` when the buffer cannot
    /// be allocated.
    ///
    /// ```
    /// # use syrp_io::{BufferedInputStream, MemoryInputStream};
    /// let s = BufferedInputStream::new(MemoryInputStream::new(b"x"), 0)?;
    /// assert_eq!(s.buffer_size(), 1);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    pub fn new(source: S, buffer_size: usize) -> Result<Self, IoError> {
        let size = buffer_size.max(1);
        let mut buffer = Vec::new();
        buffer
            .try_reserve_exact(size)
            .map_err(|_| IoError::new(IoErrorKind::OutOfMemory))?;
        buffer.resize(size, 0);
        let position = source.position();
        Ok(Self {
            source,
            buffer: buffer.into_boxed_slice(),
            filled: 0,
            buffer_start: position,
            source_position: position,
            position,
        })
    }

    /// The size of the read-ahead buffer in bytes.
    ///
    /// ```
    /// # use syrp_io::{BufferedInputStream, MemoryInputStream};
    /// let s = BufferedInputStream::new(MemoryInputStream::new(b""), 512)?;
    /// assert_eq!(s.buffer_size(), 512);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    pub fn buffer_size(&self) -> usize {
        self.buffer.len()
    }

    /// The next byte without consuming it, or `None` at the end of the stream. JUCE's
    /// `peekByte` returns zero at the end; Syrp says so with `None`.
    ///
    /// ```
    /// # use syrp_io::{BufferedInputStream, InputStream, MemoryInputStream};
    /// let mut s = BufferedInputStream::new(MemoryInputStream::new(b"a"), 8)?;
    /// assert_eq!(s.peek_byte()?, Some(b'a'));
    /// assert_eq!(s.read_u8()?, b'a');
    /// assert_eq!(s.peek_byte()?, None);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    #[doc(alias = "peekByte")]
    pub fn peek_byte(&mut self) -> Result<Option<u8>, IoError> {
        if self.buffered().is_empty() {
            self.refill()?;
        }
        Ok(self.buffered().first().copied())
    }

    /// The source, by shared reference.
    ///
    /// ```
    /// # use syrp_io::{BufferedInputStream, MemoryInputStream};
    /// let s = BufferedInputStream::new(MemoryInputStream::new(b"ab"), 8)?;
    /// assert_eq!(s.get_ref().data(), b"ab");
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    pub fn get_ref(&self) -> &S {
        &self.source
    }

    /// Gives the source back. Its position is wherever the last refill left it, which can be
    /// past this stream's position; call [`InputStream::set_position`] on it before reading on.
    ///
    /// ```
    /// # use syrp_io::{BufferedInputStream, InputStream, MemoryInputStream};
    /// let mut s = BufferedInputStream::new(MemoryInputStream::new(b"abc"), 8)?;
    /// s.read_u8()?;
    /// let at = s.position();
    /// let mut source = s.into_inner();
    /// source.set_position(at)?;
    /// assert_eq!(source.read_u8()?, b'b');
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    pub fn into_inner(self) -> S {
        self.source
    }

    /// The buffered bytes from this stream's position on; empty when the position is outside
    /// the buffered block.
    fn buffered(&self) -> &[u8] {
        let valid = &self.buffer[..self.filled];
        self.position
            .checked_sub(self.buffer_start)
            .and_then(|offset| usize::try_from(offset).ok())
            .and_then(|offset| valid.get(offset..))
            .unwrap_or(&[])
    }

    /// Moves the source to this stream's position when it is somewhere else.
    fn sync_source(&mut self) -> Result<(), IoError> {
        if self.source_position != self.position {
            self.source.set_position(self.position)?;
            self.source_position = self.position;
        }
        Ok(())
    }

    /// Replaces the buffer with the source's bytes from this stream's position.
    fn refill(&mut self) -> Result<(), IoError> {
        self.sync_source()?;
        self.filled = 0;
        self.buffer_start = self.position;
        let n = loop {
            match self.source.read(&mut self.buffer) {
                Ok(n) => break n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e.into()),
            }
        };
        self.filled = n;
        self.source_position += n as u64;
        Ok(())
    }
}

impl<S: InputStream> Read for BufferedInputStream<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.buffered().is_empty() {
            if buf.len() >= self.buffer.len() {
                self.sync_source()?;
                let n = self.source.read(buf)?;
                self.source_position += n as u64;
                self.position += n as u64;
                return Ok(n);
            }
            self.refill()?;
        }
        let available = self.buffered();
        let n = available.len().min(buf.len());
        buf[..n].copy_from_slice(&available[..n]);
        self.position += n as u64;
        Ok(n)
    }
}

impl<S: InputStream> InputStream for BufferedInputStream<S> {
    fn total_length(&mut self) -> Option<u64> {
        self.source.total_length()
    }

    fn position(&self) -> u64 {
        self.position
    }

    /// Inside the buffered block (its end included) this only moves the position; elsewhere
    /// it seeks the source at once, so an unreachable position fails here, and a source that
    /// clips the position (such as a [`MemoryInputStream`](crate::MemoryInputStream)) clips
    /// this stream's too.
    fn set_position(&mut self, position: u64) -> Result<(), IoError> {
        let buffer_end = self.buffer_start + self.filled as u64;
        if (self.buffer_start..=buffer_end).contains(&position) {
            self.position = position;
            return Ok(());
        }
        self.source.set_position(position)?;
        self.source_position = self.source.position();
        self.position = self.source_position;
        Ok(())
    }

    /// Answers from the buffer when it can; otherwise looks one byte ahead in the source, so a
    /// source of unknown length still reports its end.
    fn is_exhausted(&mut self) -> bool {
        matches!(self.peek_byte(), Ok(None))
    }

    /// Moves within the buffer or seeks the source when the length is known; reads through
    /// otherwise.
    fn skip_next_bytes(&mut self, n: u64) -> Result<u64, IoError> {
        match self.total_length() {
            Some(len) => {
                let skipped = n.min(len.saturating_sub(self.position));
                self.set_position(self.position + skipped)?;
                Ok(skipped)
            }
            None => Ok(io::copy(&mut Read::take(&mut *self, n), &mut io::sink())?),
        }
    }
}

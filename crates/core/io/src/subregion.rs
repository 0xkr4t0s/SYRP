//! [`SubregionStream`]: a window onto part of another [`InputStream`].

use std::io::{self, Read};

use crate::{InputStream, IoError};

/// An [`InputStream`] that shows only the bytes `[start, start + length)` of its source, with
/// positions counted from `start`: JUCE's `SubregionStream`.
///
/// The length is clipped to what the source has when the source knows its length. Setting a
/// position past the end of the window moves to the end (Syrp's choice, as
/// [`MemoryInputStream`](crate::MemoryInputStream) does). The stream owns its source (a
/// `Box<dyn InputStream>` works too) and gives it back with [`SubregionStream::into_inner`].
///
/// ```
/// use syrp_io::{InputStream, MemoryInputStream, SubregionStream};
/// let mut s = SubregionStream::new(MemoryInputStream::new(b"head[body]tail"), 4, Some(6))?;
/// assert_eq!(s.total_length(), Some(6));
/// assert_eq!(s.read_entire_stream_as_string()?, "[body]");
/// s.set_position(1)?;
/// assert_eq!(s.read_u8()?, b'b');
/// # Ok::<(), syrp_io::IoError>(())
/// ```
#[derive(Debug)]
#[doc(alias = "juce::SubregionStream")]
pub struct SubregionStream<S: InputStream> {
    source: S,
    start: u64,
    /// The window's length; `None` runs to the end of a source of unknown length.
    length: Option<u64>,
    /// The position inside the window.
    position: u64,
}

impl<S: InputStream> SubregionStream<S> {
    /// A window of `length` bytes (to the end of the source for `None`) from `start` in
    /// `source`. Moves the source to `start`, so fails when it cannot seek there.
    ///
    /// ```
    /// # use syrp_io::{InputStream, MemoryInputStream, SubregionStream};
    /// let mut s = SubregionStream::new(MemoryInputStream::new(b"abcd"), 1, Some(99))?;
    /// assert_eq!(s.total_length(), Some(3), "clipped to the source");
    /// let mut rest = SubregionStream::new(MemoryInputStream::new(b"abcd"), 2, None)?;
    /// assert_eq!(rest.read_entire_stream_as_string()?, "cd");
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    pub fn new(mut source: S, start: u64, length: Option<u64>) -> Result<Self, IoError> {
        let available = source.total_length().map(|len| len.saturating_sub(start));
        let length = match (length, available) {
            (Some(wanted), Some(have)) => Some(wanted.min(have)),
            (wanted, have) => wanted.or(have),
        };
        source.set_position(start)?;
        Ok(Self {
            source,
            start,
            length,
            position: 0,
        })
    }

    /// Where the window starts in the source.
    ///
    /// ```
    /// # use syrp_io::{MemoryInputStream, SubregionStream};
    /// let s = SubregionStream::new(MemoryInputStream::new(b"abc"), 2, None)?;
    /// assert_eq!(s.start(), 2);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    pub fn start(&self) -> u64 {
        self.start
    }

    /// The source, by shared reference.
    ///
    /// ```
    /// # use syrp_io::{MemoryInputStream, SubregionStream};
    /// let s = SubregionStream::new(MemoryInputStream::new(b"abc"), 1, None)?;
    /// assert_eq!(s.get_ref().data(), b"abc");
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    pub fn get_ref(&self) -> &S {
        &self.source
    }

    /// Gives the source back, positioned at `start` plus this stream's position.
    ///
    /// ```
    /// # use syrp_io::{InputStream, MemoryInputStream, SubregionStream};
    /// let mut s = SubregionStream::new(MemoryInputStream::new(b"abc"), 1, None)?;
    /// s.read_u8()?;
    /// assert_eq!(s.into_inner().position(), 2);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    pub fn into_inner(self) -> S {
        self.source
    }

    /// The bytes left in the window, or `None` when the window has no known end.
    fn remaining(&self) -> Option<u64> {
        self.length.map(|len| len.saturating_sub(self.position))
    }
}

impl<S: InputStream> Read for SubregionStream<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let want = match self.remaining() {
            Some(left) => buf.len().min(usize::try_from(left).unwrap_or(usize::MAX)),
            None => buf.len(),
        };
        let n = self.source.read(&mut buf[..want])?;
        self.position += n as u64;
        Ok(n)
    }
}

impl<S: InputStream> InputStream for SubregionStream<S> {
    fn total_length(&mut self) -> Option<u64> {
        self.length
    }

    fn position(&self) -> u64 {
        self.position
    }

    fn set_position(&mut self, position: u64) -> Result<(), IoError> {
        let position = self.length.map_or(position, |len| position.min(len));
        self.source
            .set_position(self.start.saturating_add(position))?;
        self.position = position;
        Ok(())
    }

    /// At the end of the window, or, for a window with no known end, when the source is.
    fn is_exhausted(&mut self) -> bool {
        match self.remaining() {
            Some(left) => left == 0,
            None => self.source.is_exhausted(),
        }
    }
}

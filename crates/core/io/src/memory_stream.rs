//! [`MemoryInputStream`] and [`MemoryOutputStream`]: streams over bytes in memory.

use std::borrow::Cow;
use std::io::{self, Read, Write};

use crate::{DEFAULT_NEW_LINE, InputStream, IoError, IoErrorKind, MemoryBlock, OutputStream};

/// An [`InputStream`] over bytes in memory: JUCE's `MemoryInputStream`.
///
/// It borrows a slice ([`MemoryInputStream::new`]) or owns its bytes
/// ([`MemoryInputStream::from_vec`], [`MemoryInputStream::from_block`]; JUCE's "keep internal
/// copy"). The length is always known. [`InputStream::set_position`] past the end moves to the
/// end and succeeds (Syrp's choice, as JUCE clips too).
///
/// ```
/// use syrp_io::{InputStream, MemoryInputStream};
/// let mut s = MemoryInputStream::new(b"\x01\x00ok");
/// assert_eq!(s.read_i16_le()?, 1);
/// assert_eq!(s.read_entire_stream_as_string()?, "ok");
/// # Ok::<(), syrp_io::IoError>(())
/// ```
#[derive(Debug, Clone)]
#[doc(alias = "juce::MemoryInputStream")]
pub struct MemoryInputStream<'a> {
    data: Cow<'a, [u8]>,
    position: usize,
}

impl<'a> MemoryInputStream<'a> {
    /// A stream over `data`, borrowed.
    ///
    /// ```
    /// # use syrp_io::{InputStream, MemoryInputStream};
    /// assert_eq!(MemoryInputStream::new(&[1, 2]).total_length(), Some(2));
    /// ```
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data: Cow::Borrowed(data),
            position: 0,
        }
    }

    /// All the bytes the stream reads from, whatever the position.
    ///
    /// ```
    /// # use syrp_io::{InputStream, MemoryInputStream};
    /// let mut s = MemoryInputStream::new(&[1, 2]);
    /// s.read_u8()?;
    /// assert_eq!(s.data(), &[1, 2]);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

impl MemoryInputStream<'static> {
    /// A stream that owns `data`.
    ///
    /// ```
    /// # use syrp_io::{InputStream, MemoryInputStream};
    /// let mut s = MemoryInputStream::from_vec(vec![7]);
    /// assert_eq!(s.read_u8()?, 7);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    pub fn from_vec(data: Vec<u8>) -> Self {
        Self {
            data: Cow::Owned(data),
            position: 0,
        }
    }

    /// A stream that owns the bytes of `block`, without copying them.
    ///
    /// ```
    /// # use syrp_io::{InputStream, MemoryBlock, MemoryInputStream};
    /// let mut s = MemoryInputStream::from_block(MemoryBlock::from_slice(b"hi"));
    /// assert_eq!(s.read_entire_stream_as_string()?, "hi");
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    pub fn from_block(block: MemoryBlock) -> Self {
        Self::from_vec(block.into_vec())
    }
}

impl Read for MemoryInputStream<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let rest = &self.data[self.position..];
        let n = rest.len().min(buf.len());
        buf[..n].copy_from_slice(&rest[..n]);
        self.position += n;
        Ok(n)
    }
}

impl InputStream for MemoryInputStream<'_> {
    fn total_length(&mut self) -> Option<u64> {
        Some(self.data.len() as u64)
    }

    fn position(&self) -> u64 {
        self.position as u64
    }

    fn set_position(&mut self, position: u64) -> Result<(), IoError> {
        self.position =
            usize::try_from(position).map_or(self.data.len(), |p| p.min(self.data.len()));
        Ok(())
    }
}

/// Where a [`MemoryOutputStream`] puts its bytes.
#[derive(Debug)]
enum Target<'a> {
    Owned(MemoryBlock),
    Block(&'a mut MemoryBlock),
    Fixed(&'a mut [u8]),
}

/// An [`OutputStream`] into memory: JUCE's `MemoryOutputStream`.
///
/// It writes into its own growing [`MemoryBlock`] ([`MemoryOutputStream::new`]), onto the end
/// of a caller's block ([`MemoryOutputStream::appending_to`]) or into a caller's fixed slice
/// ([`MemoryOutputStream::over_slice`]). A write that does not fit in a fixed slice writes
/// nothing and returns `SYRP-IO-0002`. [`MemoryOutputStream::len`] is the end of the written
/// data; writing before it overwrites.
///
/// Syrp's choice for [`OutputStream::set_position`] past [`MemoryOutputStream::len`]: the data
/// is extended to the new position with zero bytes at once, so `len` becomes that position.
/// For a fixed slice, a position past the slice's end is `SYRP-IO-0002`.
///
/// ```
/// use syrp_io::{MemoryOutputStream, OutputStream};
/// let mut s = MemoryOutputStream::new();
/// s.write_i16_be(1)?;
/// s.set_position(4)?;
/// assert_eq!(s.data(), &[0, 1, 0, 0]);
/// # Ok::<(), syrp_io::IoError>(())
/// ```
#[derive(Debug)]
#[doc(alias = "juce::MemoryOutputStream")]
pub struct MemoryOutputStream<'a> {
    target: Target<'a>,
    /// Where the stream's data starts in the target (non-zero only for `appending_to`).
    base: usize,
    position: usize,
    size: usize,
    new_line: String,
}

impl Default for MemoryOutputStream<'static> {
    /// Same as [`MemoryOutputStream::new`].
    ///
    /// ```
    /// assert!(syrp_io::MemoryOutputStream::default().is_empty());
    /// ```
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryOutputStream<'static> {
    /// A stream that writes into its own growing [`MemoryBlock`].
    ///
    /// ```
    /// # use syrp_io::{MemoryOutputStream, OutputStream};
    /// let mut s = MemoryOutputStream::new();
    /// s.write_string("a")?;
    /// assert_eq!(s.data(), b"a\0");
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    pub fn new() -> Self {
        Self::with_target(Target::Owned(MemoryBlock::new()), 0)
    }
}

impl<'a> MemoryOutputStream<'a> {
    fn with_target(target: Target<'a>, base: usize) -> Self {
        Self {
            target,
            base,
            position: 0,
            size: 0,
            new_line: DEFAULT_NEW_LINE.to_owned(),
        }
    }

    /// A stream that appends to `block`, starting at its current size. The stream's data,
    /// position and length do not include the bytes already in `block`.
    ///
    /// ```
    /// # use syrp_io::{MemoryBlock, MemoryOutputStream, OutputStream};
    /// let mut block = MemoryBlock::from_slice(b"ab");
    /// let mut s = MemoryOutputStream::appending_to(&mut block);
    /// s.write_u8(b'c')?;
    /// assert_eq!(s.data(), b"c");
    /// assert_eq!(&block[..], b"abc");
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    pub fn appending_to(block: &'a mut MemoryBlock) -> Self {
        let base = block.len();
        Self::with_target(Target::Block(block), base)
    }

    /// A stream that writes into `buffer` and never grows it.
    ///
    /// ```
    /// # use syrp_io::{IoErrorKind, MemoryOutputStream, OutputStream};
    /// let mut buf = [0u8; 2];
    /// let mut s = MemoryOutputStream::over_slice(&mut buf);
    /// s.write_u8(1)?;
    /// assert_eq!(s.write_i16_le(2).unwrap_err().kind(), IoErrorKind::BufferFull);
    /// assert_eq!(s.data(), &[1]);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    pub fn over_slice(buffer: &'a mut [u8]) -> Self {
        Self::with_target(Target::Fixed(buffer), 0)
    }

    /// The bytes written so far (up to [`MemoryOutputStream::len`]).
    ///
    /// ```
    /// # use syrp_io::{MemoryOutputStream, OutputStream};
    /// let mut s = MemoryOutputStream::new();
    /// s.write_bool(true)?;
    /// assert_eq!(s.data(), &[1]);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    #[doc(alias = "getData")]
    pub fn data(&self) -> &[u8] {
        let end = self.base + self.size;
        match &self.target {
            Target::Owned(block) => &block[self.base..end],
            Target::Block(block) => &block[self.base..end],
            Target::Fixed(buf) => &buf[..end],
        }
    }

    /// The length of the written data: the furthest position written or seeked to.
    ///
    /// ```
    /// # use syrp_io::{MemoryOutputStream, OutputStream};
    /// let mut s = MemoryOutputStream::new();
    /// s.write_i32_le(0)?;
    /// s.set_position(0)?;
    /// assert_eq!(s.len(), 4);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    #[doc(alias = "getDataSize")]
    pub fn len(&self) -> usize {
        self.size
    }

    /// Whether nothing has been written.
    ///
    /// ```
    /// assert!(syrp_io::MemoryOutputStream::new().is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// Discards the written data and moves back to position 0. A block passed to
    /// [`MemoryOutputStream::appending_to`] goes back to its original size.
    ///
    /// ```
    /// # use syrp_io::{MemoryOutputStream, OutputStream};
    /// let mut s = MemoryOutputStream::new();
    /// s.write_u8(1)?;
    /// s.reset();
    /// assert_eq!((s.len(), s.position()), (0, 0));
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    pub fn reset(&mut self) {
        self.position = 0;
        self.size = 0;
        let base = self.base;
        if let Some(block) = self.block_mut() {
            block.vec_mut().truncate(base);
        }
    }

    /// Makes room for at least `bytes` bytes of data without changing what has been written.
    /// Fails with `SYRP-IO-0005` when the memory cannot be reserved, or with `SYRP-IO-0002`
    /// when a fixed slice is shorter than `bytes`.
    ///
    /// ```
    /// # use syrp_io::MemoryOutputStream;
    /// let mut s = MemoryOutputStream::new();
    /// s.preallocate(1024)?;
    /// assert!(s.is_empty());
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    pub fn preallocate(&mut self, bytes: usize) -> Result<(), IoError> {
        let wanted = self.base.saturating_add(bytes);
        match &mut self.target {
            Target::Fixed(buf) if bytes > buf.len() => Err(IoError::new(IoErrorKind::BufferFull)),
            Target::Fixed(_) => Ok(()),
            _ => self.block_mut().map_or(Ok(()), |block| {
                let vec = block.vec_mut();
                vec.try_reserve_exact(wanted.saturating_sub(vec.len()))
                    .map_err(|_| IoError::new(IoErrorKind::OutOfMemory))
            }),
        }
    }

    /// The written data as UTF-8 text; invalid sequences become U+FFFD.
    ///
    /// ```
    /// # use syrp_io::{MemoryOutputStream, OutputStream};
    /// let mut s = MemoryOutputStream::new();
    /// s.write_text("hé", false, false, None)?;
    /// assert_eq!(s.to_utf8_string(), "hé");
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    #[doc(alias = "toUTF8")]
    pub fn to_utf8_string(&self) -> String {
        String::from_utf8_lossy(self.data()).into_owned()
    }

    /// The written data as a [`MemoryBlock`]. A stream from [`MemoryOutputStream::new`] hands
    /// over its block without copying; the other kinds copy their data.
    ///
    /// ```
    /// # use syrp_io::{MemoryOutputStream, OutputStream};
    /// let mut s = MemoryOutputStream::new();
    /// s.write_u8(5)?;
    /// assert_eq!(&s.into_memory_block()[..], &[5]);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    pub fn into_memory_block(self) -> MemoryBlock {
        match self.target {
            Target::Owned(block) => block,
            _ => MemoryBlock::from_slice(self.data()),
        }
    }

    /// The growable block behind the stream, if it has one.
    fn block_mut(&mut self) -> Option<&mut MemoryBlock> {
        match &mut self.target {
            Target::Owned(block) => Some(block),
            Target::Block(block) => Some(block),
            Target::Fixed(_) => None,
        }
    }

    /// Extends the data to `end` bytes, zero-filling, when it is shorter.
    fn grow_to(&mut self, end: usize) -> Result<(), IoError> {
        if end <= self.size {
            return Ok(());
        }
        let total = self
            .base
            .checked_add(end)
            .ok_or(IoError::new(IoErrorKind::OutOfMemory))?;
        let size = self.size;
        if let Target::Fixed(buf) = &mut self.target {
            buf.get_mut(size..end)
                .ok_or(IoError::new(IoErrorKind::BufferFull))?
                .fill(0);
        } else if let Some(block) = self.block_mut() {
            reserve(block, total)?;
            block.vec_mut().resize(total, 0);
        }
        self.size = end;
        Ok(())
    }
}

/// Reserves room for `total` bytes in `block`, reporting `SYRP-IO-0005` on failure. Growth is
/// amortised (`try_reserve`), so a run of small writes stays linear.
fn reserve(block: &mut MemoryBlock, total: usize) -> Result<(), IoError> {
    let vec = block.vec_mut();
    vec.try_reserve(total.saturating_sub(vec.len()))
        .map_err(|_| IoError::new(IoErrorKind::OutOfMemory))
}

impl Write for MemoryOutputStream<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let end = self
            .position
            .checked_add(buf.len())
            .ok_or(IoError::new(IoErrorKind::OutOfMemory))?;
        self.grow_to(end)?;
        let (start, end) = (self.base + self.position, self.base + end);
        let dest: &mut [u8] = match &mut self.target {
            Target::Owned(block) => block,
            Target::Block(block) => block,
            Target::Fixed(dest) => dest,
        };
        dest[start..end].copy_from_slice(buf);
        self.position += buf.len();
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl OutputStream for MemoryOutputStream<'_> {
    fn position(&self) -> u64 {
        self.position as u64
    }

    fn set_position(&mut self, position: u64) -> Result<(), IoError> {
        let position =
            usize::try_from(position).map_err(|_| IoError::new(IoErrorKind::OutOfMemory))?;
        self.grow_to(position)?;
        self.position = position;
        Ok(())
    }

    fn new_line(&self) -> &str {
        &self.new_line
    }

    fn set_new_line(&mut self, new_line: &str) {
        new_line.clone_into(&mut self.new_line);
    }
}

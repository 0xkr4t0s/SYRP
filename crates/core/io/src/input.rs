//! The [`InputStream`] trait and the [`StdInputStream`] adapter for any `Read + Seek`.

use std::io::{self, Read, Seek, SeekFrom};

use crate::{IoError, IoErrorKind, MemoryBlock};

/// Declares the fixed-width readers, each with its JUCE alias and a doc example.
macro_rules! fixed_readers {
    ($($name:ident: $t:ty, $from:ident, $alias:literal;)*) => {$(
        #[doc = concat!("Reads a `", stringify!($t), "` laid out as [`", stringify!($t), "::",
            stringify!($from), "`] expects. Floats are IEEE 754 and bit-exact.")]
        ///
        #[doc = concat!("```\n# use syrp_io::{InputStream, StdInputStream};\n",
            "let bytes = [7u8; size_of::<", stringify!($t), ">()];\n",
            "let mut s = StdInputStream::new(std::io::Cursor::new(bytes))?;\n",
            "assert_eq!(s.", stringify!($name), "()?, ", stringify!($t), "::",
            stringify!($from), "(bytes));\n# Ok::<(), syrp_io::IoError>(())\n```")]
        #[doc(alias = $alias)]
        fn $name(&mut self) -> Result<$t, IoError> {
            read_array(self).map(<$t>::$from)
        }
    )*};
}

/// Reads exactly `N` bytes, mapping a short read to [`IoErrorKind::EndOfStream`].
fn read_array<const N: usize, S: Read + ?Sized>(s: &mut S) -> Result<[u8; N], IoError> {
    let mut buf = [0u8; N];
    s.read_exact(&mut buf)?;
    Ok(buf)
}

/// Reads one byte, or `None` at the end of the stream.
fn read_opt_byte<S: Read + ?Sized>(s: &mut S) -> Result<Option<u8>, IoError> {
    let mut b = [0u8; 1];
    loop {
        match s.read(&mut b) {
            Ok(0) => return Ok(None),
            Ok(_) => return Ok(Some(b[0])),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.into()),
        }
    }
}

/// Decodes text: UTF-16 when it starts with a byte-order mark, otherwise UTF-8 with a UTF-8 BOM
/// dropped. Invalid sequences become U+FFFD.
fn decode_text(bytes: &[u8]) -> String {
    let utf16 = |rest: &[u8], le: bool| {
        let units = rest.chunks(2).map(|c| match (c, le) {
            ([a, b], true) => u16::from_le_bytes([*a, *b]),
            ([a, b], false) => u16::from_be_bytes([*a, *b]),
            _ => 0xFFFD,
        });
        char::decode_utf16(units)
            .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
            .collect()
    };
    match bytes {
        [0xFF, 0xFE, rest @ ..] => utf16(rest, true),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, false),
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// A readable byte stream with a position: JUCE's `InputStream`.
///
/// Implement [`Read`] plus the three required methods; everything else is provided. Fixed-size
/// reads that hit the end return `SYRP-IO-0001` ([`IoErrorKind::EndOfStream`]) where JUCE
/// returns zero: that is a deliberate deviation. Any `Read + Seek` type gets this trait through
/// [`StdInputStream`]. The trait is dyn-compatible.
///
/// ```
/// use syrp_io::{InputStream, StdInputStream};
/// let mut s = StdInputStream::new(std::io::Cursor::new(b"\x2a\x00hi\nrest".to_vec()))?;
/// assert_eq!(s.read_i16_le()?, 42);
/// assert_eq!(s.read_next_line()?, "hi");
/// assert_eq!(s.read_entire_stream_as_string()?, "rest");
/// assert!(s.is_exhausted());
/// # Ok::<(), syrp_io::IoError>(())
/// ```
#[doc(alias = "juce::InputStream")]
pub trait InputStream: Read {
    /// The total length in bytes, or `None` when it is not known.
    ///
    /// ```
    /// # use syrp_io::{InputStream, StdInputStream};
    /// let mut s = StdInputStream::new(std::io::Cursor::new([0u8; 3]))?;
    /// assert_eq!(s.total_length(), Some(3));
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    #[doc(alias = "getTotalLength")]
    fn total_length(&mut self) -> Option<u64>;

    /// The current read position in bytes from the start.
    ///
    /// ```
    /// # use syrp_io::{InputStream, StdInputStream};
    /// let mut s = StdInputStream::new(std::io::Cursor::new([0u8; 3]))?;
    /// s.read_u8()?;
    /// assert_eq!(s.position(), 1);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    #[doc(alias = "getPosition")]
    fn position(&self) -> u64;

    /// Moves the read position. Fails when the stream cannot seek there.
    ///
    /// ```
    /// # use syrp_io::{InputStream, StdInputStream};
    /// let mut s = StdInputStream::new(std::io::Cursor::new([7u8, 8]))?;
    /// s.set_position(1)?;
    /// assert_eq!(s.read_u8()?, 8);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    fn set_position(&mut self, position: u64) -> Result<(), IoError>;

    /// Whether the position is at or past the end. Streams of unknown length return `false`
    /// unless they override this.
    ///
    /// ```
    /// # use syrp_io::{InputStream, StdInputStream};
    /// let mut s = StdInputStream::new(std::io::Cursor::new([1u8]))?;
    /// assert!(!s.is_exhausted());
    /// s.read_u8()?;
    /// assert!(s.is_exhausted());
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    #[doc(alias = "isExhausted")]
    fn is_exhausted(&mut self) -> bool {
        self.total_length()
            .is_some_and(|len| self.position() >= len)
    }

    /// Reads and discards up to `n` bytes, stopping at the end. Returns how many were skipped.
    ///
    /// ```
    /// # use syrp_io::{InputStream, StdInputStream};
    /// let mut s = StdInputStream::new(std::io::Cursor::new([1u8, 2, 3]))?;
    /// assert_eq!(s.skip_next_bytes(2)?, 2);
    /// assert_eq!(s.skip_next_bytes(9)?, 1);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    #[doc(alias = "skipNextBytes")]
    fn skip_next_bytes(&mut self, n: u64) -> Result<u64, IoError> {
        Ok(io::copy(&mut Read::take(&mut *self, n), &mut io::sink())?)
    }

    /// Reads the rest of the stream as text: UTF-16 (either byte order) when it starts with a
    /// byte-order mark, otherwise UTF-8, dropping a UTF-8 byte-order mark. Invalid sequences
    /// become U+FFFD.
    ///
    /// ```
    /// # use syrp_io::{InputStream, StdInputStream};
    /// let mut s = StdInputStream::new(std::io::Cursor::new([0xFF, 0xFE, b'o', 0, b'k', 0]))?;
    /// assert_eq!(s.read_entire_stream_as_string()?, "ok");
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    #[doc(alias = "readEntireStreamAsString")]
    fn read_entire_stream_as_string(&mut self) -> Result<String, IoError> {
        let mut block = MemoryBlock::new();
        self.read_into_memory_block(&mut block, None)?;
        Ok(decode_text(&block))
    }

    /// Appends up to `max_bytes` (all remaining bytes for `None`) to `block` and returns how
    /// many were read. Space is reserved up front when the length is known; a failed
    /// reservation is `SYRP-IO-0005`.
    ///
    /// ```
    /// # use syrp_io::{InputStream, MemoryBlock, StdInputStream};
    /// let mut s = StdInputStream::new(std::io::Cursor::new([1u8, 2, 3]))?;
    /// let mut block = MemoryBlock::from_slice(&[0]);
    /// assert_eq!(s.read_into_memory_block(&mut block, Some(2))?, 2);
    /// assert_eq!(&block[..], &[0, 1, 2]);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    #[doc(alias = "readIntoMemoryBlock")]
    fn read_into_memory_block(
        &mut self,
        block: &mut MemoryBlock,
        max_bytes: Option<u64>,
    ) -> Result<u64, IoError> {
        let limit = max_bytes.unwrap_or(u64::MAX);
        let pos = self.position();
        let known = self.total_length().map(|len| len.saturating_sub(pos));
        let vec = block.vec_mut();
        if let Some(n) = known.map(|k| k.min(limit)) {
            let n = usize::try_from(n).map_err(|_| IoError::new(IoErrorKind::OutOfMemory))?;
            vec.try_reserve_exact(n)
                .map_err(|_| IoError::new(IoErrorKind::OutOfMemory))?;
        }
        Ok(Read::take(&mut *self, limit).read_to_end(vec)? as u64)
    }

    /// Reads one byte.
    ///
    /// ```
    /// # use syrp_io::{InputStream, StdInputStream};
    /// let mut s = StdInputStream::new(std::io::Cursor::new([9u8]))?;
    /// assert_eq!(s.read_u8()?, 9);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    #[doc(alias = "readByte")]
    fn read_u8(&mut self) -> Result<u8, IoError> {
        read_array(self).map(|[b]| b)
    }

    /// Reads one byte as a boolean: any non-zero byte is `true`.
    ///
    /// ```
    /// # use syrp_io::{InputStream, StdInputStream};
    /// let mut s = StdInputStream::new(std::io::Cursor::new([2u8]))?;
    /// assert!(s.read_bool()?);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    #[doc(alias = "readBool")]
    fn read_bool(&mut self) -> Result<bool, IoError> {
        self.read_u8().map(|b| b != 0)
    }

    fixed_readers! {
        read_i16_le: i16, from_le_bytes, "readShort";
        read_i16_be: i16, from_be_bytes, "readShortBigEndian";
        read_i32_le: i32, from_le_bytes, "readInt";
        read_i32_be: i32, from_be_bytes, "readIntBigEndian";
        read_i64_le: i64, from_le_bytes, "readInt64";
        read_i64_be: i64, from_be_bytes, "readInt64BigEndian";
        read_f32_le: f32, from_le_bytes, "readFloat";
        read_f32_be: f32, from_be_bytes, "readFloatBigEndian";
        read_f64_le: f64, from_le_bytes, "readDouble";
        read_f64_be: f64, from_be_bytes, "readDoubleBigEndian";
    }

    /// Reads an `i32` in Syrp's variable-length layout: one length byte, then the magnitude's
    /// significant bytes in little-endian order (none for zero). The length-byte part follows
    /// the public `writeCompressedInt` documentation; the sign bit (bit 7 of the length byte,
    /// set for a negative value) is Syrp's choice, an open question for WP-0.1-11. So
    /// `[0x01, 0xFF]` is `255` and `[0x81, 0x01]` is `-1`. A length above four, or a magnitude
    /// no `i32` has (above `i32::MAX`, or above 2^31 when negative), is `SYRP-IO-0004`.
    ///
    /// ```
    /// # use syrp_io::{InputStream, StdInputStream};
    /// let mut s = StdInputStream::new(std::io::Cursor::new([0x81u8, 5]))?;
    /// assert_eq!(s.read_compressed_int()?, -5);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    #[doc(alias = "readCompressedInt")]
    fn read_compressed_int(&mut self) -> Result<i32, IoError> {
        let head = self.read_u8()?;
        let len = usize::from(head & 0x7F);
        if len > 4 {
            return Err(IoError::new(IoErrorKind::InvalidData));
        }
        let mut bytes = [0u8; 4];
        self.read_exact(&mut bytes[..len])?;
        let magnitude = u32::from_le_bytes(bytes);
        let value = if head & 0x80 != 0 {
            0i32.checked_sub_unsigned(magnitude)
        } else {
            i32::try_from(magnitude).ok()
        };
        value.ok_or_else(|| IoError::new(IoErrorKind::InvalidData))
    }

    /// Reads UTF-8 text up to a NUL byte or the end; the NUL is consumed, not returned.
    /// Invalid sequences become U+FFFD.
    ///
    /// ```
    /// # use syrp_io::{InputStream, StdInputStream};
    /// let mut s = StdInputStream::new(std::io::Cursor::new(b"ab\0cd".to_vec()))?;
    /// assert_eq!(s.read_string()?, "ab");
    /// assert_eq!(s.read_string()?, "cd");
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    #[doc(alias = "readString")]
    fn read_string(&mut self) -> Result<String, IoError> {
        let mut bytes = Vec::new();
        while let Some(b) = read_opt_byte(self)? {
            if b == 0 {
                break;
            }
            bytes.push(b);
        }
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// Reads UTF-8 text up to a `\n` or the end. The `\n`, and a `\r` just before it, are
    /// consumed and not returned. Invalid sequences become U+FFFD.
    ///
    /// ```
    /// # use syrp_io::{InputStream, StdInputStream};
    /// let mut s = StdInputStream::new(std::io::Cursor::new(b"a\r\nb".to_vec()))?;
    /// assert_eq!(s.read_next_line()?, "a");
    /// assert_eq!(s.read_next_line()?, "b");
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    #[doc(alias = "readNextLine")]
    fn read_next_line(&mut self) -> Result<String, IoError> {
        let mut bytes = Vec::new();
        while let Some(b) = read_opt_byte(self)? {
            if b == b'\n' {
                if bytes.last() == Some(&b'\r') {
                    bytes.pop();
                }
                break;
            }
            bytes.push(b);
        }
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
}

/// Forwards the required methods, and the provided ones a stream may override with a cheaper
/// version (`is_exhausted`, `skip_next_bytes`, `read_into_memory_block`), through a `Box` or
/// `&mut`, so an override is not bypassed behind `Box<dyn InputStream>`.
macro_rules! forward_input_stream {
    ($($ty:ty),*) => {$(
        impl<S: InputStream + ?Sized> InputStream for $ty {
            fn total_length(&mut self) -> Option<u64> {
                (**self).total_length()
            }
            fn position(&self) -> u64 {
                (**self).position()
            }
            fn set_position(&mut self, position: u64) -> Result<(), IoError> {
                (**self).set_position(position)
            }
            fn is_exhausted(&mut self) -> bool {
                (**self).is_exhausted()
            }
            fn skip_next_bytes(&mut self, n: u64) -> Result<u64, IoError> {
                (**self).skip_next_bytes(n)
            }
            fn read_into_memory_block(
                &mut self,
                block: &mut MemoryBlock,
                max_bytes: Option<u64>,
            ) -> Result<u64, IoError> {
                (**self).read_into_memory_block(block, max_bytes)
            }
        }
    )*};
}

forward_input_stream!(Box<S>, &mut S);

/// An [`InputStream`] over any `Read + Seek` value, such as a [`std::fs::File`] or a
/// [`std::io::Cursor`]. It tracks the position itself, so the inner value is only reachable
/// by shared reference or by taking it back with [`StdInputStream::into_inner`].
///
/// ```
/// use syrp_io::{InputStream, StdInputStream};
/// let mut s = StdInputStream::new(std::io::Cursor::new([0u8, 1]))?;
/// assert_eq!(s.read_i16_be()?, 1);
/// # Ok::<(), syrp_io::IoError>(())
/// ```
#[derive(Debug)]
pub struct StdInputStream<R: Read + Seek> {
    inner: R,
    position: u64,
}

impl<R: Read + Seek> StdInputStream<R> {
    /// Wraps `inner`, starting at its current position. Fails when that position cannot be
    /// queried.
    ///
    /// ```
    /// # use syrp_io::{InputStream, StdInputStream};
    /// let mut c = std::io::Cursor::new([1u8, 2]);
    /// c.set_position(1);
    /// assert_eq!(StdInputStream::new(c)?.position(), 1);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    pub fn new(mut inner: R) -> Result<Self, IoError> {
        let position = inner.stream_position()?;
        Ok(Self { inner, position })
    }

    /// The wrapped value.
    ///
    /// ```
    /// # use syrp_io::StdInputStream;
    /// let s = StdInputStream::new(std::io::Cursor::new([1u8]))?;
    /// assert_eq!(s.get_ref().get_ref(), &[1]);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    pub fn get_ref(&self) -> &R {
        &self.inner
    }

    /// Gives the wrapped value back, positioned where this stream was.
    ///
    /// ```
    /// # use syrp_io::{InputStream, StdInputStream};
    /// let mut s = StdInputStream::new(std::io::Cursor::new([1u8, 2]))?;
    /// s.read_u8()?;
    /// assert_eq!(s.into_inner().position(), 1);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    pub fn into_inner(self) -> R {
        self.inner
    }
}

impl<R: Read + Seek> Read for StdInputStream<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.position += n as u64;
        Ok(n)
    }
}

impl<R: Read + Seek> InputStream for StdInputStream<R> {
    fn total_length(&mut self) -> Option<u64> {
        let len = self.inner.seek(SeekFrom::End(0)).ok();
        // Restore the position; if that fails the stream is unusable, so report the length as
        // unknown and keep the tracked position, which the next set_position re-establishes.
        match self.inner.seek(SeekFrom::Start(self.position)) {
            Ok(_) => len,
            Err(_) => None,
        }
    }

    fn position(&self) -> u64 {
        self.position
    }

    fn set_position(&mut self, position: u64) -> Result<(), IoError> {
        self.position = self.inner.seek(SeekFrom::Start(position))?;
        Ok(())
    }
}

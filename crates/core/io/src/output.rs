//! The [`OutputStream`] trait and the [`StdOutputStream`] adapter for any `Write + Seek`.

use std::io::{self, Read, Seek, SeekFrom, Write};

use crate::{InputStream, IoError};

/// Declares the fixed-width writers, each with its JUCE alias and a doc example.
macro_rules! fixed_writers {
    ($($name:ident: $t:ty, $to:ident, $alias:literal;)*) => {$(
        #[doc = concat!("Writes `value` laid out as [`", stringify!($t), "::", stringify!($to),
            "`] gives it. Floats are IEEE 754 and bit-exact, NaN payloads included.")]
        ///
        #[doc = concat!("```\n# use syrp_io::{OutputStream, StdOutputStream};\n",
            "let mut s = StdOutputStream::new(std::io::Cursor::new(Vec::new()))?;\n",
            "s.", stringify!($name), "(5 as ", stringify!($t), ")?;\n",
            "assert_eq!(s.into_inner().into_inner(), (5 as ", stringify!($t), ").",
            stringify!($to), "());\n# Ok::<(), syrp_io::IoError>(())\n```")]
        #[doc(alias = $alias)]
        fn $name(&mut self, value: $t) -> Result<(), IoError> {
            Ok(self.write_all(&value.$to())?)
        }
    )*};
}

/// The new-line string a stream starts with: JUCE's default, `"\r\n"`.
///
/// ```
/// assert_eq!(syrp_io::DEFAULT_NEW_LINE, "\r\n");
/// ```
pub const DEFAULT_NEW_LINE: &str = "\r\n";

/// A writable byte stream with a position: JUCE's `OutputStream`.
///
/// Implement [`Write`] plus the four required methods; the typed writers are provided and
/// match the [`InputStream`] readers byte for byte. Any `Write + Seek` type gets this trait
/// through [`StdOutputStream`]. The trait is dyn-compatible.
///
/// ```
/// use syrp_io::{OutputStream, StdOutputStream};
/// let mut s = StdOutputStream::new(std::io::Cursor::new(Vec::new()))?;
/// s.write_i16_be(1)?;
/// s.write_string("hi")?;
/// assert_eq!(s.into_inner().into_inner(), b"\x00\x01hi\0");
/// # Ok::<(), syrp_io::IoError>(())
/// ```
#[doc(alias = "juce::OutputStream")]
pub trait OutputStream: Write {
    /// The current write position in bytes from the start.
    ///
    /// ```
    /// # use syrp_io::{OutputStream, StdOutputStream};
    /// let mut s = StdOutputStream::new(std::io::Cursor::new(Vec::new()))?;
    /// s.write_i32_le(0)?;
    /// assert_eq!(s.position(), 4);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    #[doc(alias = "getPosition")]
    fn position(&self) -> u64;

    /// Moves the write position. Fails when the stream cannot seek there.
    ///
    /// ```
    /// # use syrp_io::{OutputStream, StdOutputStream};
    /// let mut s = StdOutputStream::new(std::io::Cursor::new(vec![0u8; 2]))?;
    /// s.set_position(1)?;
    /// s.write_u8(7)?;
    /// assert_eq!(s.into_inner().into_inner(), [0, 7]);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    fn set_position(&mut self, position: u64) -> Result<(), IoError>;

    /// The string [`OutputStream::write_new_line`] writes.
    ///
    /// ```
    /// # use syrp_io::{OutputStream, StdOutputStream};
    /// let s = StdOutputStream::new(std::io::Cursor::new(Vec::new()))?;
    /// assert_eq!(s.new_line(), "\r\n");
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    #[doc(alias = "getNewLineString")]
    fn new_line(&self) -> &str;

    /// Changes the string [`OutputStream::write_new_line`] writes.
    ///
    /// ```
    /// # use syrp_io::{OutputStream, StdOutputStream};
    /// let mut s = StdOutputStream::new(std::io::Cursor::new(Vec::new()))?;
    /// s.set_new_line("\n");
    /// assert_eq!(s.new_line(), "\n");
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    #[doc(alias = "setNewLineString")]
    fn set_new_line(&mut self, new_line: &str);

    /// Writes this stream's new-line string.
    ///
    /// ```
    /// # use syrp_io::{OutputStream, StdOutputStream};
    /// let mut s = StdOutputStream::new(std::io::Cursor::new(Vec::new()))?;
    /// s.write_new_line()?;
    /// assert_eq!(s.into_inner().into_inner(), b"\r\n");
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    fn write_new_line(&mut self) -> Result<(), IoError> {
        let nl = self.new_line().to_owned();
        Ok(self.write_all(nl.as_bytes())?)
    }

    /// Writes one byte.
    ///
    /// ```
    /// # use syrp_io::{OutputStream, StdOutputStream};
    /// let mut s = StdOutputStream::new(std::io::Cursor::new(Vec::new()))?;
    /// s.write_u8(9)?;
    /// assert_eq!(s.into_inner().into_inner(), [9]);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    #[doc(alias = "writeByte")]
    fn write_u8(&mut self, value: u8) -> Result<(), IoError> {
        Ok(self.write_all(&[value])?)
    }

    /// Writes a boolean as one byte, `1` or `0`.
    ///
    /// ```
    /// # use syrp_io::{OutputStream, StdOutputStream};
    /// let mut s = StdOutputStream::new(std::io::Cursor::new(Vec::new()))?;
    /// s.write_bool(true)?;
    /// assert_eq!(s.into_inner().into_inner(), [1]);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    #[doc(alias = "writeBool")]
    fn write_bool(&mut self, value: bool) -> Result<(), IoError> {
        self.write_u8(u8::from(value))
    }

    fixed_writers! {
        write_i16_le: i16, to_le_bytes, "writeShort";
        write_i16_be: i16, to_be_bytes, "writeShortBigEndian";
        write_i32_le: i32, to_le_bytes, "writeInt";
        write_i32_be: i32, to_be_bytes, "writeIntBigEndian";
        write_i64_le: i64, to_le_bytes, "writeInt64";
        write_i64_be: i64, to_be_bytes, "writeInt64BigEndian";
        write_f32_le: f32, to_le_bytes, "writeFloat";
        write_f32_be: f32, to_be_bytes, "writeFloatBigEndian";
        write_f64_le: f64, to_le_bytes, "writeDouble";
        write_f64_be: f64, to_be_bytes, "writeDoubleBigEndian";
    }

    /// Writes an `i32` in Syrp's variable-length layout: one length byte, then the magnitude's
    /// significant bytes in little-endian order (none for zero). The length byte follows the
    /// public `writeCompressedInt` documentation (the number of significant bytes, 0 to 4); the
    /// sign bit (bit 7 of the length byte, set for a negative value) is Syrp's choice, an open
    /// question for WP-0.1-11. So `0` is `[0x00]`, `255` is `[0x01, 0xFF]` and `-1` is
    /// `[0x81, 0x01]`. The length is always the shortest one, and every output (`i32::MIN`
    /// included) is accepted by [`InputStream::read_compressed_int`].
    ///
    /// ```
    /// # use syrp_io::{OutputStream, StdOutputStream};
    /// let mut s = StdOutputStream::new(std::io::Cursor::new(Vec::new()))?;
    /// s.write_compressed_int(300)?;
    /// assert_eq!(s.into_inner().into_inner(), [0x02, 0x2C, 0x01]);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    #[doc(alias = "writeCompressedInt")]
    fn write_compressed_int(&mut self, value: i32) -> Result<(), IoError> {
        let magnitude = value.unsigned_abs();
        let len = (4 - magnitude.leading_zeros() / 8) as usize;
        let mut buf = [0u8; 5];
        buf[0] = len as u8 | if value < 0 { 0x80 } else { 0 };
        buf[1..=len].copy_from_slice(&magnitude.to_le_bytes()[..len]);
        Ok(self.write_all(&buf[..=len])?)
    }

    /// Writes `text` as UTF-8 followed by a NUL byte.
    ///
    /// ```
    /// # use syrp_io::{OutputStream, StdOutputStream};
    /// let mut s = StdOutputStream::new(std::io::Cursor::new(Vec::new()))?;
    /// s.write_string("ok")?;
    /// assert_eq!(s.into_inner().into_inner(), b"ok\0");
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    #[doc(alias = "writeString")]
    fn write_string(&mut self, text: &str) -> Result<(), IoError> {
        self.write_all(text.as_bytes())?;
        self.write_u8(0)
    }

    /// Writes `text` with no terminator, as UTF-8 or as UTF-16 little-endian. With `write_bom`,
    /// UTF-16 output starts with the byte-order mark `FF FE`; UTF-8 output never gets one.
    /// With `line_ending`, every `\r\n` and every lone `\n` in `text` is written as that
    /// string instead; with `None` the text is written unchanged.
    ///
    /// ```
    /// # use syrp_io::{OutputStream, StdOutputStream};
    /// let mut s = StdOutputStream::new(std::io::Cursor::new(Vec::new()))?;
    /// s.write_text("a\nb", false, false, Some("\r\n"))?;
    /// s.write_text("c", true, true, None)?;
    /// assert_eq!(s.into_inner().into_inner(), b"a\r\nb\xFF\xFEc\0");
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    #[doc(alias = "writeText")]
    fn write_text(
        &mut self,
        text: &str,
        as_utf16: bool,
        write_bom: bool,
        line_ending: Option<&str>,
    ) -> Result<(), IoError> {
        let converted;
        let text = match line_ending {
            Some(ending) => {
                converted = text.replace("\r\n", "\n").replace('\n', ending);
                converted.as_str()
            }
            None => text,
        };
        if !as_utf16 {
            return Ok(self.write_all(text.as_bytes())?);
        }
        if write_bom {
            self.write_all(&[0xFF, 0xFE])?;
        }
        // Encode through a small stack buffer: no allocation, and nothing is left buffered to
        // be written after an error has been returned.
        let mut chunk = [0u8; 512];
        let mut used = 0;
        for unit in text.encode_utf16() {
            chunk[used..used + 2].copy_from_slice(&unit.to_le_bytes());
            used += 2;
            if used == chunk.len() {
                self.write_all(&chunk)?;
                used = 0;
            }
        }
        Ok(self.write_all(&chunk[..used])?)
    }

    /// Writes `byte` `count` times.
    ///
    /// ```
    /// # use syrp_io::{OutputStream, StdOutputStream};
    /// let mut s = StdOutputStream::new(std::io::Cursor::new(Vec::new()))?;
    /// s.write_repeated_byte(7, 3)?;
    /// assert_eq!(s.into_inner().into_inner(), [7, 7, 7]);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    #[doc(alias = "writeRepeatedByte")]
    fn write_repeated_byte(&mut self, byte: u8, count: u64) -> Result<(), IoError> {
        io::copy(&mut io::repeat(byte).take(count), self)?;
        Ok(())
    }

    /// Copies up to `max_bytes` (everything for `None`) from `source` to this stream and returns
    /// the number of bytes copied. A read or write failure is an error whose
    /// [`IoError::bytes_copied`] (also in its message) says how much was copied before it. After
    /// a write failure that count is a lower bound: part of the failing chunk may also have
    /// been written.
    ///
    /// ```
    /// # use syrp_io::{OutputStream, StdInputStream, StdOutputStream};
    /// let mut src = StdInputStream::new(std::io::Cursor::new([1u8, 2, 3]))?;
    /// let mut s = StdOutputStream::new(std::io::Cursor::new(Vec::new()))?;
    /// assert_eq!(s.write_from_input_stream(&mut src, Some(2))?, 2);
    /// assert_eq!(s.into_inner().into_inner(), [1, 2]);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    #[doc(alias = "writeFromInputStream")]
    fn write_from_input_stream(
        &mut self,
        source: &mut dyn InputStream,
        max_bytes: Option<u64>,
    ) -> Result<u64, IoError> {
        let mut remaining = max_bytes.unwrap_or(u64::MAX);
        let mut copied = 0u64;
        let mut buf = [0u8; 8192];
        while remaining > 0 {
            let want = buf
                .len()
                .min(usize::try_from(remaining).unwrap_or(usize::MAX));
            let n = match source.read(&mut buf[..want]) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(IoError::from(e).with_copied(copied)),
            };
            self.write_all(&buf[..n])
                .map_err(|e| IoError::from(e).with_copied(copied))?;
            copied += n as u64;
            remaining -= n as u64;
        }
        Ok(copied)
    }
}

/// Forwards the required methods, and the provided ones a stream may override with a cheaper
/// version (`write_repeated_byte`, `write_from_input_stream`), through a `Box` or `&mut`, so an
/// override is not bypassed behind `Box<dyn OutputStream>`.
macro_rules! forward_output_stream {
    ($($ty:ty),*) => {$(
        impl<S: OutputStream + ?Sized> OutputStream for $ty {
            fn position(&self) -> u64 {
                (**self).position()
            }
            fn set_position(&mut self, position: u64) -> Result<(), IoError> {
                (**self).set_position(position)
            }
            fn new_line(&self) -> &str {
                (**self).new_line()
            }
            fn set_new_line(&mut self, new_line: &str) {
                (**self).set_new_line(new_line)
            }
            fn write_repeated_byte(&mut self, byte: u8, count: u64) -> Result<(), IoError> {
                (**self).write_repeated_byte(byte, count)
            }
            fn write_from_input_stream(
                &mut self,
                source: &mut dyn InputStream,
                max_bytes: Option<u64>,
            ) -> Result<u64, IoError> {
                (**self).write_from_input_stream(source, max_bytes)
            }
        }
    )*};
}

forward_output_stream!(Box<S>, &mut S);

/// An [`OutputStream`] over any `Write + Seek` value, such as a [`std::fs::File`] or a
/// [`std::io::Cursor`]. It tracks the position itself, so the inner value is only reachable
/// by shared reference or by taking it back with [`StdOutputStream::into_inner`]. Its new-line
/// string starts as [`DEFAULT_NEW_LINE`].
///
/// ```
/// use syrp_io::{OutputStream, StdOutputStream};
/// let mut s = StdOutputStream::new(std::io::Cursor::new(Vec::new()))?;
/// s.write_compressed_int(-1)?;
/// assert_eq!(s.into_inner().into_inner(), [0x81, 0x01]);
/// # Ok::<(), syrp_io::IoError>(())
/// ```
#[derive(Debug)]
pub struct StdOutputStream<W: Write + Seek> {
    inner: W,
    position: u64,
    new_line: String,
}

impl<W: Write + Seek> StdOutputStream<W> {
    /// Wraps `inner`, starting at its current position. Fails when that position cannot be
    /// queried.
    ///
    /// ```
    /// # use syrp_io::{OutputStream, StdOutputStream};
    /// let mut c = std::io::Cursor::new(vec![0u8; 4]);
    /// c.set_position(4);
    /// assert_eq!(StdOutputStream::new(c)?.position(), 4);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    pub fn new(mut inner: W) -> Result<Self, IoError> {
        let position = inner.stream_position()?;
        Ok(Self {
            inner,
            position,
            new_line: DEFAULT_NEW_LINE.to_owned(),
        })
    }

    /// The wrapped value.
    ///
    /// ```
    /// # use syrp_io::StdOutputStream;
    /// let s = StdOutputStream::new(std::io::Cursor::new(Vec::<u8>::new()))?;
    /// assert!(s.get_ref().get_ref().is_empty());
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    pub fn get_ref(&self) -> &W {
        &self.inner
    }

    /// Gives the wrapped value back, positioned where this stream was. Flush first if `W`
    /// buffers.
    ///
    /// ```
    /// # use syrp_io::{OutputStream, StdOutputStream};
    /// let mut s = StdOutputStream::new(std::io::Cursor::new(Vec::new()))?;
    /// s.write_u8(1)?;
    /// assert_eq!(s.into_inner().into_inner(), [1]);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    pub fn into_inner(self) -> W {
        self.inner
    }
}

impl<W: Write + Seek> Write for StdOutputStream<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.position += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<W: Write + Seek> OutputStream for StdOutputStream<W> {
    fn position(&self) -> u64 {
        self.position
    }

    fn set_position(&mut self, position: u64) -> Result<(), IoError> {
        self.position = self.inner.seek(SeekFrom::Start(position))?;
        Ok(())
    }

    fn new_line(&self) -> &str {
        &self.new_line
    }

    fn set_new_line(&mut self, new_line: &str) {
        new_line.clone_into(&mut self.new_line);
    }
}

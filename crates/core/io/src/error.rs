//! [`IoError`], the error type of every stream and memory-block operation.

use std::fmt;
use std::io;

use syrp_error::{Diagnostic, ErrorCode};

/// What went wrong in an [`IoError`].
///
/// ```
/// use syrp_io::{InputStream, IoErrorKind, StdInputStream};
/// let mut s = StdInputStream::new(std::io::Cursor::new([1u8]))?;
/// assert_eq!(s.read_i32_le().unwrap_err().kind(), IoErrorKind::EndOfStream);
/// # Ok::<(), syrp_io::IoError>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum IoErrorKind {
    /// A fixed-size read reached the end of the stream (`SYRP-IO-0001`).
    EndOfStream,
    /// A write did not fit in a fixed output buffer, so nothing was written (`SYRP-IO-0002`).
    BufferFull,
    /// The underlying reader, writer or seek failed (`SYRP-IO-0003`).
    Io,
    /// The bytes do not follow the expected format (`SYRP-IO-0004`).
    InvalidData,
    /// A buffer could not be allocated (`SYRP-IO-0005`).
    OutOfMemory,
}

/// The error type of this crate: an [`IoErrorKind`], the underlying [`std::io::Error`] when
/// there is one (returned by [`std::error::Error::source`]), and, for a failed copy, the number
/// of bytes copied before the failure.
///
/// It implements [`Diagnostic`] with `SYRP-IO-*` codes and converts both ways with
/// [`std::io::Error`], so the stream traits compose with `std` code. Converting an `IoError` to
/// `std::io::Error` and back gives the original `IoError`.
///
/// ```
/// use syrp_error::Diagnostic;
/// use syrp_io::{IoError, IoErrorKind};
/// let err = IoError::from(std::io::Error::other("disk gone"));
/// assert_eq!(err.kind(), IoErrorKind::Io);
/// assert_eq!(err.code().as_str(), "SYRP-IO-0003");
/// let back = IoError::from(std::io::Error::from(err));
/// assert_eq!(back.kind(), IoErrorKind::Io);
/// ```
#[derive(Debug)]
pub struct IoError {
    kind: IoErrorKind,
    source: Option<io::Error>,
    copied: Option<u64>,
}

impl IoError {
    /// An error of `kind` with no underlying cause.
    ///
    /// ```
    /// use syrp_io::{IoError, IoErrorKind};
    /// assert_eq!(IoError::new(IoErrorKind::InvalidData).kind(), IoErrorKind::InvalidData);
    /// ```
    pub const fn new(kind: IoErrorKind) -> Self {
        Self {
            kind,
            source: None,
            copied: None,
        }
    }

    /// What went wrong.
    ///
    /// ```
    /// use syrp_io::{IoError, IoErrorKind};
    /// assert_eq!(IoError::new(IoErrorKind::OutOfMemory).kind(), IoErrorKind::OutOfMemory);
    /// ```
    pub const fn kind(&self) -> IoErrorKind {
        self.kind
    }

    /// For a failed [`OutputStream::write_from_input_stream`](crate::OutputStream::write_from_input_stream),
    /// the number of bytes copied before the failure; `None` for every other error.
    ///
    /// ```
    /// use syrp_io::{IoError, IoErrorKind};
    /// assert_eq!(IoError::new(IoErrorKind::Io).bytes_copied(), None);
    /// ```
    pub const fn bytes_copied(&self) -> Option<u64> {
        self.copied
    }

    /// Records how many bytes a copy moved before this error.
    pub(crate) fn with_copied(mut self, copied: u64) -> Self {
        self.copied = Some(copied);
        self
    }
}

impl From<io::Error> for IoError {
    fn from(err: io::Error) -> Self {
        if err.get_ref().is_some_and(|inner| inner.is::<IoError>()) {
            if let Some(Ok(inner)) = err.into_inner().map(|b| b.downcast::<IoError>()) {
                return *inner;
            }
            return Self::new(IoErrorKind::Io);
        }
        let kind = match err.kind() {
            io::ErrorKind::UnexpectedEof => IoErrorKind::EndOfStream,
            io::ErrorKind::OutOfMemory => IoErrorKind::OutOfMemory,
            _ => IoErrorKind::Io,
        };
        Self {
            kind,
            source: Some(err),
            copied: None,
        }
    }
}

impl From<IoError> for io::Error {
    fn from(err: IoError) -> Self {
        let kind = match (&err.kind, &err.source) {
            (IoErrorKind::EndOfStream, _) => io::ErrorKind::UnexpectedEof,
            (IoErrorKind::BufferFull, _) => io::ErrorKind::StorageFull,
            (IoErrorKind::InvalidData, _) => io::ErrorKind::InvalidData,
            (IoErrorKind::OutOfMemory, _) => io::ErrorKind::OutOfMemory,
            (IoErrorKind::Io, Some(source)) => source.kind(),
            (IoErrorKind::Io, None) => io::ErrorKind::Other,
        };
        io::Error::new(kind, err)
    }
}

impl fmt::Display for IoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            kind,
            source,
            copied,
        } = self;
        f.write_str(match kind {
            IoErrorKind::EndOfStream => "the stream ended inside a fixed-size read",
            IoErrorKind::BufferFull => "the fixed output buffer is full",
            IoErrorKind::Io => "the underlying I/O operation failed",
            IoErrorKind::InvalidData => "malformed data in the stream",
            IoErrorKind::OutOfMemory => "out of memory for a stream or memory block",
        })?;
        if let Some(n) = copied {
            write!(f, " after copying {n} bytes")?;
        }
        match source {
            Some(source) => write!(f, ": {source}"),
            None => Ok(()),
        }
    }
}

impl std::error::Error for IoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|e| e as &(dyn std::error::Error + 'static))
    }
}

impl Diagnostic for IoError {
    fn code(&self) -> ErrorCode {
        ErrorCode::new(match self.kind {
            IoErrorKind::EndOfStream => "SYRP-IO-0001",
            IoErrorKind::BufferFull => "SYRP-IO-0002",
            IoErrorKind::Io => "SYRP-IO-0003",
            IoErrorKind::InvalidData => "SYRP-IO-0004",
            IoErrorKind::OutOfMemory => "SYRP-IO-0005",
        })
    }
}

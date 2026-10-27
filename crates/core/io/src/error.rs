//! [`IoError`], the error type of every stream and memory-block operation.

use std::fmt;
use std::io;

use syrp_error::{Diagnostic, ErrorCode};

/// What went wrong in an [`IoError`].
///
/// ```
/// use syrp_io::{IoErrorKind, MemoryBlock};
/// let err = MemoryBlock::with_size(usize::MAX, true).unwrap_err();
/// assert_eq!(err.kind(), IoErrorKind::OutOfMemory);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum IoErrorKind {
    /// A fixed-size read reached the end of the stream (`SYRP-IO-0001`).
    EndOfStream,
    /// The underlying reader, writer or seek failed (`SYRP-IO-0003`).
    Io,
    /// The bytes do not follow the expected format (`SYRP-IO-0004`).
    InvalidData,
    /// A buffer could not be allocated (`SYRP-IO-0005`).
    OutOfMemory,
}

/// The error type of this crate: an [`IoErrorKind`] plus the underlying [`std::io::Error`] when
/// there is one (returned by [`std::error::Error::source`]).
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
}

impl IoError {
    /// An error of `kind` with no underlying cause.
    ///
    /// ```
    /// use syrp_io::{IoError, IoErrorKind};
    /// assert_eq!(IoError::new(IoErrorKind::InvalidData).kind(), IoErrorKind::InvalidData);
    /// ```
    pub const fn new(kind: IoErrorKind) -> Self {
        Self { kind, source: None }
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
        }
    }
}

impl From<IoError> for io::Error {
    fn from(err: IoError) -> Self {
        let kind = match (&err.kind, &err.source) {
            (IoErrorKind::EndOfStream, _) => io::ErrorKind::UnexpectedEof,
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
        let Self { kind, source } = self;
        f.write_str(match kind {
            IoErrorKind::EndOfStream => "the stream ended inside a fixed-size read",
            IoErrorKind::Io => "the underlying I/O operation failed",
            IoErrorKind::InvalidData => "malformed data in the stream",
            IoErrorKind::OutOfMemory => "out of memory for a stream or memory block",
        })?;
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
            IoErrorKind::Io => "SYRP-IO-0003",
            IoErrorKind::InvalidData => "SYRP-IO-0004",
            IoErrorKind::OutOfMemory => "SYRP-IO-0005",
        })
    }
}

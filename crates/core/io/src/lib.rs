//! Syrp's stream layer (master plan §5.2, L2 Core): JUCE's `InputStream`, `OutputStream` and
//! `MemoryBlock`.
//!
//! - [`InputStream`] and [`OutputStream`]: positioned byte streams built on [`std::io::Read`]
//!   and [`std::io::Write`], with typed little- and big-endian readers and writers, compressed
//!   integers, NUL-terminated strings and text.
//! - [`StdInputStream`] and [`StdOutputStream`]: adapters that give any `Read + Seek` or
//!   `Write + Seek` value those traits.
//! - [`MemoryInputStream`] and [`MemoryOutputStream`]: streams over bytes in memory, borrowed
//!   or owned, growing or fixed-size.
//! - [`BufferedInputStream`] and [`SubregionStream`]: a read-ahead buffer over another stream,
//!   and a window onto part of one.
//! - [`InputSource`]: something that opens fresh input streams on demand.
//! - [`MemoryBlock`]: an owned, growable byte block.
//! - [`IoError`]: the error type, with `SYRP-IO-*` codes, convertible to and from
//!   [`std::io::Error`].
//!
//! This is cold-path code: everything may allocate and none of it is real-time safe.
//!
//! ```
//! use syrp_io::{InputStream, OutputStream, StdInputStream, StdOutputStream};
//! let mut out = StdOutputStream::new(std::io::Cursor::new(Vec::new()))?;
//! out.write_f64_be(0.5)?;
//! out.write_compressed_int(-70000)?;
//! let mut inp = StdInputStream::new(std::io::Cursor::new(out.into_inner().into_inner()))?;
//! assert_eq!(inp.read_f64_be()?, 0.5);
//! assert_eq!(inp.read_compressed_int()?, -70000);
//! # Ok::<(), syrp_io::IoError>(())
//! ```

mod buffered;
mod error;
mod input;
mod memory_block;
mod memory_stream;
mod output;
mod source;
mod subregion;

pub use buffered::BufferedInputStream;
pub use error::{IoError, IoErrorKind};
pub use input::{InputStream, StdInputStream};
pub use memory_block::MemoryBlock;
pub use memory_stream::{MemoryInputStream, MemoryOutputStream};
pub use output::{DEFAULT_NEW_LINE, OutputStream, StdOutputStream};
pub use source::InputSource;
pub use subregion::SubregionStream;

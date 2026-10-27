//! Syrp's stream layer (master plan §5.2, L2 Core).
//!
//! - [`InputStream`]: JUCE's positioned byte stream, built on [`std::io::Read`], with typed
//!   little- and big-endian readers, compressed integers, NUL-terminated strings and text.
//! - [`StdInputStream`]: an adapter that gives any `Read + Seek` value that trait.
//! - [`MemoryBlock`]: an owned, growable byte block (JUCE's `MemoryBlock`).
//! - [`IoError`]: the error type, with `SYRP-IO-*` codes, convertible to and from
//!   [`std::io::Error`].
//!
//! `OutputStream` and the memory, buffered and subregion streams follow in later increments of
//! WP-1.1-03. This is cold-path code: everything may allocate and none of it is real-time safe.
//!
//! ```
//! use syrp_io::{InputStream, StdInputStream};
//! let mut s = StdInputStream::new(std::io::Cursor::new(b"\x00\x00\x00\x2aline\n".to_vec()))?;
//! assert_eq!(s.read_i32_be()?, 42);
//! assert_eq!(s.read_next_line()?, "line");
//! # Ok::<(), syrp_io::IoError>(())
//! ```

mod error;
mod input;
mod memory_block;

pub use error::{IoError, IoErrorKind};
pub use input::{InputStream, StdInputStream};
pub use memory_block::MemoryBlock;

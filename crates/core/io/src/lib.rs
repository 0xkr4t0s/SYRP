//! Syrp's stream layer (master plan §5.2, L2 Core).
//!
//! This first increment holds the pieces every stream builds on:
//!
//! - [`MemoryBlock`]: an owned, growable byte block (JUCE's `MemoryBlock`).
//! - [`IoError`]: the error type, with `SYRP-IO-*` codes, convertible to and from
//!   [`std::io::Error`].
//!
//! The `InputStream` and `OutputStream` traits and the memory, buffered and subregion streams
//! follow in later increments of WP-1.1-03. This is cold-path code: everything may allocate and
//! none of it is real-time safe.
//!
//! ```
//! use syrp_io::MemoryBlock;
//! let mut block = MemoryBlock::from_slice(b"stream");
//! block.set_bit_range(0, 1, 0);
//! assert_eq!(block.to_string_lossy(), "rtream");
//! ```

mod error;
mod memory_block;

pub use error::{IoError, IoErrorKind};
pub use memory_block::MemoryBlock;

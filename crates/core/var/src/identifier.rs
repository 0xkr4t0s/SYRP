//! Process-wide interned names.

use std::collections::HashMap;
use std::fmt;
use std::sync::{LazyLock, Mutex, OnceLock};

/// Number of slots in the first chunk of the name table; chunk `c` holds `FIRST_CHUNK << c`.
const FIRST_CHUNK: usize = 32;
/// Enough chunks to address every `u32` index.
const CHUNKS: usize = 28;

/// Interned text by index. Chunks are allocated on demand and never move or shrink, so a
/// reader only needs two acquire loads (no lock, no allocation) to find a name.
static NAMES: [OnceLock<Box<[OnceLock<&'static str>]>>; CHUNKS] =
    [const { OnceLock::new() }; CHUNKS];

/// Text to index, used only when creating identifiers. Index 0 is reserved for the empty name.
static INDEX: LazyLock<Mutex<HashMap<&'static str, u32>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Splits an index into its chunk and the offset inside that chunk.
fn locate(index: u32) -> (usize, usize) {
    let index = index as usize;
    let block = index / FIRST_CHUNK + 1;
    let chunk = (usize::BITS - 1 - block.leading_zeros()) as usize;
    let start = FIRST_CHUNK * ((1 << chunk) - 1);
    (chunk, index - start)
}

/// A process-wide interned name, used for property names and node types.
///
/// An `Identifier` is a 4-byte handle (`size_of::<Identifier>() == 4`) that is `Copy`,
/// `Send` and `Sync`. Two identifiers made from equal strings are equal, on any thread, and
/// equality and hashing only look at the handle, never at the text.
///
/// - **Ordering.** `Ord` follows interning order (the order in which names were first seen by
///   the process), which is cheap but not alphabetical. Use [`Identifier::cmp_text`] for
///   lexical order.
/// - **Memory.** Interned text is never freed: it lives until the process exits. Do not intern
///   unbounded user input.
/// - **Threads.** [`Identifier::new`] may allocate and takes a lock, so it is not for the audio
///   thread. Every other method, [`Identifier::as_str`] included, is lock-free and does not
///   allocate.
/// - Empty and non-ASCII names are allowed. [`Identifier::EMPTY`] is the empty name and the
///   default.
///
/// ```
/// use syrp_var::Identifier;
///
/// let a = Identifier::new("gain");
/// let b = Identifier::new(&String::from("gain"));
/// assert_eq!(a, b);
/// assert_eq!(a.as_str(), "gain");
/// assert_eq!(std::mem::size_of::<Identifier>(), 4);
/// ```
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Identifier(u32);

impl Identifier {
    /// The empty name, `""`. It is also [`Identifier::default`].
    ///
    /// ```
    /// use syrp_var::Identifier;
    /// assert_eq!(Identifier::new(""), Identifier::EMPTY);
    /// assert_eq!(Identifier::default(), Identifier::EMPTY);
    /// assert!(Identifier::EMPTY.is_empty());
    /// ```
    pub const EMPTY: Identifier = Identifier(0);

    /// Interns `name` and returns its identifier.
    ///
    /// Equal strings give equal identifiers from any thread. The first call for a name copies it
    /// into memory that is never freed. This takes a process-wide lock and may allocate, so it
    /// must not be called on the audio thread: make identifiers ahead of time and copy them.
    ///
    /// ```
    /// use syrp_var::Identifier;
    /// let id = Identifier::new("größe");
    /// assert_eq!(id.as_str(), "größe");
    /// ```
    ///
    /// # Panics
    /// If the process has already interned `u32::MAX` distinct names.
    #[must_use]
    pub fn new(name: &str) -> Self {
        if name.is_empty() {
            return Self::EMPTY;
        }
        // A panic while holding the lock leaves the map consistent (entries are inserted only
        // once fully built), so a poisoned lock is safe to keep using.
        let mut index = INDEX
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(&id) = index.get(name) {
            return Self(id);
        }
        let id = u32::try_from(index.len() + 1).expect("syrp-var: identifier table is full");
        let text: &'static str = Box::leak(name.to_owned().into_boxed_str());
        let (chunk, offset) = locate(id);
        let slots = NAMES[chunk].get_or_init(|| {
            (0..FIRST_CHUNK << chunk)
                .map(|_| OnceLock::new())
                .collect::<Vec<_>>()
                .into_boxed_slice()
        });
        // The slot is fresh: ids are handed out once, under the lock.
        let _ = slots[offset].set(text);
        index.insert(text, id);
        Self(id)
    }

    /// Returns the interned text. It is `'static` because interned names are never freed.
    ///
    /// Lock-free and allocation-free.
    ///
    /// ```
    /// use syrp_var::Identifier;
    /// let name: &'static str = Identifier::new("width").as_str();
    /// assert_eq!(name, "width");
    /// ```
    #[must_use]
    pub fn as_str(self) -> &'static str {
        if self.0 == 0 {
            return "";
        }
        let (chunk, offset) = locate(self.0);
        // Ids only come from `new`, which fills the slot before returning, and moving an
        // identifier to another thread synchronises with that write.
        NAMES[chunk]
            .get()
            .and_then(|slots| slots[offset].get())
            .copied()
            .expect("syrp-var: identifier not in the name table")
    }

    /// Returns whether this is the empty name.
    ///
    /// ```
    /// use syrp_var::Identifier;
    /// assert!(Identifier::new("").is_empty());
    /// assert!(!Identifier::new("x").is_empty());
    /// ```
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Returns the handle's number: `0` for the empty name, then increasing in interning order.
    ///
    /// The value is stable for the life of the process only; do not persist it.
    ///
    /// ```
    /// use syrp_var::Identifier;
    /// assert_eq!(Identifier::EMPTY.as_u32(), 0);
    /// assert_ne!(Identifier::new("left").as_u32(), 0);
    /// ```
    #[must_use]
    pub fn as_u32(self) -> u32 {
        self.0
    }

    /// Compares the text of two identifiers lexically (by bytes), unlike `Ord`, which follows
    /// interning order.
    ///
    /// ```
    /// use std::cmp::Ordering;
    /// use syrp_var::Identifier;
    /// let (z, a) = (Identifier::new("zz-cmp"), Identifier::new("aa-cmp"));
    /// assert_eq!(a.cmp_text(z), Ordering::Less);
    /// ```
    #[must_use]
    pub fn cmp_text(self, other: Identifier) -> std::cmp::Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl Default for Identifier {
    /// Returns [`Identifier::EMPTY`].
    fn default() -> Self {
        Self::EMPTY
    }
}

impl From<&str> for Identifier {
    /// Same as [`Identifier::new`]; not for the audio thread.
    fn from(name: &str) -> Self {
        Self::new(name)
    }
}

impl AsRef<str> for Identifier {
    /// The interned text, so an identifier can be passed wherever a name is taken as
    /// `impl AsRef<str>` (for example XML attribute names).
    ///
    /// ```
    /// use syrp_var::Identifier;
    /// let id = Identifier::new("gain");
    /// assert_eq!(AsRef::<str>::as_ref(&id), "gain");
    /// ```
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Debug for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Identifier").field(&self.as_str()).finish()
    }
}

impl fmt::Display for Identifier {
    /// Writes the text.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_tile_the_index_space() {
        assert_eq!(locate(0), (0, 0));
        assert_eq!(locate(31), (0, 31));
        assert_eq!(locate(32), (1, 0));
        assert_eq!(locate(95), (1, 63));
        assert_eq!(locate(96), (2, 0));
        let (chunk, offset) = locate(u32::MAX);
        assert!(chunk < CHUNKS);
        assert!(offset < FIRST_CHUNK << chunk);
    }
}

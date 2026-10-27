//! [`MemoryBlock`]: an owned, growable block of bytes.

use std::ops::{Deref, DerefMut};

use crate::{IoError, IoErrorKind};

/// An owned, growable block of bytes: JUCE's `MemoryBlock`.
///
/// It dereferences to `[u8]`, so slicing, indexing and iteration come from the slice. The
/// range-based operations clip out-of-range parts instead of panicking, as the JUCE class
/// documentation describes. Resizes that take a size argument reserve with
/// `try_reserve_exact` and report `SYRP-IO-0005` instead of aborting.
///
/// New bytes are always zero: Syrp never exposes uninitialised memory, so the `zeroed` flags
/// that JUCE takes only exist for porting and do not change the result.
///
/// ```
/// use syrp_io::MemoryBlock;
/// let mut block = MemoryBlock::from_slice(b"abc");
/// block.insert(1, b"XY");
/// block.remove_section(0, 1);
/// assert_eq!(&block[..], b"XYbc");
/// assert_eq!(block.to_string_lossy(), "XYbc");
/// ```
#[doc(alias = "juce::MemoryBlock")]
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MemoryBlock {
    data: Vec<u8>,
}

fn oom(_: std::collections::TryReserveError) -> IoError {
    IoError::new(IoErrorKind::OutOfMemory)
}

/// For copying `n` bytes to `offset` in a block of `len` bytes, returns `(skip, start, count)`:
/// skip `skip` source bytes and copy `count` of them to `start`. `count` is 0 when nothing
/// overlaps.
fn overlap(offset: isize, n: usize, len: usize) -> (usize, usize, usize) {
    let (skip, start) = match usize::try_from(offset) {
        Ok(start) => (0, start),
        Err(_) => (offset.unsigned_abs(), 0),
    };
    if skip >= n || start >= len {
        return (0, 0, 0);
    }
    (skip, start, (n - skip).min(len - start))
}

impl MemoryBlock {
    /// An empty block.
    ///
    /// ```
    /// assert!(syrp_io::MemoryBlock::new().is_empty());
    /// ```
    pub const fn new() -> Self {
        Self { data: Vec::new() }
    }

    /// A block of `size` zero bytes. `zeroed` is accepted for porting; the bytes are always
    /// zero. A failed allocation is `SYRP-IO-0005`.
    ///
    /// ```
    /// let block = syrp_io::MemoryBlock::with_size(3, true)?;
    /// assert_eq!(&block[..], &[0, 0, 0]);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    pub fn with_size(size: usize, zeroed: bool) -> Result<Self, IoError> {
        let mut block = Self::new();
        block.set_size(size, zeroed)?;
        Ok(block)
    }

    /// A block holding a copy of `bytes`.
    ///
    /// ```
    /// assert_eq!(syrp_io::MemoryBlock::from_slice(&[1, 2]).len(), 2);
    /// ```
    pub fn from_slice(bytes: &[u8]) -> Self {
        Self {
            data: bytes.to_vec(),
        }
    }

    /// Takes the block's bytes back as a `Vec<u8>`, without copying.
    ///
    /// ```
    /// assert_eq!(syrp_io::MemoryBlock::from(vec![4u8]).into_vec(), vec![4]);
    /// ```
    pub fn into_vec(self) -> Vec<u8> {
        self.data
    }

    /// A borrowed view of the bytes; the same as dereferencing.
    ///
    /// ```
    /// let block = syrp_io::MemoryBlock::from_slice(b"hi");
    /// assert_eq!(block.as_view(), b"hi");
    /// ```
    pub fn as_view(&self) -> &[u8] {
        &self.data
    }

    /// The size in bytes.
    ///
    /// ```
    /// assert_eq!(syrp_io::MemoryBlock::from_slice(b"abc").len(), 3);
    /// ```
    #[doc(alias = "getSize")]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Whether the block holds no bytes.
    ///
    /// ```
    /// assert!(syrp_io::MemoryBlock::default().is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Grows or shrinks to `size` bytes, keeping the existing prefix; new bytes are zero.
    /// `zero_new_bytes` is accepted for porting. A failed allocation is `SYRP-IO-0005` and
    /// leaves the block unchanged.
    ///
    /// ```
    /// let mut block = syrp_io::MemoryBlock::from_slice(&[1, 2, 3]);
    /// block.set_size(4, false)?;
    /// assert_eq!(&block[..], &[1, 2, 3, 0]);
    /// block.set_size(1, false)?;
    /// assert_eq!(&block[..], &[1]);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    #[doc(alias = "setSize")]
    pub fn set_size(&mut self, size: usize, zero_new_bytes: bool) -> Result<(), IoError> {
        let _ = zero_new_bytes;
        if let Some(extra) = size.checked_sub(self.data.len()) {
            self.data.try_reserve_exact(extra).map_err(oom)?;
        }
        self.data.resize(size, 0);
        Ok(())
    }

    /// Grows to at least `min_size` bytes, filling new bytes with zero; never shrinks. A failed
    /// allocation is `SYRP-IO-0005`.
    ///
    /// ```
    /// let mut block = syrp_io::MemoryBlock::from_slice(&[1, 2]);
    /// block.ensure_size(1)?;
    /// assert_eq!(block.len(), 2);
    /// block.ensure_size(3)?;
    /// assert_eq!(&block[..], &[1, 2, 0]);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    #[doc(alias = "ensureSize")]
    pub fn ensure_size(&mut self, min_size: usize) -> Result<(), IoError> {
        if min_size > self.data.len() {
            self.set_size(min_size, true)?;
        }
        Ok(())
    }

    /// Sets every byte to `byte`.
    ///
    /// ```
    /// let mut block = syrp_io::MemoryBlock::with_size(2, true)?;
    /// block.fill(7);
    /// assert_eq!(&block[..], &[7, 7]);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    #[doc(alias = "fillWith")]
    pub fn fill(&mut self, byte: u8) {
        self.data.fill(byte);
    }

    /// Adds `bytes` at the end.
    ///
    /// ```
    /// let mut block = syrp_io::MemoryBlock::from_slice(b"a");
    /// block.append(b"bc");
    /// assert_eq!(&block[..], b"abc");
    /// ```
    pub fn append(&mut self, bytes: &[u8]) {
        self.data.extend_from_slice(bytes);
    }

    /// Inserts `bytes` before byte `offset`; an offset past the end appends.
    ///
    /// ```
    /// let mut block = syrp_io::MemoryBlock::from_slice(b"ac");
    /// block.insert(1, b"b");
    /// block.insert(99, b"d");
    /// assert_eq!(&block[..], b"abcd");
    /// ```
    pub fn insert(&mut self, offset: usize, bytes: &[u8]) {
        let at = offset.min(self.data.len());
        self.data.splice(at..at, bytes.iter().copied());
    }

    /// Removes up to `len` bytes starting at `start`; the part past the end is ignored.
    ///
    /// ```
    /// let mut block = syrp_io::MemoryBlock::from_slice(b"abcd");
    /// block.remove_section(2, 99);
    /// assert_eq!(&block[..], b"ab");
    /// ```
    #[doc(alias = "removeSection")]
    pub fn remove_section(&mut self, start: usize, len: usize) {
        let size = self.data.len();
        let start = start.min(size);
        let end = start.saturating_add(len).min(size);
        self.data.drain(start..end);
    }

    /// Replaces the whole contents with a copy of `bytes`; the size becomes `bytes.len()`.
    ///
    /// ```
    /// let mut block = syrp_io::MemoryBlock::from_slice(b"old data");
    /// block.replace_all(b"new");
    /// assert_eq!(&block[..], b"new");
    /// ```
    #[doc(alias = "replaceAll")]
    pub fn replace_all(&mut self, bytes: &[u8]) {
        self.data.clear();
        self.data.extend_from_slice(bytes);
    }

    /// Copies `src` into the block so that `src[0]` lands at `dest_offset`, which may be
    /// negative. Only the part that falls inside the block is copied; the size never changes.
    ///
    /// ```
    /// let mut block = syrp_io::MemoryBlock::with_size(3, true)?;
    /// block.copy_from(&[1, 2, 3], -1);
    /// assert_eq!(&block[..], &[2, 3, 0]);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    #[doc(alias = "copyFrom")]
    pub fn copy_from(&mut self, src: &[u8], dest_offset: isize) {
        let (skip, start, n) = overlap(dest_offset, src.len(), self.data.len());
        self.data[start..start + n].copy_from_slice(&src[skip..skip + n]);
    }

    /// Fills `dest` from the block starting at `src_offset`, which may be negative. Positions
    /// of `dest` that fall outside the block are set to zero.
    ///
    /// ```
    /// let block = syrp_io::MemoryBlock::from_slice(&[1, 2]);
    /// let mut out = [9u8; 4];
    /// block.copy_to(&mut out, -1);
    /// assert_eq!(out, [0, 1, 2, 0]);
    /// ```
    #[doc(alias = "copyTo")]
    pub fn copy_to(&self, dest: &mut [u8], src_offset: isize) {
        dest.fill(0);
        let (skip, start, n) = overlap(src_offset, dest.len(), self.data.len());
        dest[skip..skip + n].copy_from_slice(&self.data[start..start + n]);
    }

    /// Reads `num_bits` bits (at most 32; more are clamped) starting at bit `bit_start`, where
    /// bit `i` is bit `i % 8` of byte `i / 8` and the first bit read is bit 0 of the result.
    /// Bits past the end read as zero.
    ///
    /// ```
    /// let block = syrp_io::MemoryBlock::from_slice(&[0b1011_0000, 0b0000_0001]);
    /// assert_eq!(block.bit_range(4, 5), 0b1_1011);
    /// assert_eq!(block.bit_range(100, 8), 0);
    /// ```
    #[doc(alias = "getBitRange")]
    pub fn bit_range(&self, bit_start: usize, num_bits: usize) -> u32 {
        (0..num_bits.min(32)).fold(0, |acc, i| {
            let set = bit_start
                .checked_add(i)
                .and_then(|bit| Some((self.data.get(bit / 8)? >> (bit % 8)) & 1 == 1));
            if set == Some(true) { acc | 1 << i } else { acc }
        })
    }

    /// Writes the low `num_bits` bits of `value` (at most 32; more are clamped) starting at bit
    /// `bit_start`, in the order [`MemoryBlock::bit_range`] reads them. Bits past the end are
    /// dropped.
    ///
    /// ```
    /// let mut block = syrp_io::MemoryBlock::with_size(2, true)?;
    /// block.set_bit_range(4, 8, 0xAB);
    /// assert_eq!(&block[..], &[0xB0, 0x0A]);
    /// assert_eq!(block.bit_range(4, 8), 0xAB);
    /// # Ok::<(), syrp_io::IoError>(())
    /// ```
    #[doc(alias = "setBitRange")]
    pub fn set_bit_range(&mut self, bit_start: usize, num_bits: usize, value: u32) {
        for i in 0..num_bits.min(32) {
            let Some(bit) = bit_start.checked_add(i) else {
                return;
            };
            let Some(byte) = self.data.get_mut(bit / 8) else {
                return;
            };
            let mask = 1u8 << (bit % 8);
            if value >> i & 1 == 1 {
                *byte |= mask;
            } else {
                *byte &= !mask;
            }
        }
    }

    /// Exchanges the contents of two blocks without copying.
    ///
    /// ```
    /// let mut a = syrp_io::MemoryBlock::from_slice(b"a");
    /// let mut b = syrp_io::MemoryBlock::from_slice(b"bb");
    /// a.swap_with(&mut b);
    /// assert_eq!((&a[..], &b[..]), (&b"bb"[..], &b"a"[..]));
    /// ```
    #[doc(alias = "swapWith")]
    pub fn swap_with(&mut self, other: &mut MemoryBlock) {
        std::mem::swap(&mut self.data, &mut other.data);
    }

    /// The bytes as UTF-8 text, with invalid sequences replaced by U+FFFD.
    ///
    /// ```
    /// assert_eq!(syrp_io::MemoryBlock::from_slice(b"ok\xFF").to_string_lossy(), "ok\u{FFFD}");
    /// ```
    #[doc(alias = "toString")]
    pub fn to_string_lossy(&self) -> String {
        String::from_utf8_lossy(&self.data).into_owned()
    }

    /// The backing vector, for stream code in this crate that appends to a block.
    pub(crate) fn vec_mut(&mut self) -> &mut Vec<u8> {
        &mut self.data
    }
}

impl From<Vec<u8>> for MemoryBlock {
    fn from(data: Vec<u8>) -> Self {
        Self { data }
    }
}

impl Deref for MemoryBlock {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.data
    }
}

impl DerefMut for MemoryBlock {
    fn deref_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }
}

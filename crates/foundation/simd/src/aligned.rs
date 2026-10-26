// SPDX-License-Identifier: MIT OR Apache-2.0
//! [`AlignedBuf`], a fixed-length, zero-initialised heap buffer aligned to 64 bytes.

use core::fmt;
use core::ops::{Deref, DerefMut};
use core::ptr::NonNull;
use std::alloc::{self, Layout};

use crate::sample::Sealed;
use crate::{F32x4, F32x8, F64x2, F64x4};

/// The alignment of every non-empty [`AlignedBuf`], in bytes (one cache line, one AVX-512
/// register).
pub const ALIGN: usize = 64;

/// Element types an [`AlignedBuf`] can hold: types for which all-zero bytes are a valid
/// `0.0` value. Sealed; implemented for `f32`, `f64` and the lane packs.
pub trait Zeroable: Copy + Sealed {}
impl Zeroable for f32 {}
impl Zeroable for f64 {}
impl Zeroable for F32x4 {}
impl Zeroable for F32x8 {}
impl Zeroable for F64x2 {}
impl Zeroable for F64x4 {}

/// A heap buffer of `len` zeroed elements whose first element sits on a 64-byte boundary.
///
/// The length is fixed at construction and the buffer never reallocates, so slices taken
/// from it stay valid and aligned for its whole life. Allocation happens only in
/// [`zeroed`](AlignedBuf::zeroed); dropping frees it. Neither belongs on the audio thread.
///
/// ```
/// use syrp_simd::AlignedBuf;
/// let mut buf = AlignedBuf::<f32>::zeroed(100);
/// assert_eq!(buf.len(), 100);
/// assert_eq!(buf.as_slice().as_ptr() as usize % 64, 0);
/// buf.as_mut_slice()[3] = 1.0;
/// assert_eq!(buf[3], 1.0);
/// ```
pub struct AlignedBuf<T: Zeroable> {
    ptr: NonNull<T>,
    len: usize,
}

// SAFETY: `AlignedBuf<T>` owns its elements exactly as `Box<[T]>` does, so it is `Send` when
// `T` is `Send`.
unsafe impl<T: Zeroable + Send> Send for AlignedBuf<T> {}
// SAFETY: shared access only hands out `&[T]`, so it is `Sync` when `T` is `Sync`.
unsafe impl<T: Zeroable + Sync> Sync for AlignedBuf<T> {}

impl<T: Zeroable> AlignedBuf<T> {
    /// Allocates `len` elements, every one `0.0`.
    ///
    /// # Panics
    /// When `len * size_of::<T>()` overflows `isize`. Allocation failure aborts through
    /// [`std::alloc::handle_alloc_error`], as `Vec` does.
    ///
    /// ```
    /// let buf = syrp_simd::AlignedBuf::<f64>::zeroed(3);
    /// assert_eq!(buf.as_slice(), &[0.0; 3]);
    /// ```
    pub fn zeroed(len: usize) -> Self {
        let Some(layout) = Self::layout(len) else {
            return Self {
                ptr: NonNull::dangling(),
                len: 0,
            };
        };
        // SAFETY: `layout` has a non-zero size (checked in `layout`).
        let raw = unsafe { alloc::alloc_zeroed(layout) };
        let Some(ptr) = NonNull::new(raw.cast::<T>()) else {
            alloc::handle_alloc_error(layout)
        };
        Self { ptr, len }
    }

    /// The layout for `len` elements, or `None` when it would be empty.
    fn layout(len: usize) -> Option<Layout> {
        let size = size_of::<T>()
            .checked_mul(len)
            .expect("AlignedBuf length overflows usize");
        if size == 0 {
            return None;
        }
        let align = ALIGN.max(align_of::<T>());
        Some(Layout::from_size_align(size, align).expect("AlignedBuf length overflows isize"))
    }

    /// Number of elements.
    ///
    /// ```
    /// assert_eq!(syrp_simd::AlignedBuf::<f32>::zeroed(7).len(), 7);
    /// ```
    pub fn len(&self) -> usize {
        self.len
    }

    /// True when the buffer holds no elements.
    ///
    /// ```
    /// assert!(syrp_simd::AlignedBuf::<f32>::zeroed(0).is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The elements as a slice.
    ///
    /// ```
    /// assert_eq!(syrp_simd::AlignedBuf::<f32>::zeroed(2).as_slice(), &[0.0, 0.0]);
    /// ```
    pub fn as_slice(&self) -> &[T] {
        // SAFETY: `ptr` is non-null and aligned for `T`; it is either dangling with `len == 0`
        // or points to `len` initialised elements (all-zero bytes are valid for `Zeroable`)
        // owned by `self`, and `&self` prevents mutation for the borrow.
        unsafe { core::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }

    /// The elements as a mutable slice.
    ///
    /// ```
    /// let mut buf = syrp_simd::AlignedBuf::<f32>::zeroed(2);
    /// buf.as_mut_slice().fill(0.5);
    /// assert_eq!(buf.as_slice(), &[0.5, 0.5]);
    /// ```
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        // SAFETY: as in `as_slice`, and `&mut self` makes this the only live reference.
        unsafe { core::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}

impl<T: Zeroable> Drop for AlignedBuf<T> {
    fn drop(&mut self) {
        if let Some(layout) = Self::layout(self.len) {
            // SAFETY: `ptr` came from `alloc_zeroed` with this same layout (it depends only on
            // `len`, which never changes) and is freed exactly once, here.
            unsafe { alloc::dealloc(self.ptr.as_ptr().cast(), layout) };
        }
    }
}

impl<T: Zeroable> Deref for AlignedBuf<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        self.as_slice()
    }
}

impl<T: Zeroable> DerefMut for AlignedBuf<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        self.as_mut_slice()
    }
}

impl<T: Zeroable + fmt::Debug> fmt::Debug for AlignedBuf<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.as_slice()).finish()
    }
}

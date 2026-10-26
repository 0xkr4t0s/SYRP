// SPDX-License-Identifier: MIT OR Apache-2.0
//! A debug allocation guard for tests: [`GuardedAllocator`] plus [`no_alloc`].

use core::alloc::{GlobalAlloc, Layout};
use core::cell::Cell;
use std::alloc::System;
use std::io::Write as _;

thread_local! {
    /// Whether the current thread is inside `no_alloc`. Const-initialised with no destructor,
    /// so reading it from the allocator never allocates.
    static FORBIDDEN: Cell<bool> = const { Cell::new(false) };
}

/// The message printed before aborting. It carries the catalogued code (`errors/rt.toml`).
const MESSAGE: &[u8] = b"error[SYRP-RT-0001]: heap allocation or deallocation inside \
    syrp_rt::no_alloc (audio-thread code must not touch the allocator); aborting\n";

/// A `#[global_allocator]` that forwards to [`System`] and, in debug builds, aborts the
/// process with `SYRP-RT-0001` when the current thread allocates, reallocates or frees inside
/// [`no_alloc`]. Outside `no_alloc`, and in release builds, it is `System` plus one
/// thread-local read.
///
/// Install it in test binaries (or debug builds of apps) only:
///
/// ```
/// #[global_allocator]
/// static ALLOC: syrp_rt::GuardedAllocator = syrp_rt::GuardedAllocator;
///
/// let v = vec![1, 2, 3]; // allowed: not inside `no_alloc`
/// let sum = syrp_rt::no_alloc(|| v.iter().sum::<i32>()); // allowed: no allocation
/// assert_eq!(sum, 6);
/// ```
#[doc(alias = "assert_no_alloc")]
#[derive(Debug, Default, Clone, Copy)]
pub struct GuardedAllocator;

impl GuardedAllocator {
    /// Aborts if the current thread is inside `no_alloc` (debug builds only).
    #[inline]
    fn check() {
        if cfg!(debug_assertions) && FORBIDDEN.try_with(Cell::get).unwrap_or(false) {
            // Lift the guard first so that anything the write path does cannot recurse here.
            let _ = FORBIDDEN.try_with(|f| f.set(false));
            let _ = std::io::stderr().write_all(MESSAGE);
            std::process::abort();
        }
    }
}

// SAFETY: every method forwards to `System` with the caller's arguments unchanged, so
// `System`'s guarantees carry over; `check` either returns or aborts the process.
unsafe impl GlobalAlloc for GuardedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        Self::check();
        // SAFETY: forwarded from our caller, who upholds `GlobalAlloc::alloc`'s contract.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        Self::check();
        // SAFETY: forwarded from our caller, who upholds `GlobalAlloc::alloc_zeroed`'s contract.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        Self::check();
        // SAFETY: `ptr` was allocated by `System` through this allocator with `layout`.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        Self::check();
        // SAFETY: forwarded from our caller, who upholds `GlobalAlloc::realloc`'s contract;
        // `ptr` came from `System` through this allocator.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

/// Runs `f` and returns its result, with the current thread barred from the allocator.
///
/// With [`GuardedAllocator`] installed, any allocation, reallocation or deallocation on this
/// thread inside `f` aborts the process (debug builds) with `SYRP-RT-0001` in the message. Use
/// it in tests to prove that audio-thread code is allocation-free. Other threads are not
/// affected. Calls nest, and the previous state is restored when `f` returns or unwinds
/// (a panic allocates, so with the guard active it aborts instead). Without
/// `GuardedAllocator`, or in release builds, it simply runs `f`.
///
/// ```
/// # #[global_allocator]
/// # static ALLOC: syrp_rt::GuardedAllocator = syrp_rt::GuardedAllocator;
/// let mut buf = [0.0_f32; 64];
/// syrp_rt::no_alloc(|| buf.iter_mut().for_each(|s| *s = 0.5));
/// assert_eq!(buf[63], 0.5);
/// ```
pub fn no_alloc<R>(f: impl FnOnce() -> R) -> R {
    /// Restores the previous state on scope exit, including unwinding.
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            let _ = FORBIDDEN.try_with(|c| c.set(self.0));
        }
    }
    let _restore = Restore(FORBIDDEN.try_with(|c| c.replace(true)).unwrap_or(false));
    f()
}

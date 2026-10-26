// SPDX-License-Identifier: MIT OR Apache-2.0
//! Real-time primitives for code that runs on Syrp audio threads (master plan §5.4).
//!
//! Everything here is allocation-free, lock-free and non-blocking once constructed:
//!
//! - [`RtSafe`]: the `unsafe` marker for types usable from `process`.
//! - [`AtomicF32`] and [`AtomicF64`]: float atomics for parameters and meters.
//! - [`spsc`]: a bounded single-producer, single-consumer queue.
//! - [`DeferredDrop`] and [`Collector`]: hand values from the audio thread to another thread
//!   so their destructors (and deallocations) never run on the audio thread.
//! - [`RtHandoff`]: publish a new `Box<T>` to an audio thread; the old one is reclaimed through
//!   [`DeferredDrop`].
//! - [`GuardedAllocator`] and [`no_alloc`]: a debug-build allocation guard for tests.
//!
//! ```
//! use syrp_rt::{AtomicF32, RtSafe};
//! use std::sync::atomic::Ordering;
//!
//! fn assert_rt_safe<T: RtSafe>(_: &T) {}
//! let gain = AtomicF32::new(0.5);
//! assert_rt_safe(&gain);
//! gain.store(0.25, Ordering::Relaxed);
//! assert_eq!(gain.load(Ordering::Relaxed), 0.25);
//! ```

mod alloc_guard;
mod atomic;
mod deferred;
mod handoff;
pub mod spsc;
mod sync;

pub use alloc_guard::{GuardedAllocator, no_alloc};
pub use atomic::{AtomicF32, AtomicF64};
pub use deferred::{Collector, DeferredDrop};
pub use handoff::{HandoffReader, RtHandoff};

/// Marker for types that may be used on an audio thread.
///
/// # Safety
/// Implementing `RtSafe` asserts that, on an audio thread:
///
/// 1. moving a value of the type, and calling any of its methods documented as RT-safe, never
///    allocates or frees heap memory, takes a lock, blocks, waits on another thread or does I/O;
/// 2. if the type has no destructor (primitives, and arrays and tuples of them), dropping it is
///    covered by rule 1 too. A type with a destructor documents where it may be dropped: the
///    handle types in this crate may free shared storage when they are the last handle alive,
///    so drop them off the audio thread.
///
/// The compiler cannot check any of this, so the trait is `unsafe` to implement, and claiming it
/// puts a PR in the high-risk review category (master plan §5.4). Implemented here for the
/// primitive numeric types, `bool`, `char`, `()`, arrays and tuples (up to 8) of `RtSafe`
/// types, and this crate's own types.
///
/// ```
/// use syrp_rt::RtSafe;
///
/// #[derive(Clone, Copy)]
/// struct Biquad { a1: f32, a2: f32, b0: f32, b1: f32, b2: f32 }
/// // SAFETY: plain `Copy` data with no destructor and no RT-unsafe methods.
/// unsafe impl RtSafe for Biquad {}
///
/// fn needs_rt<T: RtSafe>() {}
/// needs_rt::<Biquad>();
/// needs_rt::<[(f32, u8); 4]>();
/// ```
#[doc(alias = "realtime-safe")]
pub unsafe trait RtSafe {}

/// Implements [`RtSafe`] for a list of primitive types.
///
/// A macro because the same empty `unsafe impl` is needed for 17 types; writing them out
/// would add only noise.
macro_rules! rt_safe_primitives {
    ($($t:ty),* $(,)?) => {
        // SAFETY: primitives are plain bits with no destructor; moving them is a copy.
        $(unsafe impl RtSafe for $t {})*
    };
}
#[rustfmt::skip]
rt_safe_primitives!(
    u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize, f32, f64, bool, char, ()
);

// SAFETY: an array is its elements laid out inline; each element is `RtSafe`.
unsafe impl<T: RtSafe, const N: usize> RtSafe for [T; N] {}

/// Implements [`RtSafe`] for tuples whose elements are all `RtSafe`.
///
/// A macro because one impl per arity (1 to 8) is otherwise eight near-identical lines.
macro_rules! rt_safe_tuples {
    ($(($($t:ident),+)),+ $(,)?) => {
        // SAFETY: a tuple is its fields laid out inline; each field is `RtSafe`.
        $(unsafe impl<$($t: RtSafe),+> RtSafe for ($($t,)+) {})+
    };
}
#[rustfmt::skip]
rt_safe_tuples!(
    (A), (A, B), (A, B, C), (A, B, C, D), (A, B, C, D, E), (A, B, C, D, E, F),
    (A, B, C, D, E, F, G), (A, B, C, D, E, F, G, H),
);

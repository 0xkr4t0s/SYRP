// SPDX-License-Identifier: MIT OR Apache-2.0
//! `AtomicF32` and `AtomicF64`: floats stored as their bit patterns in integer atomics.

use crate::RtSafe;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// Defines a float atomic over an integer atomic of the same width.
///
/// A macro because `AtomicF32` and `AtomicF64` are identical apart from the float type, the
/// integer atomic and the doc examples; two hand-written copies would drift apart.
macro_rules! atomic_float {
    ($name:ident, $float:ident, $atomic:ident, $alias:literal) => {
        #[doc = concat!("An `", stringify!($float), "` that can be shared between threads.")]
        ///
        /// Stored as the float's bit pattern in an integer atomic, so every value round-trips
        /// exactly, including `-0.0`, infinities and NaN payloads. All methods are RT-safe:
        /// each one is a single atomic instruction or a compare-and-swap loop.
        ///
        /// ```
        #[doc = concat!("use syrp_rt::", stringify!($name), ";")]
        /// use std::sync::atomic::Ordering;
        ///
        #[doc = concat!("let level = ", stringify!($name), "::new(1.5);")]
        /// assert_eq!(level.swap(2.0, Ordering::AcqRel), 1.5);
        /// assert_eq!(level.load(Ordering::Acquire), 2.0);
        /// ```
        #[doc(alias = $alias)]
        #[derive(Default)]
        pub struct $name($atomic);

        impl $name {
            /// Creates an atomic holding `value`.
            ///
            /// ```
            #[doc = concat!("# use syrp_rt::", stringify!($name), ";")]
            #[doc = concat!("static GAIN: ", stringify!($name), " = ", stringify!($name), "::new(1.0);")]
            /// assert_eq!(GAIN.load(std::sync::atomic::Ordering::Relaxed), 1.0);
            /// ```
            #[must_use]
            pub const fn new(value: $float) -> Self {
                Self($atomic::new(value.to_bits()))
            }

            /// Reads the value with the given ordering (`Release` and `AcqRel` panic, as for
            /// the integer atomics).
            ///
            /// ```
            #[doc = concat!("# use syrp_rt::", stringify!($name), ";")]
            #[doc = concat!("let a = ", stringify!($name), "::new(-0.0);")]
            /// assert!(a.load(std::sync::atomic::Ordering::Relaxed).is_sign_negative());
            /// ```
            #[must_use]
            pub fn load(&self, order: Ordering) -> $float {
                $float::from_bits(self.0.load(order))
            }

            /// Writes `value` with the given ordering (`Acquire` and `AcqRel` panic).
            ///
            /// ```
            #[doc = concat!("# use syrp_rt::", stringify!($name), ";")]
            /// # use std::sync::atomic::Ordering;
            #[doc = concat!("let a = ", stringify!($name), "::new(0.0);")]
            /// a.store(3.0, Ordering::Release);
            /// assert_eq!(a.load(Ordering::Acquire), 3.0);
            /// ```
            pub fn store(&self, value: $float, order: Ordering) {
                self.0.store(value.to_bits(), order);
            }

            /// Stores `value` and returns the previous value.
            ///
            /// ```
            #[doc = concat!("# use syrp_rt::", stringify!($name), ";")]
            /// # use std::sync::atomic::Ordering;
            #[doc = concat!("let a = ", stringify!($name), "::new(1.0);")]
            /// assert_eq!(a.swap(4.0, Ordering::AcqRel), 1.0);
            /// ```
            pub fn swap(&self, value: $float, order: Ordering) -> $float {
                $float::from_bits(self.0.swap(value.to_bits(), order))
            }

            /// Applies `f` in a compare-and-swap loop until it succeeds or `f` returns `None`.
            ///
            /// Returns `Ok(previous)` on success and `Err(current)` when `f` declined. The
            /// orderings mean the same as in `AtomicU32::fetch_update`. Values compare by bit
            /// pattern, so a NaN never makes the loop spin.
            ///
            /// ```
            #[doc = concat!("# use syrp_rt::", stringify!($name), ";")]
            /// # use std::sync::atomic::Ordering::{AcqRel, Acquire};
            #[doc = concat!("let peak = ", stringify!($name), "::new(0.5);")]
            /// // Keep the maximum.
            /// assert_eq!(peak.fetch_update(AcqRel, Acquire, |p| (0.75 > p).then_some(0.75)), Ok(0.5));
            /// assert_eq!(peak.fetch_update(AcqRel, Acquire, |p| (0.25 > p).then_some(0.25)), Err(0.75));
            /// ```
            pub fn fetch_update(
                &self,
                set_order: Ordering,
                fetch_order: Ordering,
                mut f: impl FnMut($float) -> Option<$float>,
            ) -> Result<$float, $float> {
                let mut prev = self.0.load(fetch_order);
                while let Some(next) = f($float::from_bits(prev)) {
                    match self.0.compare_exchange_weak(prev, next.to_bits(), set_order, fetch_order)
                    {
                        Ok(_) => return Ok($float::from_bits(prev)),
                        Err(actual) => prev = actual,
                    }
                }
                Err($float::from_bits(prev))
            }

            /// Consumes the atomic and returns the value.
            ///
            /// ```
            #[doc = concat!("# use syrp_rt::", stringify!($name), ";")]
            #[doc = concat!("assert_eq!(", stringify!($name), "::new(2.5).into_inner(), 2.5);")]
            /// ```
            #[must_use]
            pub fn into_inner(self) -> $float {
                $float::from_bits(self.0.into_inner())
            }
        }

        impl core::fmt::Debug for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                core::fmt::Debug::fmt(&self.load(Ordering::Relaxed), f)
            }
        }

        // SAFETY: every method is one atomic operation or a CAS loop on an inline integer
        // atomic; there is no heap memory, lock or destructor.
        unsafe impl RtSafe for $name {}
    };
}

atomic_float!(AtomicF32, f32, AtomicU32, "atomic_float");
atomic_float!(AtomicF64, f64, AtomicU64, "atomic_double");

// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Vector operations over `f32` and `f64` slices, the role of JUCE's `FloatVectorOperations`.
//!
//! Empty in this increment: the operations (spec B5) arrive in a later increment of WP-2.1-01,
//! built on `syrp_simd::kernels` and, where no kernel exists, written once against
//! `syrp_simd::Sample` and run through `syrp_simd::dispatch!` with `#[inline(always)]` down
//! the call path (ADR 0003 D5).

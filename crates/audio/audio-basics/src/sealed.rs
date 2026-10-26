// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The sealed half of [`BufferSample`](crate::BufferSample): conversions and the lane pack
//! that `vector_ops` processes each element type in (ADR 0003 D5).

use syrp_simd::{F32x8, F64x4, Sample};

/// Seals `BufferSample` and maps each element type to its widest lane pack.
///
/// Every method is `#[inline(always)]`, so it is compiled for the level of the `dispatch!`
/// body that calls it (the `syrp-simd` inlining rule).
pub trait Sealed: Copy {
    /// The lane pack `vector_ops` uses for this element (`F32x8`, `F64x4`).
    type Pack: Sample<Element = Self>;
    /// Lanes in [`Pack`](Self::Pack).
    const PACK_LANES: usize;
    fn to_f64(self) -> f64;
    /// Rounds to nearest when narrowing.
    fn from_f64(value: f64) -> Self;
    /// Loads up to `LANES` elements, filling the missing lanes with `fill`.
    fn load(src: &[Self], fill: Self) -> Self::Pack;
    /// Stores the first `dst.len()` (at most `LANES`) lanes.
    fn store(pack: Self::Pack, dst: &mut [Self]);
    /// Folds the lanes of `pack` with `f`, first lane first.
    fn reduce(pack: Self::Pack, f: impl Fn(Self, Self) -> Self) -> Self;
}

impl Sealed for f32 {
    type Pack = F32x8;
    const PACK_LANES: usize = 8;
    #[inline(always)]
    fn to_f64(self) -> f64 {
        f64::from(self)
    }
    #[inline(always)]
    fn from_f64(value: f64) -> Self {
        value as f32
    }
    #[inline(always)]
    fn load(src: &[f32], fill: f32) -> F32x8 {
        let mut lanes = [fill; 8];
        lanes[..src.len()].copy_from_slice(src);
        F32x8::from_array(lanes)
    }
    #[inline(always)]
    fn store(pack: F32x8, dst: &mut [f32]) {
        let n = dst.len();
        dst.copy_from_slice(&pack.to_array()[..n]);
    }
    #[inline(always)]
    fn reduce(pack: F32x8, f: impl Fn(f32, f32) -> f32) -> f32 {
        let [first, rest @ ..] = pack.to_array();
        rest.into_iter().fold(first, f)
    }
}

impl Sealed for f64 {
    type Pack = F64x4;
    const PACK_LANES: usize = 4;
    #[inline(always)]
    fn to_f64(self) -> f64 {
        self
    }
    #[inline(always)]
    fn from_f64(value: f64) -> Self {
        value
    }
    #[inline(always)]
    fn load(src: &[f64], fill: f64) -> F64x4 {
        let mut lanes = [fill; 4];
        lanes[..src.len()].copy_from_slice(src);
        F64x4::from_array(lanes)
    }
    #[inline(always)]
    fn store(pack: F64x4, dst: &mut [f64]) {
        let n = dst.len();
        dst.copy_from_slice(&pack.to_array()[..n]);
    }
    #[inline(always)]
    fn reduce(pack: F64x4, f: impl Fn(f64, f64) -> f64) -> f64 {
        let [first, rest @ ..] = pack.to_array();
        rest.into_iter().fold(first, f)
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0
//! Slice kernels with a scalar reference path.
//!
//! Every kernel takes the [`Level`] to run at; pass [`Level::detect()`] in normal use.
//! [`Level::Scalar`] runs the scalar reference, which the property tests compare every SIMD
//! level against. All six kernels are exact on every level: `add`, `mul`, `copy_with_gain`
//! and `mul_add` round once per element (`mul_add` is fused, even on levels without an FMA
//! instruction), and the reductions return an input value, so every level returns the same
//! bits as the reference for NaN-free input (up to the sign of a zero in `find_min_max`).
//!
//! Kernels never allocate or lock, so they are safe on the audio thread. Kernels that take
//! several slices panic when their lengths differ.
//!
//! ```
//! use syrp_simd::{kernels, Level};
//! let level = Level::detect();
//! let mut out = [1.0_f32; 5];
//! kernels::mul_add(level, &mut out, &[2.0; 5], &[3.0; 5]);
//! assert_eq!(out, [7.0; 5]);
//! assert_eq!(kernels::find_min_max(level, &[3.0_f32, -1.0, 2.0]), Some((-1.0, 3.0)));
//! ```

use fearless_simd::{Simd, SimdBase, SimdFloat};

use crate::{Level, Sample, dispatch};

/// The element types the kernels accept: `f32` and `f64`. Sealed.
pub trait Element: Sample<Element = Self> + fearless_simd::SimdFloatElement {
    /// The native-width `fearless_simd` vector of this element at level `S`.
    #[doc(hidden)]
    type Vector<S: Simd>: SimdFloat<S, Element = Self>;
}
impl Element for f32 {
    type Vector<S: Simd> = S::f32s;
}
impl Element for f64 {
    type Vector<S: Simd> = S::f64s;
}

/// `dst[i] = vf(dst[i], ins[k][i]...)` in native-width chunks, `sf` on the tail.
#[inline(always)]
fn update<S: Simd, E: Element, const K: usize>(
    simd: S,
    dst: &mut [E],
    ins: [&[E]; K],
    vf: impl Fn(E::Vector<S>, [E::Vector<S>; K]) -> E::Vector<S>,
    sf: impl Fn(E, [E; K]) -> E,
) {
    const { assert!(E::Vector::<S>::LEN > 0) };
    let n = E::Vector::<S>::LEN;
    let len = dst.len();
    // Reslicing to `len` and counting whole chunks lets LLVM drop the per-chunk bounds checks.
    let ins = ins.map(|s| &s[..len]);
    let end = len / n * n;
    for c in 0..len / n {
        let r = c * n..c * n + n;
        let d = E::Vector::<S>::from_slice(simd, &dst[r.clone()]);
        let v = ins.map(|s| E::Vector::<S>::from_slice(simd, &s[r.clone()]));
        vf(d, v).store_slice(&mut dst[r]);
    }
    scalar_update(&mut dst[end..], ins.map(|s| &s[end..]), sf);
}

/// The scalar reference of [`update`].
#[inline(always)]
fn scalar_update<E: Element, const K: usize>(
    dst: &mut [E],
    ins: [&[E]; K],
    sf: impl Fn(E, [E; K]) -> E,
) {
    for (j, d) in dst.iter_mut().enumerate() {
        *d = sf(*d, ins.map(|s| s[j]));
    }
}

fn check_lengths<E, const K: usize>(dst: &[E], ins: &[&[E]; K]) {
    for s in ins {
        assert_eq!(s.len(), dst.len(), "kernel slices must have equal lengths");
    }
}

/// `dst[i] += src[i]`.
///
/// ```
/// let mut d = [1.0_f64, 2.0];
/// syrp_simd::kernels::add(syrp_simd::Level::detect(), &mut d, &[0.5, 0.5]);
/// assert_eq!(d, [1.5, 2.5]);
/// ```
#[doc(alias = "FloatVectorOperations::add")]
pub fn add<E: Element>(level: Level, dst: &mut [E], src: &[E]) {
    check_lengths(dst, &[src]);
    dispatch!(level, simd => update(simd, dst, [src], |d, [s]| d + s, |d, [s]| d + s),
        scalar => scalar_update(dst, [src], |d, [s]| d + s));
}

/// `out[i] = a[i] + b[i]`.
///
/// ```
/// let mut o = [0.0_f32; 3];
/// syrp_simd::kernels::add_into(syrp_simd::Level::detect(), &mut o, &[1.0; 3], &[2.0; 3]);
/// assert_eq!(o, [3.0; 3]);
/// ```
pub fn add_into<E: Element>(level: Level, out: &mut [E], a: &[E], b: &[E]) {
    check_lengths(out, &[a, b]);
    dispatch!(level, simd => update(simd, out, [a, b], |_, [a, b]| a + b, |_, [a, b]| a + b),
        scalar => scalar_update(out, [a, b], |_, [a, b]| a + b));
}

/// `dst[i] *= src[i]`.
///
/// ```
/// let mut d = [3.0_f32, 4.0];
/// syrp_simd::kernels::mul(syrp_simd::Level::detect(), &mut d, &[2.0, 0.5]);
/// assert_eq!(d, [6.0, 2.0]);
/// ```
#[doc(alias = "FloatVectorOperations::multiply")]
pub fn mul<E: Element>(level: Level, dst: &mut [E], src: &[E]) {
    check_lengths(dst, &[src]);
    dispatch!(level, simd => update(simd, dst, [src], |d, [s]| d * s, |d, [s]| d * s),
        scalar => scalar_update(dst, [src], |d, [s]| d * s));
}

/// `out[i] = a[i] * b[i]`.
///
/// ```
/// let mut o = [0.0_f64; 2];
/// syrp_simd::kernels::mul_into(syrp_simd::Level::detect(), &mut o, &[3.0; 2], &[2.0; 2]);
/// assert_eq!(o, [6.0; 2]);
/// ```
pub fn mul_into<E: Element>(level: Level, out: &mut [E], a: &[E], b: &[E]) {
    check_lengths(out, &[a, b]);
    dispatch!(level, simd => update(simd, out, [a, b], |_, [a, b]| a * b, |_, [a, b]| a * b),
        scalar => scalar_update(out, [a, b], |_, [a, b]| a * b));
}

/// `dst[i] = a[i] * b[i] + dst[i]`, fused (one rounding) on every level.
///
/// ```
/// let mut d = [1.0_f32; 2];
/// syrp_simd::kernels::mul_add(syrp_simd::Level::detect(), &mut d, &[2.0; 2], &[4.0; 2]);
/// assert_eq!(d, [9.0; 2]);
/// ```
#[doc(alias = "FloatVectorOperations::addWithMultiply")]
pub fn mul_add<E: Element>(level: Level, dst: &mut [E], a: &[E], b: &[E]) {
    check_lengths(dst, &[a, b]);
    dispatch!(level, simd => update(simd, dst, [a, b], |d, [a, b]| a.mul_add_precise(b, d),
            |d, [a, b]| Sample::mul_add(a, b, d)),
        scalar => scalar_update(dst, [a, b], |d, [a, b]| Sample::mul_add(a, b, d)));
}

/// `out[i] = a[i] * b[i] + c[i]`, fused (one rounding) on every level.
///
/// ```
/// let mut o = [0.0_f64; 2];
/// syrp_simd::kernels::mul_add_into(syrp_simd::Level::detect(), &mut o, &[2.0; 2], &[3.0; 2], &[1.0; 2]);
/// assert_eq!(o, [7.0; 2]);
/// ```
pub fn mul_add_into<E: Element>(level: Level, out: &mut [E], a: &[E], b: &[E], c: &[E]) {
    check_lengths(out, &[a, b, c]);
    dispatch!(level, simd => update(simd, out, [a, b, c], |_, [a, b, c]| a.mul_add_precise(b, c),
            |_, [a, b, c]| Sample::mul_add(a, b, c)),
        scalar => scalar_update(out, [a, b, c], |_, [a, b, c]| Sample::mul_add(a, b, c)));
}

/// `dst[i] = src[i] * gain`.
///
/// ```
/// let mut d = [0.0_f32; 3];
/// syrp_simd::kernels::copy_with_gain(syrp_simd::Level::detect(), &mut d, &[2.0; 3], 0.25);
/// assert_eq!(d, [0.5; 3]);
/// ```
#[doc(alias = "FloatVectorOperations::copyWithMultiply")]
pub fn copy_with_gain<E: Element>(level: Level, dst: &mut [E], src: &[E], gain: E) {
    check_lengths(dst, &[src]);
    dispatch!(level, simd => update(simd, dst, [src], |_, [s]| s * gain, |_, [s]| s * gain),
        scalar => scalar_update(dst, [src], |_, [s]| s * gain));
}

/// Folds `src` with a lane-wise `vf` and a scalar `sf`, starting from `init`.
#[inline(always)]
fn fold<S: Simd, E: Element>(
    simd: S,
    src: &[E],
    init: E,
    vf: impl Fn(E::Vector<S>, E::Vector<S>) -> E::Vector<S>,
    reduce: impl Fn(E::Vector<S>) -> E,
    sf: impl Fn(E, E) -> E,
) -> E {
    let n = E::Vector::<S>::LEN;
    let mut chunks = src.chunks_exact(n);
    let mut acc = E::Vector::<S>::splat(simd, init);
    for c in &mut chunks {
        acc = vf(acc, E::Vector::<S>::from_slice(simd, c));
    }
    chunks
        .remainder()
        .iter()
        .fold(reduce(acc), |a, &x| sf(a, x))
}

/// The smallest and largest values of `src`, or `None` when it is empty. NaNs are skipped
/// unless every value is NaN.
///
/// ```
/// let r = syrp_simd::kernels::find_min_max(syrp_simd::Level::detect(), &[0.5_f32, -2.0, 4.0]);
/// assert_eq!(r, Some((-2.0, 4.0)));
/// ```
#[doc(alias = "FloatVectorOperations::findMinAndMax")]
pub fn find_min_max<E: Element>(level: Level, src: &[E]) -> Option<(E, E)> {
    let &first = src.first()?;
    let min = dispatch!(level, simd => fold(simd, src, first, |a, b| a.min_precise(b),
            |a| a.reduce_min_precise(), Sample::min),
        scalar => src.iter().fold(first, |a, &x| a.min(x)));
    let max = dispatch!(level, simd => fold(simd, src, first, |a, b| a.max_precise(b),
            |a| a.reduce_max_precise(), Sample::max),
        scalar => src.iter().fold(first, |a, &x| a.max(x)));
    Some((min, max))
}

/// The largest absolute value in `src`, `0.0` when it is empty.
///
/// ```
/// let m = syrp_simd::kernels::abs_max(syrp_simd::Level::detect(), &[0.5_f64, -3.0, 2.0]);
/// assert_eq!(m, 3.0);
/// ```
#[doc(alias = "FloatVectorOperations::findMaximum")]
pub fn abs_max<E: Element>(level: Level, src: &[E]) -> E {
    dispatch!(level, simd => fold(simd, src, E::zero(), |a, b| a.max_precise(b.abs()),
            |a| a.reduce_max_precise(), |a, x| a.max(x.abs())),
        scalar => src.iter().fold(E::zero(), |a, &x| a.max(x.abs())))
}

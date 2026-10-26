// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Vector operations over `f32` and `f64` slices, in the role of JUCE's
//! `FloatVectorOperations` (spec B5).
//!
//! Every operation that has a SIMD path takes the [`Level`] to run at; pass
//! [`Level::detect()`] in normal use. [`Level::Scalar`] runs the scalar reference, which the
//! property tests compare every level against. Where `syrp_simd::kernels` has the operation,
//! it is used; the rest are written once against [`Sample`] lane packs (`F32x8`, `F64x4`) and
//! compiled per level through [`dispatch!`], with `#[inline(always)]` on every function from
//! the `dispatch!` body down (ADR 0003 D5). `clear`, `fill` and `copy` are memory operations
//! with no level.
//!
//! Operations update `dst` in place. They never allocate or lock, so they are safe on the
//! audio thread once [`Level::detect`] has run once. Operations that take two slices panic
//! when the lengths differ.
//!
//! ```
//! use syrp_audio_basics::vector_ops;
//! use syrp_simd::Level;
//! let level = Level::detect();
//! let mut out = [0.5_f32, -2.0, 3.0];
//! vector_ops::multiply_scalar(level, &mut out, 2.0);
//! vector_ops::clip(level, &mut out, -1.0, 1.0);
//! assert_eq!(out, [1.0, -1.0, 1.0]);
//! assert_eq!(vector_ops::find_min_max(level, &out), Some((-1.0, 1.0)));
//! ```

use syrp_simd::{F32x8, Level, Sample, dispatch, kernels};

use crate::BufferSample;

/// `dst[i] = f(dst[i], src[k][i]...)` pack by pack. The last pack is partial: its missing
/// lanes are loaded as zero and never stored.
#[inline(always)]
fn each<E: BufferSample, const K: usize>(
    dst: &mut [E],
    src: [&[E]; K],
    f: impl Fn(E::Pack, [E::Pack; K]) -> E::Pack,
) {
    let n = E::PACK_LANES;
    for (i, d) in dst.chunks_mut(n).enumerate() {
        let r = i * n..i * n + d.len();
        let s = src.map(|s| E::load(&s[r.clone()], E::zero()));
        E::store(f(E::load(d, E::zero()), s), d);
    }
}

/// The scalar reference of [`each`].
#[inline(always)]
fn each_ref<E: BufferSample, const K: usize>(
    dst: &mut [E],
    src: [&[E]; K],
    f: impl Fn(E, [E; K]) -> E,
) {
    for (i, d) in dst.iter_mut().enumerate() {
        *d = f(*d, src.map(|s| s[i]));
    }
}

/// Folds `src` pack by pack with `f`, then its lanes with `g`; `None` when `src` is empty.
/// The last pack's missing lanes repeat the first element, which leaves min and max unchanged.
#[inline(always)]
fn fold<E: BufferSample>(
    src: &[E],
    f: impl Fn(E::Pack, E::Pack) -> E::Pack,
    g: impl Fn(E, E) -> E,
) -> Option<E> {
    let &first = src.first()?;
    let mut acc = E::Pack::splat(first);
    for c in src.chunks(E::PACK_LANES) {
        acc = f(acc, E::load(c, first));
    }
    Some(E::reduce(acc, g))
}

fn check<E>(dst: &[E], src: &[E]) {
    assert_eq!(
        dst.len(),
        src.len(),
        "vector_ops slices must have equal lengths"
    );
}

/// Sets every element to `0.0`.
///
/// ```
/// let mut d = [1.0_f32; 3];
/// syrp_audio_basics::vector_ops::clear(&mut d);
/// assert_eq!(d, [0.0; 3]);
/// ```
#[doc(alias = "FloatVectorOperations::clear")]
pub fn clear<E: BufferSample>(dst: &mut [E]) {
    dst.fill(E::zero());
}

/// Sets every element to `value`.
///
/// ```
/// let mut d = [0.0_f64; 2];
/// syrp_audio_basics::vector_ops::fill(&mut d, 0.5);
/// assert_eq!(d, [0.5; 2]);
/// ```
#[doc(alias = "FloatVectorOperations::fill")]
pub fn fill<E: BufferSample>(dst: &mut [E], value: E) {
    dst.fill(value);
}

/// `dst[i] = src[i]`.
///
/// ```
/// let mut d = [0.0_f32; 2];
/// syrp_audio_basics::vector_ops::copy(&mut d, &[1.0, 2.0]);
/// assert_eq!(d, [1.0, 2.0]);
/// ```
#[doc(alias = "FloatVectorOperations::copy")]
pub fn copy<E: BufferSample>(dst: &mut [E], src: &[E]) {
    dst.copy_from_slice(src);
}

/// `dst[i] = src[i] * gain` (kernel `copy_with_gain`).
///
/// ```
/// let mut d = [0.0_f32; 2];
/// syrp_audio_basics::vector_ops::copy_with_multiply(syrp_simd::Level::detect(), &mut d, &[1.0, 2.0], 0.5);
/// assert_eq!(d, [0.5, 1.0]);
/// ```
#[doc(alias = "FloatVectorOperations::copyWithMultiply")]
pub fn copy_with_multiply<E: BufferSample>(level: Level, dst: &mut [E], src: &[E], gain: E) {
    kernels::copy_with_gain(level, dst, src, gain);
}

/// `dst[i] += src[i] * gain`, rounded after the multiply and after the add.
///
/// ```
/// let mut d = [1.0_f64; 2];
/// syrp_audio_basics::vector_ops::add_with_multiply(syrp_simd::Level::detect(), &mut d, &[2.0, 4.0], 0.5);
/// assert_eq!(d, [2.0, 3.0]);
/// ```
#[doc(alias = "FloatVectorOperations::addWithMultiply")]
pub fn add_with_multiply<E: BufferSample>(level: Level, dst: &mut [E], src: &[E], gain: E) {
    check(dst, src);
    let g = E::Pack::splat(gain);
    dispatch!(level, _s => each(dst, [src], |d, [s]| d + s * g),
        scalar => each_ref(dst, [src], |d, [s]| d + s * gain));
}

/// `dst[i] += src[i]` (kernel `add`).
///
/// ```
/// let mut d = [1.0_f32; 2];
/// syrp_audio_basics::vector_ops::add(syrp_simd::Level::detect(), &mut d, &[0.5, 1.0]);
/// assert_eq!(d, [1.5, 2.0]);
/// ```
#[doc(alias = "FloatVectorOperations::add")]
pub fn add<E: BufferSample>(level: Level, dst: &mut [E], src: &[E]) {
    kernels::add(level, dst, src);
}

/// `dst[i] += value`.
///
/// ```
/// let mut d = [1.0_f32; 2];
/// syrp_audio_basics::vector_ops::add_scalar(syrp_simd::Level::detect(), &mut d, 0.25);
/// assert_eq!(d, [1.25; 2]);
/// ```
#[doc(alias = "FloatVectorOperations::add")]
pub fn add_scalar<E: BufferSample>(level: Level, dst: &mut [E], value: E) {
    let v = E::Pack::splat(value);
    dispatch!(level, _s => each(dst, [], |d, []| d + v),
        scalar => each_ref(dst, [], |d, []| d + value));
}

/// `dst[i] -= src[i]`.
///
/// ```
/// let mut d = [3.0_f64, 1.0];
/// syrp_audio_basics::vector_ops::subtract(syrp_simd::Level::detect(), &mut d, &[1.0, 2.0]);
/// assert_eq!(d, [2.0, -1.0]);
/// ```
#[doc(alias = "FloatVectorOperations::subtract")]
pub fn subtract<E: BufferSample>(level: Level, dst: &mut [E], src: &[E]) {
    check(dst, src);
    dispatch!(level, _s => each(dst, [src], |d, [s]| d - s),
        scalar => each_ref(dst, [src], |d, [s]| d - s));
}

/// `dst[i] *= src[i]` (kernel `mul`).
///
/// ```
/// let mut d = [3.0_f32, 4.0];
/// syrp_audio_basics::vector_ops::multiply(syrp_simd::Level::detect(), &mut d, &[2.0, 0.5]);
/// assert_eq!(d, [6.0, 2.0]);
/// ```
#[doc(alias = "FloatVectorOperations::multiply")]
pub fn multiply<E: BufferSample>(level: Level, dst: &mut [E], src: &[E]) {
    kernels::mul(level, dst, src);
}

/// `dst[i] *= value`.
///
/// ```
/// let mut d = [3.0_f32, 4.0];
/// syrp_audio_basics::vector_ops::multiply_scalar(syrp_simd::Level::detect(), &mut d, 0.5);
/// assert_eq!(d, [1.5, 2.0]);
/// ```
#[doc(alias = "FloatVectorOperations::multiply")]
pub fn multiply_scalar<E: BufferSample>(level: Level, dst: &mut [E], value: E) {
    let v = E::Pack::splat(value);
    dispatch!(level, _s => each(dst, [], |d, []| d * v),
        scalar => each_ref(dst, [], |d, []| d * value));
}

/// `dst[i] = -dst[i]`.
///
/// ```
/// let mut d = [1.0_f64, -2.0];
/// syrp_audio_basics::vector_ops::negate(syrp_simd::Level::detect(), &mut d);
/// assert_eq!(d, [-1.0, 2.0]);
/// ```
#[doc(alias = "FloatVectorOperations::negate")]
pub fn negate<E: BufferSample>(level: Level, dst: &mut [E]) {
    dispatch!(level, _s => each(dst, [], |d, []| -d),
        scalar => each_ref(dst, [], |d, []| -d));
}

/// `dst[i] = |dst[i]|`.
///
/// ```
/// let mut d = [1.0_f32, -2.0];
/// syrp_audio_basics::vector_ops::abs(syrp_simd::Level::detect(), &mut d);
/// assert_eq!(d, [1.0, 2.0]);
/// ```
#[doc(alias = "FloatVectorOperations::abs")]
pub fn abs<E: BufferSample>(level: Level, dst: &mut [E]) {
    dispatch!(level, _s => each(dst, [], |d, []| d.abs()),
        scalar => each_ref(dst, [], |d, []| Sample::abs(d)));
}

/// `dst[i] = min(dst[i], src[i])`. A NaN loses to a number.
///
/// ```
/// let mut d = [1.0_f32, 5.0];
/// syrp_audio_basics::vector_ops::min(syrp_simd::Level::detect(), &mut d, &[2.0, 3.0]);
/// assert_eq!(d, [1.0, 3.0]);
/// ```
#[doc(alias = "FloatVectorOperations::min")]
pub fn min<E: BufferSample>(level: Level, dst: &mut [E], src: &[E]) {
    check(dst, src);
    dispatch!(level, _s => each(dst, [src], |d, [s]| d.min(s)),
        scalar => each_ref(dst, [src], |d, [s]| Sample::min(d, s)));
}

/// `dst[i] = min(dst[i], value)`.
///
/// ```
/// let mut d = [1.0_f64, 5.0];
/// syrp_audio_basics::vector_ops::min_scalar(syrp_simd::Level::detect(), &mut d, 2.0);
/// assert_eq!(d, [1.0, 2.0]);
/// ```
#[doc(alias = "FloatVectorOperations::min")]
pub fn min_scalar<E: BufferSample>(level: Level, dst: &mut [E], value: E) {
    let v = E::Pack::splat(value);
    dispatch!(level, _s => each(dst, [], |d, []| d.min(v)),
        scalar => each_ref(dst, [], |d, []| Sample::min(d, value)));
}

/// `dst[i] = max(dst[i], src[i])`. A NaN loses to a number.
///
/// ```
/// let mut d = [1.0_f32, 5.0];
/// syrp_audio_basics::vector_ops::max(syrp_simd::Level::detect(), &mut d, &[2.0, 3.0]);
/// assert_eq!(d, [2.0, 5.0]);
/// ```
#[doc(alias = "FloatVectorOperations::max")]
pub fn max<E: BufferSample>(level: Level, dst: &mut [E], src: &[E]) {
    check(dst, src);
    dispatch!(level, _s => each(dst, [src], |d, [s]| d.max(s)),
        scalar => each_ref(dst, [src], |d, [s]| Sample::max(d, s)));
}

/// `dst[i] = max(dst[i], value)`.
///
/// ```
/// let mut d = [1.0_f64, 5.0];
/// syrp_audio_basics::vector_ops::max_scalar(syrp_simd::Level::detect(), &mut d, 2.0);
/// assert_eq!(d, [2.0, 5.0]);
/// ```
#[doc(alias = "FloatVectorOperations::max")]
pub fn max_scalar<E: BufferSample>(level: Level, dst: &mut [E], value: E) {
    let v = E::Pack::splat(value);
    dispatch!(level, _s => each(dst, [], |d, []| d.max(v)),
        scalar => each_ref(dst, [], |d, []| Sample::max(d, value)));
}

/// `dst[i] = min(max(dst[i], low), high)`: limits every element to `low..=high`.
///
/// ```
/// let mut d = [-3.0_f32, 0.5, 9.0];
/// syrp_audio_basics::vector_ops::clip(syrp_simd::Level::detect(), &mut d, -1.0, 1.0);
/// assert_eq!(d, [-1.0, 0.5, 1.0]);
/// ```
#[doc(alias = "FloatVectorOperations::clip")]
pub fn clip<E: BufferSample>(level: Level, dst: &mut [E], low: E, high: E) {
    let (l, h) = (E::Pack::splat(low), E::Pack::splat(high));
    dispatch!(level, _s => each(dst, [], |d, []| d.max(l).min(h)),
        scalar => each_ref(dst, [], |d, []| Sample::min(Sample::max(d, low), high)));
}

/// The smallest element, or `None` when `src` is empty. NaNs are skipped unless every element
/// is NaN.
///
/// ```
/// let m = syrp_audio_basics::vector_ops::find_minimum(syrp_simd::Level::detect(), &[2.0_f32, -1.0]);
/// assert_eq!(m, Some(-1.0));
/// ```
#[doc(alias = "FloatVectorOperations::findMinimum")]
pub fn find_minimum<E: BufferSample>(level: Level, src: &[E]) -> Option<E> {
    dispatch!(level, _s => fold(src, |a, b| a.min(b), Sample::min),
        scalar => src.iter().copied().reduce(Sample::min))
}

/// The largest element, or `None` when `src` is empty. NaNs are skipped unless every element
/// is NaN.
///
/// ```
/// let m = syrp_audio_basics::vector_ops::find_maximum(syrp_simd::Level::detect(), &[2.0_f64, -1.0]);
/// assert_eq!(m, Some(2.0));
/// ```
#[doc(alias = "FloatVectorOperations::findMaximum")]
pub fn find_maximum<E: BufferSample>(level: Level, src: &[E]) -> Option<E> {
    dispatch!(level, _s => fold(src, |a, b| a.max(b), Sample::max),
        scalar => src.iter().copied().reduce(Sample::max))
}

/// The smallest and largest elements, or `None` when `src` is empty (kernel `find_min_max`).
///
/// ```
/// let r = syrp_audio_basics::vector_ops::find_min_max(syrp_simd::Level::detect(), &[0.5_f32, -2.0, 4.0]);
/// assert_eq!(r, Some((-2.0, 4.0)));
/// ```
#[doc(alias = "FloatVectorOperations::findMinAndMax")]
pub fn find_min_max<E: BufferSample>(level: Level, src: &[E]) -> Option<(E, E)> {
    kernels::find_min_max(level, src)
}

/// `dst[i] = src[i] as f32 * multiplier`: converts fixed-point integers to floats, for
/// example with `multiplier = 1.0 / 2^31` for full-scale 32-bit samples.
///
/// # Panics
/// When `dst` and `src` have different lengths.
///
/// ```
/// let mut d = [0.0_f32; 2];
/// syrp_audio_basics::vector_ops::convert_fixed_to_float(syrp_simd::Level::detect(), &mut d, &[1 << 30, -(1 << 30)], 1.0 / 2_147_483_648.0);
/// assert_eq!(d, [0.5, -0.5]);
/// ```
#[doc(alias = "FloatVectorOperations::convertFixedToFloat")]
pub fn convert_fixed_to_float(level: Level, dst: &mut [f32], src: &[i32], multiplier: f32) {
    assert_eq!(
        dst.len(),
        src.len(),
        "vector_ops slices must have equal lengths"
    );
    dispatch!(level, _s => fixed_to_float(dst, src, multiplier),
        scalar => dst.iter_mut().zip(src).for_each(|(d, &s)| *d = s as f32 * multiplier));
}

#[inline(always)]
fn fixed_to_float(dst: &mut [f32], src: &[i32], multiplier: f32) {
    let m = F32x8::splat(multiplier);
    for (d, s) in dst.chunks_mut(8).zip(src.chunks(8)) {
        let mut lanes = [0.0_f32; 8];
        lanes.iter_mut().zip(s).for_each(|(l, &x)| *l = x as f32);
        <f32 as crate::sealed::Sealed>::store(F32x8::from_array(lanes) * m, d);
    }
}

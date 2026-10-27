// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Per-channel conversion loops: `syrp_simd` pack paths for little-endian `Int16`, `Int24`,
//! `Int32` and `Float32` to and from `f32` (B4, ADR 0003 D5), and the scalar codec for the
//! rest and for [`Level::Scalar`].
//!
//! Why the pack paths give the scalar codec's bits:
//! - Decoding: the codec computes `n / 2^(bits-1)` exactly in `f64` and rounds once to `f32`.
//!   The pack converts `n` to `f32` (one rounding, only for `Int32`) and multiplies by the
//!   power of two `2^-(bits-1)`, which is exact for these magnitudes, so the result is the
//!   same single rounding. `Float32` is a bit copy on both paths.
//! - Encoding: `x * 2^(bits-1)` is exact in `f32` (or overflows to an infinity that clips to
//!   the same end of the range). Splitting it into `trunc` and the exact remainder and
//!   stepping away from zero when the remainder is at least one half is round half away from
//!   zero. The saturating `as i32` maps NaN to 0 and clips infinities, then a clamp narrows to
//!   the format's range: the codec's rule.
//!
//! Every function from the `dispatch!` body down is `#[inline(always)]` (the `syrp-simd`
//! inlining rule).

use syrp_simd::{F32x8, Level, Sample, dispatch};

use super::SampleFormat;
use super::codec::Codec;
use crate::sealed::Sealed;

const LANES: usize = 8;

// Format tags for the const-generic kernels.
const I16: u8 = 0;
const I24: u8 = 1;
const I32: u8 = 2;
const F32: u8 = 3;

/// The pack format for `codec`, if it has one.
fn pack_format(codec: Codec) -> Option<u8> {
    if codec.is_big_endian() {
        return None;
    }
    match codec.format() {
        SampleFormat::Int16 => Some(I16),
        SampleFormat::Int24 => Some(I24),
        SampleFormat::Int32 => Some(I32),
        SampleFormat::Float32 => Some(F32),
        _ => None,
    }
}

#[inline(always)]
const fn size_of_tag(tag: u8) -> usize {
    match tag {
        I16 => 2,
        I24 => 3,
        _ => 4,
    }
}

#[inline(always)]
const fn bits_of_tag(tag: u8) -> u32 {
    match tag {
        I16 => 16,
        I24 => 24,
        _ => 32,
    }
}

/// The raw little-endian word in `b`: the integer value, or the float's bits.
#[inline(always)]
fn load<const F: u8, const N: usize>(b: &[u8; N]) -> i32 {
    match F {
        I16 => i32::from(i16::from_le_bytes([b[0], b[1]])),
        I24 => i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8,
        _ => i32::from_le_bytes([b[0], b[1], b[2], b[3]]),
    }
}

/// Decodes `n` (at most eight) samples `step` bytes apart from `window`, which ends at the
/// last of them; lanes past `n` are zero.
#[inline(always)]
fn decode_pack<const F: u8, const N: usize>(window: &[u8], step: usize, n: usize) -> F32x8 {
    // Plain loops: `array::from_fn` is not always inlined, which would compile it for the
    // baseline instead of the dispatched level.
    let mut words = [0_i32; LANES];
    for (j, w) in words.iter_mut().enumerate().take(n) {
        let mut b = [0_u8; N];
        for (k, x) in b.iter_mut().enumerate() {
            *x = window[j * step + k];
        }
        *w = load::<F, N>(&b);
    }
    if F == F32 {
        F32x8::from_array(words.map(|w| f32::from_bits(w as u32)))
    } else {
        let scale = 1.0 / (1_u64 << (bits_of_tag(F) - 1)) as f32;
        F32x8::from_array(words.map(|w| w as f32)) * F32x8::splat(scale)
    }
}

#[inline(always)]
fn decode_run<const F: u8, const N: usize>(src: &[u8], step: usize, out: &mut [f32]) {
    let mut chunks = out.chunks_exact_mut(LANES);
    let mut at = 0;
    // One length check per pack: the window ends at the eighth sample.
    let span = (LANES - 1) * step + N;
    for chunk in &mut chunks {
        let lanes = decode_pack::<F, N>(&src[at..at + span], step, LANES).to_array();
        chunk.copy_from_slice(&lanes);
        at += LANES * step;
    }
    let tail = chunks.into_remainder();
    if let Some(last) = tail.len().checked_sub(1) {
        let window = &src[at..at + last * step + N];
        let lanes = decode_pack::<F, N>(window, step, tail.len()).to_array();
        tail.copy_from_slice(&lanes[..=last]);
    }
}

/// The integer words for `x`: scaled, rounded half away from zero, clipped (NaN gives 0).
#[inline(always)]
fn encode_pack<const F: u8>(x: F32x8) -> [i32; LANES] {
    if F == F32 {
        return x.to_array().map(|v| v.to_bits() as i32);
    }
    let bits = bits_of_tag(F);
    let y = x * F32x8::splat((1_u64 << (bits - 1)) as f32);
    let t = F32x8::from_array(y.to_array().map(f32::trunc));
    let away = F32x8::select(y.simd_lt(F32x8::zero()), -F32x8::one(), F32x8::one());
    let half = (y - t).abs().simd_ge(F32x8::splat(0.5));
    let r = t + F32x8::select(half, away, F32x8::zero());
    let (lo, hi) = (-(1_i64 << (bits - 1)), (1_i64 << (bits - 1)) - 1);
    r.to_array().map(|v| (v as i32).clamp(lo as i32, hi as i32))
}

/// Stores the first `n` words, `step` bytes apart, into `window`, which ends at the last.
#[inline(always)]
fn store_pack<const N: usize>(words: &[i32; LANES], window: &mut [u8], step: usize, n: usize) {
    for (j, w) in words.iter().enumerate().take(n) {
        for (k, &b) in w.to_le_bytes().iter().take(N).enumerate() {
            window[j * step + k] = b;
        }
    }
}

#[inline(always)]
fn encode_run<const F: u8, const N: usize>(input: &[f32], dest: &mut [u8], step: usize) {
    let mut chunks = input.chunks_exact(LANES);
    let mut at = 0;
    // One length check per pack: the window ends at the eighth sample.
    let span = (LANES - 1) * step + N;
    for chunk in &mut chunks {
        if let Some(&lanes) = chunk.first_chunk::<LANES>() {
            let words = encode_pack::<F>(F32x8::from_array(lanes));
            store_pack::<N>(&words, &mut dest[at..at + span], step, LANES);
        }
        at += LANES * step;
    }
    let tail = chunks.remainder();
    if let Some(last) = tail.len().checked_sub(1) {
        let mut lanes = [0.0; LANES];
        for (l, &x) in lanes.iter_mut().zip(tail) {
            *l = x;
        }
        let words = encode_pack::<F>(F32x8::from_array(lanes));
        store_pack::<N>(
            &words,
            &mut dest[at..at + last * step + N],
            step,
            tail.len(),
        );
    }
}

/// Picks the kernel for `tag`. Planar data (`step` equal to the sample size) gets its own
/// copy with a constant step.
#[inline(always)]
fn decode_tag(tag: u8, src: &[u8], step: usize, out: &mut [f32]) {
    match (tag, step == size_of_tag(tag)) {
        (I16, true) => decode_run::<I16, 2>(src, 2, out),
        (I16, false) => decode_run::<I16, 2>(src, step, out),
        (I24, true) => decode_run::<I24, 3>(src, 3, out),
        (I24, false) => decode_run::<I24, 3>(src, step, out),
        (I32, true) => decode_run::<I32, 4>(src, 4, out),
        (I32, false) => decode_run::<I32, 4>(src, step, out),
        (_, true) => decode_run::<F32, 4>(src, 4, out),
        (_, false) => decode_run::<F32, 4>(src, step, out),
    }
}

/// As [`decode_tag`], for encoding.
#[inline(always)]
fn encode_tag(tag: u8, input: &[f32], dest: &mut [u8], step: usize) {
    match (tag, step == size_of_tag(tag)) {
        (I16, true) => encode_run::<I16, 2>(input, dest, 2),
        (I16, false) => encode_run::<I16, 2>(input, dest, step),
        (I24, true) => encode_run::<I24, 3>(input, dest, 3),
        (I24, false) => encode_run::<I24, 3>(input, dest, step),
        (I32, true) => encode_run::<I32, 4>(input, dest, 4),
        (I32, false) => encode_run::<I32, 4>(input, dest, step),
        (_, true) => encode_run::<F32, 4>(input, dest, 4),
        (_, false) => encode_run::<F32, 4>(input, dest, step),
    }
}

/// Decodes `out.len()` samples, the first at `src[0]` and each `step` bytes after the last.
pub(super) fn decode<T: Sealed>(
    level: Level,
    codec: Codec,
    src: &[u8],
    step: usize,
    out: &mut [T],
) {
    if let (Some(tag), Some(out)) = (pack_format(codec), T::as_f32_mut(&mut *out)) {
        dispatch!(level, _simd => decode_tag(tag, src, step, out),
            scalar => scalar_decode(codec, src, step, out));
        return;
    }
    scalar_decode(codec, src, step, out);
}

/// Encodes `input`, the first sample at `dest[0]` and each `step` bytes after the last.
pub(super) fn encode<T: Sealed>(
    level: Level,
    codec: Codec,
    input: &[T],
    dest: &mut [u8],
    step: usize,
) {
    if let (Some(tag), Some(input)) = (pack_format(codec), T::as_f32(input)) {
        dispatch!(level, _simd => encode_tag(tag, input, dest, step),
            scalar => scalar_encode(codec, input, dest, step));
        return;
    }
    scalar_encode(codec, input, dest, step);
}

/// The scalar reference loop.
#[inline(always)]
fn scalar_decode<T: Sealed>(codec: Codec, src: &[u8], step: usize, out: &mut [T]) {
    for (i, o) in out.iter_mut().enumerate() {
        *o = codec.decode(&src[i * step..]);
    }
}

/// The scalar reference loop.
#[inline(always)]
fn scalar_encode<T: Sealed>(codec: Codec, input: &[T], dest: &mut [u8], step: usize) {
    for (i, &x) in input.iter().enumerate() {
        codec.encode(x, &mut dest[i * step..]);
    }
}

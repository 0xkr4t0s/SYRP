// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The scalar codec for one packed sample: the reference that every faster path must match
//! bit for bit.

use super::{Endianness, SampleFormat};
use crate::sealed::Sealed;

/// One format and resolved byte order.
#[derive(Clone, Copy)]
pub(super) struct Codec {
    format: SampleFormat,
    big: bool,
}

/// The first `N` bytes of `bytes`, in little-endian order.
#[inline(always)]
fn le<const N: usize>(bytes: &[u8], big: bool) -> [u8; N] {
    let mut a = [0_u8; N];
    a.copy_from_slice(&bytes[..N]);
    if big {
        a.reverse();
    }
    a
}

/// Stores little-endian `a` into the first `N` bytes of `out` in the codec's byte order.
#[inline(always)]
fn put<const N: usize>(out: &mut [u8], mut a: [u8; N], big: bool) {
    if big {
        a.reverse();
    }
    out[..N].copy_from_slice(&a);
}

/// `n / 2^(bits - 1)`, rounded once to `T`. Exact in `f64` (at most 32 significant bits and a
/// power-of-two scale), so `from_f64` is the only rounding.
#[inline(always)]
fn from_int<T: Sealed>(n: i32, bits: u32) -> T {
    T::from_f64(f64::from(n) / f64::from(1_u32 << (bits - 1)))
}

/// `x * 2^(bits - 1)` rounded to nearest with ties away from zero, clipped to the signed
/// `bits`-bit range; NaN gives 0. The product is exact in `f64` (power-of-two scale, and the
/// clip happens long before it could overflow to anything but infinity).
#[inline(always)]
pub(super) fn quantise(x: f64, bits: u32) -> i32 {
    if x.is_nan() {
        return 0;
    }
    let scale = f64::from(1_u32 << (bits - 1));
    // `f64::round` rounds half away from zero. The clipped value fits in i32, so `as` is exact.
    (x * scale).round().clamp(-scale, scale - 1.0) as i32
}

impl Codec {
    pub(super) fn new(format: SampleFormat, endianness: Endianness) -> Self {
        Codec {
            format,
            big: endianness.resolve() == Endianness::Big,
        }
    }

    /// Decodes the sample at the start of `bytes` (which holds at least one sample).
    #[inline(always)]
    pub(super) fn decode<T: Sealed>(self, bytes: &[u8]) -> T {
        let big = self.big;
        match self.format {
            SampleFormat::Int8 => from_int(i32::from(bytes[0] as i8), 8),
            SampleFormat::UInt8 => from_int(i32::from(bytes[0]) - 128, 8),
            SampleFormat::Int16 => from_int(i32::from(i16::from_le_bytes(le(bytes, big))), 16),
            SampleFormat::Int24 => {
                let [b0, b1, b2] = le(bytes, big);
                // Place the 24 bits at the top of an i32, then shift back with sign.
                from_int(i32::from_le_bytes([0, b0, b1, b2]) >> 8, 24)
            }
            SampleFormat::Int24In32 => from_int((i32::from_le_bytes(le(bytes, big)) << 8) >> 8, 24),
            SampleFormat::Int32 => from_int(i32::from_le_bytes(le(bytes, big)), 32),
            SampleFormat::Float32 => T::from_f32(f32::from_le_bytes(le(bytes, big))),
            SampleFormat::Float64 => T::from_f64(f64::from_le_bytes(le(bytes, big))),
        }
    }

    /// Encodes `x` into the start of `out` (which has room for at least one sample).
    #[inline(always)]
    pub(super) fn encode<T: Sealed>(self, x: T, out: &mut [u8]) {
        let big = self.big;
        match self.format {
            SampleFormat::Int8 => out[0] = quantise(x.to_f64(), 8) as i8 as u8,
            SampleFormat::UInt8 => out[0] = (quantise(x.to_f64(), 8) + 128) as u8,
            SampleFormat::Int16 => {
                put(out, (quantise(x.to_f64(), 16) as i16).to_le_bytes(), big);
            }
            SampleFormat::Int24 => {
                let [b0, b1, b2, _] = quantise(x.to_f64(), 24).to_le_bytes();
                put(out, [b0, b1, b2], big);
            }
            // The top byte is the sign extension of the 24-bit value.
            SampleFormat::Int24In32 => put(out, quantise(x.to_f64(), 24).to_le_bytes(), big),
            SampleFormat::Int32 => put(out, quantise(x.to_f64(), 32).to_le_bytes(), big),
            SampleFormat::Float32 => put(out, x.to_f32().to_le_bytes(), big),
            SampleFormat::Float64 => put(out, x.to_f64().to_le_bytes(), big),
        }
    }
}

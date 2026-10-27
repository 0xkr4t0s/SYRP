// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The 80-bit IEEE 754 extended value AIFF uses for its sample rate.
//!
//! Layout (big-endian, 10 bytes): one sign bit, a 15-bit exponent biased by 16,383, and a
//! 64-bit significand whose top bit is the explicit integer bit. Written from the IEEE 754
//! definition of the format and the AIFF 1.3 specification only.

use syrp_format_api::FormatError;

const BIAS: i32 = 16_383;
const EXP_MAX: u16 = 0x7FFF;

/// Converts a big-endian 80-bit extended value to the nearest `f64` (ties to even), which is
/// exact whenever the value is representable in `f64`.
///
/// Returns `SYRP-FORMAT-0002` for infinities and NaNs, for non-canonical encodings (a non-zero
/// exponent without the integer bit, or a zero exponent with it), and for finite values beyond
/// `f64::MAX`. Values below the smallest `f64` subnormal round to zero.
///
/// ```
/// use syrp_format_aiff::extended_to_f64;
/// let rate = [0x40, 0x0E, 0xAC, 0x44, 0, 0, 0, 0, 0, 0];
/// assert_eq!(extended_to_f64(rate)?, 44_100.0);
/// assert!(extended_to_f64([0x7F, 0xFF, 0x80, 0, 0, 0, 0, 0, 0, 0]).is_err());
/// # Ok::<(), syrp_format_api::FormatError>(())
/// ```
pub fn extended_to_f64(bytes: [u8; 10]) -> Result<f64, FormatError> {
    let head = u16::from_be_bytes([bytes[0], bytes[1]]);
    let negative = head & 0x8000 != 0;
    let exp = head & EXP_MAX;
    let mut sig = u64::from_be_bytes([
        bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7], bytes[8], bytes[9],
    ]);
    let integer_bit = sig >> 63 == 1;
    if exp == EXP_MAX {
        return Err(FormatError::invalid_data(
            "extended value is an infinity or NaN",
        ));
    }
    if (exp != 0) != integer_bit {
        return Err(FormatError::invalid_data(
            "extended value is not canonically encoded",
        ));
    }
    let sign = if negative { 1u64 << 63 } else { 0 };
    if sig == 0 {
        return Ok(f64::from_bits(sign));
    }
    // value = sig * 2^scale, then normalise so the top bit of `sig` is set.
    let mut scale = i32::from(exp.max(1)) - BIAS - 63;
    let lz = sig.leading_zeros();
    sig <<= lz;
    scale -= lz.cast_signed();
    // Unbiased binary exponent of the value: it lies in [2^e, 2^(e+1)).
    let e = scale + 63;
    if e > 1023 {
        return Err(FormatError::invalid_data(
            "extended value exceeds the f64 range",
        ));
    }
    // Significand bits kept: 53 for normal results, fewer for subnormal ones.
    let keep = if e >= -1022 { 53 } else { e + 1074 + 1 };
    if keep <= 0 {
        // Below half the smallest subnormal (keep == 0 means at most half, and only an exact
        // half with no remainder ties to zero, which is even).
        let rounds_up = keep == 0 && sig > 1 << 63;
        return Ok(f64::from_bits(sign | u64::from(rounds_up)));
    }
    let drop = 64 - keep.cast_unsigned();
    let mut kept = sig >> drop;
    let rest = sig & ((1u64 << drop) - 1);
    let half = 1u64 << (drop - 1);
    if rest > half || (rest == half && kept & 1 == 1) {
        kept += 1;
    }
    let bits = if e >= -1022 {
        // `kept` is in [2^52, 2^53]; a carry to 2^53 moves to the next binade.
        let (kept, e) = if kept >> 53 == 1 {
            (kept >> 1, e + 1)
        } else {
            (kept, e)
        };
        if e > 1023 {
            return Err(FormatError::invalid_data(
                "extended value exceeds the f64 range",
            ));
        }
        (u64::from((e + 1023).cast_unsigned()) << 52) | (kept & ((1u64 << 52) - 1))
    } else {
        // Subnormal: the stored fraction is `kept`; a carry into bit 52 is the smallest
        // normal number, which the same bit pattern already encodes.
        kept
    };
    Ok(f64::from_bits(sign | bits))
}

/// Converts a finite `f64` to its big-endian 80-bit extended encoding, which is always exact.
///
/// Returns `SYRP-FORMAT-0003` for infinities and NaNs, which a sample rate can never be.
///
/// ```
/// use syrp_format_aiff::{extended_to_f64, f64_to_extended};
/// let bytes = f64_to_extended(48_000.0)?;
/// assert_eq!(bytes, [0x40, 0x0E, 0xBB, 0x80, 0, 0, 0, 0, 0, 0]);
/// assert_eq!(extended_to_f64(bytes)?, 48_000.0);
/// # Ok::<(), syrp_format_api::FormatError>(())
/// ```
pub fn f64_to_extended(value: f64) -> Result<[u8; 10], FormatError> {
    if !value.is_finite() {
        return Err(FormatError::unsupported(
            "an AIFF extended value must be finite",
        ));
    }
    let bits = value.to_bits();
    let sign: u16 = if bits >> 63 == 1 { 0x8000 } else { 0 };
    let field = i32::from(u16::try_from((bits >> 52) & 0x7FF).unwrap_or(0));
    let fraction = bits & ((1u64 << 52) - 1);
    let (mut sig, mut scale) = if field == 0 {
        (fraction, -1074)
    } else {
        (fraction | (1u64 << 52), field - 1075)
    };
    let mut out = [0u8; 10];
    if sig == 0 {
        out[..2].copy_from_slice(&sign.to_be_bytes());
        return Ok(out);
    }
    let lz = sig.leading_zeros();
    sig <<= lz;
    scale -= lz.cast_signed();
    // f64 exponents span -1074..=1023, well inside the extended range, so this never clamps.
    let exp = u16::try_from(scale + 63 + BIAS).unwrap_or(0);
    out[..2].copy_from_slice(&(sign | exp).to_be_bytes());
    out[2..].copy_from_slice(&sig.to_be_bytes());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use syrp_error::Diagnostic;

    fn ext(exp: u16, sig: u64) -> [u8; 10] {
        let mut b = [0u8; 10];
        b[..2].copy_from_slice(&exp.to_be_bytes());
        b[2..].copy_from_slice(&sig.to_be_bytes());
        b
    }

    /// The f64 an extended value converts to.
    fn x(exp: u16, sig: u64) -> f64 {
        extended_to_f64(ext(exp, sig)).unwrap()
    }

    fn exp(e: i32) -> u16 {
        u16::try_from(BIAS + e).unwrap()
    }

    #[test]
    fn b2_extended_common_rates_hand_built() {
        // 44,100 = 0xAC44 has 16 significant bits, so the exponent is 15 + 16,383 = 0x400E.
        assert_eq!(x(0x400E, 0xAC44 << 48), 44_100.0);
        assert_eq!(x(0x400E, 0xBB80 << 48), 48_000.0);
        assert_eq!(x(0x3FFF, 1 << 63), 1.0);
        assert_eq!(x(0xBFFF, 3 << 62), -1.5);
        for rate in [8_000.0, 22_050.0, 88_200.0, 96_000.0, 192_000.0, 0.5] {
            assert_eq!(
                extended_to_f64(f64_to_extended(rate).unwrap()).unwrap(),
                rate
            );
        }
        assert_eq!(
            f64_to_extended(44_100.0).unwrap(),
            ext(0x400E, 0xAC44 << 48)
        );
    }

    #[test]
    fn b2_extended_zero_and_signed_zero() {
        assert_eq!(x(0, 0).to_bits(), 0);
        assert_eq!(x(0x8000, 0).to_bits(), (-0.0f64).to_bits());
        assert_eq!(f64_to_extended(0.0).unwrap(), [0; 10]);
        assert_eq!(f64_to_extended(-0.0).unwrap(), ext(0x8000, 0));
    }

    #[test]
    fn b2_extended_f64_edges_round_trip_exactly() {
        let subnormal_max = f64::from_bits(0x000F_FFFF_FFFF_FFFF);
        let edges = [
            f64::MAX,
            f64::MIN_POSITIVE,
            f64::from_bits(1),
            subnormal_max,
        ];
        for v in edges
            .into_iter()
            .chain([-f64::from_bits(12_345), 1.0 + f64::EPSILON])
        {
            let back = extended_to_f64(f64_to_extended(v).unwrap()).unwrap();
            assert_eq!(back.to_bits(), v.to_bits(), "{v:e}");
        }
        // The smallest f64 subnormal, 2^-1074, is a normal extended value.
        assert_eq!(
            f64_to_extended(f64::from_bits(1)).unwrap(),
            ext(exp(-1074), 1 << 63)
        );
    }

    #[test]
    fn b2_extended_denormals_and_underflow() {
        // Extended denormals (exponent 0, integer bit clear) are far below f64's range.
        assert_eq!(x(0, 1), 0.0);
        assert_eq!(x(0, (1 << 63) - 1), 0.0);
        // Exactly half of 2^-1074 ties to even (zero); just above half rounds up.
        assert_eq!(x(exp(-1075), 1 << 63), 0.0);
        assert_eq!(x(exp(-1075), (1 << 63) | 1), f64::from_bits(1));
        // 1.5 * 2^-1074 ties to even (2 * 2^-1074).
        assert_eq!(x(exp(-1074), 3 << 62), f64::from_bits(2));
    }

    #[test]
    fn b2_extended_rounds_to_nearest_even() {
        // 1 + 2^-53 is a tie between 1 and 1 + 2^-52: it goes to the even one, 1.
        assert_eq!(x(0x3FFF, (1 << 63) | (1 << 10)), 1.0);
        // 1 + 3 * 2^-53 ties up to 1 + 2^-51.
        assert_eq!(x(0x3FFF, (1 << 63) | (3 << 10)), 1.0 + 2.0 * f64::EPSILON);
        // All ones rounds up across the binade to 2.0.
        assert_eq!(x(0x3FFF, u64::MAX), 2.0);
    }

    #[test]
    fn b2_extended_rejects_impossible_values() {
        let impossible = [
            (0x7FFF, 1 << 63),      // infinity
            (0xFFFF, 3 << 62),      // NaN
            (0x7FFF, 0),            // pseudo-infinity
            (0x400E, 0xAC44 << 47), // unnormal: no integer bit
            (0, 1 << 63),           // pseudo-denormal: integer bit with exponent 0
            (0x7FFE, 1 << 63),      // far beyond f64::MAX
            (exp(1023), u64::MAX),  // beyond f64::MAX only after rounding
        ];
        for (e, sig) in impossible {
            let err = extended_to_f64(ext(e, sig)).unwrap_err();
            assert_eq!(err.code().as_str(), "SYRP-FORMAT-0002", "{e:#x} {sig:#x}");
        }
        assert_eq!(x(exp(1023), u64::MAX << 11), f64::MAX);
        for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let err = f64_to_extended(v).unwrap_err();
            assert_eq!(err.code().as_str(), "SYRP-FORMAT-0003");
        }
    }
}

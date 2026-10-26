//! Base64 with the standard alphabet and `=` padding (RFC 4648 §4), JUCE's `Base64`.
//!
//! Encoding always pads to a multiple of four characters. Decoding is strict: it accepts exactly
//! what [`encode`] produces and rejects everything else with a coded [`StringsError`]:
//! characters outside the alphabet, whitespace and line breaks included (`SYRP-STRINGS-0001`),
//! and a length that is not a multiple of four, `=` anywhere but the last one or two places, or
//! non-zero unused bits in the last group (`SYRP-STRINGS-0002`).
//!
//! ```
//! use syrp_strings::base64;
//! let text = base64::encode(b"foobar");
//! assert_eq!(text, "Zm9vYmFy");
//! assert_eq!(base64::decode(&text).unwrap(), b"foobar");
//! ```

use std::fmt;

use crate::{StringsError, StringsErrorKind};

/// The 64 symbols, in value order.
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Marks a byte that is not in [`ALPHABET`] in [`DECODE`].
const INVALID: u8 = 0xFF;

/// Maps an input byte to its 6-bit value, or [`INVALID`].
const DECODE: [u8; 256] = {
    let mut table = [INVALID; 256];
    let mut i = 0;
    while i < 64 {
        table[ALPHABET[i] as usize] = i as u8;
        i += 1;
    }
    table
};

/// Encodes `bytes` as padded Base64 text.
///
/// The output is `4 * ceil(len / 3)` ASCII characters. Use [`encode_to`] to append to an
/// existing buffer instead.
///
/// ```
/// assert_eq!(syrp_strings::base64::encode(b"fo"), "Zm8=");
/// assert_eq!(syrp_strings::base64::encode(b""), "");
/// ```
#[doc(alias = "toBase64")]
pub fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    // Writing into a `String` cannot fail.
    let _ = encode_to(bytes, &mut out);
    out
}

/// Appends the padded Base64 encoding of `bytes` to `out`.
///
/// Returns the first error `out` reports; nothing after it is written.
///
/// ```
/// let mut s = String::from("data:");
/// syrp_strings::base64::encode_to(b"f", &mut s).unwrap();
/// assert_eq!(s, "data:Zg==");
/// ```
#[doc(alias = "convertToBase64")]
pub fn encode_to(bytes: &[u8], out: &mut impl fmt::Write) -> fmt::Result {
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        let group = [
            ALPHABET[usize::from(b0 >> 2)],
            ALPHABET[usize::from(((b0 & 0x03) << 4) | (b1 >> 4))],
            if chunk.len() > 1 {
                ALPHABET[usize::from(((b1 & 0x0F) << 2) | (b2 >> 6))]
            } else {
                b'='
            },
            if chunk.len() > 2 {
                ALPHABET[usize::from(b2 & 0x3F)]
            } else {
                b'='
            },
        ];
        // Every byte of `group` is ASCII, so this never fails.
        out.write_str(std::str::from_utf8(&group).map_err(|_| fmt::Error)?)?;
    }
    Ok(())
}

/// Decodes padded Base64 `text` back into bytes.
///
/// # Errors
/// - `SYRP-STRINGS-0001` ([`StringsErrorKind::Base64Character`]) for a character outside the
///   alphabet; [`StringsError::position`] is its byte offset.
/// - `SYRP-STRINGS-0002` ([`StringsErrorKind::Base64Padding`]) for a length that is not a
///   multiple of four, misplaced or excess `=`, or non-zero unused bits in the last group.
/// - `SYRP-STRINGS-0003` ([`StringsErrorKind::OutOfMemory`]) if the output cannot be allocated.
///
/// ```
/// use syrp_strings::base64;
/// assert_eq!(base64::decode("Zm9vYg==").unwrap(), b"foob");
/// assert!(base64::decode("Zm9vYg").is_err()); // padding is required
/// assert!(base64::decode("Zm9vYh==").is_err()); // unused bits must be zero
/// ```
#[doc(alias = "convertFromBase64")]
pub fn decode(text: &str) -> Result<Vec<u8>, StringsError> {
    let input = text.as_bytes();
    let pad = input.iter().rev().take_while(|&&b| b == b'=').count();
    let data_len = input.len() - pad;
    for (i, &b) in input[..data_len].iter().enumerate() {
        if b == b'=' {
            return Err(padding_error(Some(i)));
        }
        if DECODE[usize::from(b)] == INVALID {
            return Err(StringsError::new(
                StringsErrorKind::Base64Character,
                Some(i),
            ));
        }
    }
    if !input.len().is_multiple_of(4) {
        return Err(padding_error(None));
    }
    if pad > 2 {
        return Err(padding_error(Some(data_len)));
    }

    let mut out = Vec::new();
    out.try_reserve_exact(input.len() / 4 * 3 - pad)
        .map_err(|_| StringsError::new(StringsErrorKind::OutOfMemory, None))?;
    for (g, group) in input.chunks_exact(4).enumerate() {
        let v = |i: usize| u32::from(DECODE[usize::from(group[i])]);
        let last = g * 4 + 4 == input.len();
        let used = if last { 4 - pad } else { 4 };
        let mut bits = 0u32;
        for i in 0..used {
            bits |= v(i) << (18 - 6 * i);
        }
        let [_, b0, b1, b2] = bits.to_be_bytes();
        match used {
            4 => out.extend_from_slice(&[b0, b1, b2]),
            3 if b2 == 0 => out.extend_from_slice(&[b0, b1]),
            2 if b1 == 0 && b2 == 0 => out.push(b0),
            _ => return Err(padding_error(Some(g * 4 + used - 1))),
        }
    }
    Ok(out)
}

fn padding_error(position: Option<usize>) -> StringsError {
    StringsError::new(StringsErrorKind::Base64Padding, position)
}

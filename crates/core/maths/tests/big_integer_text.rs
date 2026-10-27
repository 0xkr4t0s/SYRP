//! B4: `BigInteger` text and byte forms.

use proptest::prelude::*;
use syrp_error::Diagnostic;
use syrp_maths::{BigInteger, MathsError};

fn big(v: i128) -> BigInteger {
    BigInteger::from(v)
}

fn large() -> impl Strategy<Value = BigInteger> {
    (prop::collection::vec(any::<u8>(), 0..200), any::<bool>()).prop_map(|(b, neg)| {
        let mut v = BigInteger::from_le_bytes(&b);
        v.set_negative(neg);
        v
    })
}

#[test]
fn b4_to_string_radix_in_every_base() {
    let v = big(-255);
    assert_eq!(v.to_string_radix(2, 0).unwrap(), "-11111111");
    assert_eq!(v.to_string_radix(8, 0).unwrap(), "-377");
    assert_eq!(v.to_string_radix(10, 0).unwrap(), "-255");
    assert_eq!(v.to_string_radix(16, 0).unwrap(), "-ff");
    assert_eq!(big(0).to_string_radix(10, 0).unwrap(), "0");
    assert_eq!(big(0).to_string_radix(16, 3).unwrap(), "000");
    assert_eq!(big(42).to_string_radix(10, 1).unwrap(), "42");
    assert_eq!(big(-42).to_string_radix(10, 5).unwrap(), "-00042");
    // Spans several 10^19 chunks, so inner chunks keep their leading zeros.
    let p = BigInteger::from(10_u64.pow(19)) * BigInteger::from(10_u64.pow(19));
    assert_eq!(p.to_string(), format!("1{}", "0".repeat(38)));
}

#[test]
fn b4_parse_reads_every_base_and_sign() {
    assert_eq!(BigInteger::parse("-11111111", 2), Ok(big(-255)));
    assert_eq!(BigInteger::parse("377", 8), Ok(big(255)));
    assert_eq!(BigInteger::parse("00255", 10), Ok(big(255)));
    assert_eq!(BigInteger::parse("fF", 16), Ok(big(255)));
    let zero = BigInteger::parse("-0", 10).unwrap();
    assert!(zero.is_zero() && !zero.is_negative());
    let text = format!("1{}", "0".repeat(100));
    let v: BigInteger = text.parse().unwrap();
    assert_eq!(v.to_string(), text);
}

#[test]
fn b4_parse_rejects_bad_base_and_digits_with_coded_error() {
    for (text, base) in [
        ("12", 3),
        ("12", 36),
        ("", 10),
        ("-", 10),
        ("12a", 10),
        ("8", 8),
        ("2", 2),
        ("g", 16),
        (" 1", 10),
        ("+1", 10),
        ("1_0", 10),
        ("--1", 10),
        ("١", 10),
    ] {
        let err = BigInteger::parse(text, base).unwrap_err();
        assert_eq!(err, MathsError::InvalidBaseOrDigit, "{text:?} base {base}");
        assert_eq!(err.code().as_str(), "SYRP-MATHS-0002");
    }
    assert_eq!(
        big(1).to_string_radix(7, 0),
        Err(MathsError::InvalidBaseOrDigit)
    );
}

#[test]
fn b4_display_and_hex_formatting() {
    let v = big(-0xABCDEF);
    assert_eq!(v.to_string(), "-11259375");
    assert_eq!(format!("{v:x}"), "-abcdef");
    assert_eq!(format!("{v:X}"), "-ABCDEF");
    assert_eq!(format!("{v:#x}"), "-0xabcdef");
    assert_eq!(format!("{:>6}", big(42)), "    42");
    assert_eq!(format!("{:+}", big(42)), "+42");
    assert_eq!(format!("{:05}", big(-42)), "-0042");
}

#[test]
fn b4_le_bytes_hold_the_magnitude_only() {
    let v = BigInteger::from(-0x0102_0304_0506_0708_090A_i128);
    assert_eq!(v.to_le_bytes(), [10, 9, 8, 7, 6, 5, 4, 3, 2, 1]);
    assert_eq!(BigInteger::from_le_bytes(&v.to_le_bytes()), v.abs());
    assert!(BigInteger::default().to_le_bytes().is_empty());
    assert!(BigInteger::from_le_bytes(&[]).is_zero());
    assert_eq!(BigInteger::from_le_bytes(&[0, 0, 0]), BigInteger::default());
    assert_eq!(
        BigInteger::from_le_bytes(&[1, 0, 0, 0, 0, 0, 0, 0, 0]),
        big(1)
    );
}

proptest! {
    #[test]
    fn b4_text_matches_i128(v in any::<i128>(), pad in 0_usize..50) {
        let n = big(v);
        prop_assert_eq!(n.to_string(), v.to_string());
        prop_assert_eq!(format!("{n:x}"), if v < 0 { format!("-{:x}", v.unsigned_abs()) } else { format!("{v:x}") });
        let sign = if v < 0 { "-" } else { "" };
        let m = v.unsigned_abs();
        prop_assert_eq!(n.to_string_radix(2, pad).unwrap(), format!("{sign}{m:0pad$b}"));
        prop_assert_eq!(n.to_string_radix(8, pad).unwrap(), format!("{sign}{m:0pad$o}"));
        prop_assert_eq!(n.to_string_radix(10, pad).unwrap(), format!("{sign}{m:0pad$}"));
        prop_assert_eq!(BigInteger::parse(&v.to_string(), 10), Ok(n.clone()));
        let le = m.to_le_bytes();
        let used = 16 - (m.leading_zeros() / 8) as usize;
        prop_assert_eq!(n.to_le_bytes(), le[..used].to_vec());
    }

    #[test]
    fn b4_parse_of_to_string_radix_is_identity(v in large(), pad in 0_usize..300) {
        for base in [2, 8, 10, 16] {
            let text = v.to_string_radix(base, pad).unwrap();
            prop_assert_eq!(BigInteger::parse(&text, base), Ok(v.clone()));
            prop_assert_eq!(BigInteger::parse(&text.to_uppercase(), base), Ok(v.clone()));
        }
    }

    #[test]
    fn b4_le_bytes_round_trip_to_abs(v in large(), extra in 0_usize..9) {
        let mut bytes = v.to_le_bytes();
        prop_assert!(bytes.last() != Some(&0));
        prop_assert_eq!(BigInteger::from_le_bytes(&bytes), v.abs());
        bytes.resize(bytes.len() + extra, 0);
        prop_assert_eq!(BigInteger::from_le_bytes(&bytes), v.abs());
    }
}

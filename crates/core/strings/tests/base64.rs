//! B1: Base64 encode and decode (RFC 4648 §4).

use proptest::prelude::*;
use syrp_error::Diagnostic;
use syrp_strings::{StringsErrorKind, base64};

/// RFC 4648 §10 test vectors.
const RFC_VECTORS: [(&str, &str); 7] = [
    ("", ""),
    ("f", "Zg=="),
    ("fo", "Zm8="),
    ("foo", "Zm9v"),
    ("foob", "Zm9vYg=="),
    ("fooba", "Zm9vYmE="),
    ("foobar", "Zm9vYmFy"),
];

#[test]
fn b1_rfc4648_vectors_encode_and_decode() {
    for (plain, encoded) in RFC_VECTORS {
        assert_eq!(base64::encode(plain.as_bytes()), encoded);
        assert_eq!(base64::decode(encoded).unwrap(), plain.as_bytes());
        let mut out = String::new();
        base64::encode_to(plain.as_bytes(), &mut out).unwrap();
        assert_eq!(out, encoded);
    }
}

#[test]
fn b1_full_alphabet_round_trips() {
    let all: Vec<u8> = (0..=255).collect();
    let text = base64::encode(&all);
    assert!(text.contains('+') && text.contains('/'));
    assert_eq!(base64::decode(&text).unwrap(), all);
}

#[test]
fn b1_rejects_characters_outside_the_alphabet() {
    for (text, at) in [
        ("Zm9v!mFy", 4),
        ("Zm9v\nYmFy", 4),
        ("Zm-v", 2),
        ("Zm_v", 2),
        ("Zé==", 1),
    ] {
        let err = base64::decode(text).unwrap_err();
        assert_eq!(err.kind(), StringsErrorKind::Base64Character, "{text:?}");
        assert_eq!(err.code().as_str(), "SYRP-STRINGS-0001");
        assert_eq!(err.position(), Some(at), "{text:?}");
    }
}

#[test]
fn b1_rejects_bad_padding() {
    for text in [
        "Zg", "Zg=", "Zm9", "Z===", "====", "Zg==Zg==", "Z=g=", "Zh==", "Zm9=", "Zm8=Zm8=",
    ] {
        let err = base64::decode(text).unwrap_err();
        assert_eq!(err.kind(), StringsErrorKind::Base64Padding, "{text:?}");
        assert_eq!(err.code().as_str(), "SYRP-STRINGS-0002");
    }
}

#[test]
fn b1_error_display_names_the_position() {
    assert_eq!(
        base64::decode("ab!d").unwrap_err().to_string(),
        "character outside the Base64 alphabet at byte 2"
    );
}

proptest! {
    #[test]
    fn b1_decode_inverts_encode(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
        let text = base64::encode(&bytes);
        prop_assert_eq!(text.len(), bytes.len().div_ceil(3) * 4);
        prop_assert_eq!(base64::decode(&text).unwrap(), bytes);
    }

    #[test]
    fn b1_decode_never_panics(text in "[A-Za-z0-9+/=!\\s]{0,40}") {
        if let Ok(bytes) = base64::decode(&text) {
            prop_assert_eq!(base64::encode(&bytes), text);
        }
    }
}

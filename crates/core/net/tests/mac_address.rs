// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B2: `MacAddress` text forms and enumeration.

use std::collections::HashSet;

use proptest::prelude::*;
use syrp_net::{Diagnostic, MacAddress};

const SAMPLE: MacAddress = MacAddress::new([0x00, 0x1a, 0x2b, 0xc3, 0xd4, 0xef]);

#[test]
fn b2_to_string_default_separator_is_dash() {
    assert_eq!(SAMPLE.to_string(), "00-1a-2b-c3-d4-ef");
    assert_eq!(SAMPLE.to_string_with("-"), SAMPLE.to_string());
    assert_eq!(SAMPLE.to_string_with(":"), "00:1a:2b:c3:d4:ef");
    assert_eq!(SAMPLE.to_string_with(""), "001a2bc3d4ef");
    assert_eq!(SAMPLE.to_string_with(" : "), "00 : 1a : 2b : c3 : d4 : ef");
}

#[test]
fn b2_parse_accepts_dash_colon_or_none() {
    for text in [
        "00-1a-2b-c3-d4-ef",
        "00:1A:2B:C3:D4:EF",
        "001a2bC3D4eF",
        " 00-1a-2b-c3-d4-ef ",
    ] {
        assert_eq!(MacAddress::parse(text).expect(text), SAMPLE, "{text}");
    }
    assert_eq!("00:1a:2b:c3:d4:ef".parse::<MacAddress>().ok(), Some(SAMPLE));
}

#[test]
fn b2_parse_rejects_malformed_text() {
    for bad in [
        "",
        "00-1a-2b-c3-d4",
        "00-1a-2b-c3-d4-ef-01",
        "00-1a:2b-c3-d4-ef",
        "00.1a.2b.c3.d4.ef",
        "001a2bc3d4eg",
        "0-1a-2b-c3-d4-eff",
        "+01a2bc3d4ef",
    ] {
        let err = MacAddress::parse(bad).expect_err(bad);
        assert_eq!(err.code().as_str(), "SYRP-NET-0004", "{bad}");
    }
}

#[test]
fn b2_null_bytes_and_ordering() {
    assert!(MacAddress::default().is_null());
    assert!(MacAddress::new([0; 6]).is_null());
    assert!(!MacAddress::new([0, 0, 0, 0, 0, 1]).is_null());
    assert_eq!(SAMPLE.bytes(), [0x00, 0x1a, 0x2b, 0xc3, 0xd4, 0xef]);
    assert_eq!(MacAddress::from(SAMPLE.bytes()), SAMPLE);
    assert!(MacAddress::new([0, 0, 0, 0, 0, 1]) < MacAddress::new([0, 0, 0, 0, 1, 0]));
}

#[test]
fn b2_find_all_addresses_has_no_null_or_duplicates() {
    // Containers may have no hardware address at all, so only the contents are checked.
    let all = MacAddress::find_all_addresses().expect("interface query");
    assert!(all.iter().all(|m| !m.is_null()), "null address in {all:?}");
    let unique: HashSet<_> = all.iter().collect();
    assert_eq!(unique.len(), all.len(), "duplicates in {all:?}");
}

proptest! {
    #[test]
    fn b2_parse_display_round_trip(bytes in any::<[u8; 6]>(), sep in prop::sample::select(vec!["-", ":", ""])) {
        let m = MacAddress::new(bytes);
        prop_assert_eq!(MacAddress::parse(&m.to_string_with(sep))?, m);
        prop_assert_eq!(MacAddress::parse(&m.to_string_with(sep).to_uppercase())?, m);
    }
}

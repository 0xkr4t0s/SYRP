// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B1: `IpAddress` value semantics, RFC 5952 text and interface enumeration.

use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use proptest::prelude::*;
use syrp_net::{Diagnostic, IpAddress, IpFamily};

fn ip(text: &str) -> IpAddress {
    IpAddress::parse(text).unwrap_or_else(|e| panic!("{text}: {e}"))
}

#[test]
fn b1_rfc5952_section4_vectors() {
    let cases = [
        // 4.1: leading zeros in a group are dropped.
        ("2001:0db8::0001", "2001:db8::1"),
        // 4.2.1: `::` covers the whole zero run.
        ("2001:db8:0:0:0:0:2:1", "2001:db8::2:1"),
        // 4.2.2: a single zero group is not shortened.
        ("2001:db8:0:1:1:1:1:1", "2001:db8:0:1:1:1:1:1"),
        // 4.2.3: the longest run is shortened, the first one on a tie.
        ("2001:0:0:1:0:0:0:1", "2001:0:0:1::1"),
        ("2001:db8:0:0:1:0:0:1", "2001:db8::1:0:0:1"),
        // 4.3: lower case.
        ("2001:DB8::ABCD", "2001:db8::abcd"),
        // Section 2's equivalent spellings all print the same way.
        ("2001:0db8:0:0:1:0:0:1", "2001:db8::1:0:0:1"),
        ("2001:db8::0:1:0:0:1", "2001:db8::1:0:0:1"),
        ("2001:db8:0000:0:1::1", "2001:db8::1:0:0:1"),
        ("2001:DB8:0:0:1::1", "2001:db8::1:0:0:1"),
        // Section 5: IPv4-mapped addresses keep the dotted quad.
        ("::ffff:c000:0201", "::ffff:192.0.2.1"),
        ("0:0:0:0:0:0:0:0", "::"),
        ("0:0:0:0:0:0:0:1", "::1"),
    ];
    for (input, expected) in cases {
        assert_eq!(ip(input).to_string(), expected, "input {input}");
    }
}

#[test]
fn b1_parse_accepts_v4_and_rejects_garbage() {
    assert_eq!(ip("192.168.1.20"), IpAddress::v4(192, 168, 1, 20));
    assert_eq!(ip("  10.0.0.1\n"), IpAddress::v4(10, 0, 0, 1));
    assert_eq!(
        "::1".parse::<IpAddress>().ok(),
        Some(IpAddress::local(IpFamily::V6))
    );
    for bad in [
        "",
        "1.2.3",
        "256.0.0.1",
        "1.2.3.4:80",
        "[::1]",
        "fe80::1%eth0",
        "10.0.0.0/8",
        "x",
    ] {
        let err = IpAddress::parse(bad).expect_err(bad);
        assert_eq!(err.code().as_str(), "SYRP-NET-0001", "{bad}");
    }
}

#[test]
fn b1_special_addresses_per_family() {
    assert_eq!(IpAddress::any(IpFamily::V4).to_string(), "0.0.0.0");
    assert_eq!(IpAddress::any(IpFamily::V6).to_string(), "::");
    assert_eq!(IpAddress::local(IpFamily::V4).to_string(), "127.0.0.1");
    assert_eq!(IpAddress::local(IpFamily::V6).to_string(), "::1");
    assert_eq!(
        IpAddress::broadcast(IpFamily::V4).to_string(),
        "255.255.255.255"
    );
    assert_eq!(IpAddress::broadcast(IpFamily::V6).to_string(), "ff02::1");
    for family in [IpFamily::V4, IpFamily::V6] {
        assert!(IpAddress::any(family).is_null());
        assert!(!IpAddress::local(family).is_null());
        assert!(!IpAddress::broadcast(family).is_null());
        assert!(IpAddress::local(family).is_loopback());
        assert_eq!(IpAddress::any(family).family(), family);
    }
    assert_eq!(IpAddress::default(), IpAddress::any(IpFamily::V4));
}

#[test]
fn b1_ipv4_mapped_conversions_both_ways() {
    let v4 = IpAddress::v4(192, 0, 2, 33);
    let mapped = v4.to_ipv4_mapped();
    assert!(mapped.is_ipv6() && mapped.is_ipv4_mapped());
    assert_eq!(mapped, ip("::ffff:192.0.2.33"));
    assert_eq!(mapped.to_ipv4(), Some(v4));
    assert_eq!(v4.to_ipv4(), Some(v4));
    assert_eq!(mapped.to_ipv4_mapped(), mapped);
    assert!(!v4.is_ipv4_mapped());
    assert_eq!(ip("2001:db8::1").to_ipv4(), None);
    assert_eq!(ip("::1").to_ipv4(), None, "::1 is not IPv4-mapped");
}

#[test]
fn b1_std_conversion_ordering_and_hashing() {
    let std4 = IpAddr::V4(Ipv4Addr::new(10, 1, 2, 3));
    let std6 = IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 9));
    assert_eq!(IpAddr::from(IpAddress::from(std4)), std4);
    assert_eq!(IpAddress::from(std6).to_std(), std6);
    assert_eq!(
        IpAddress::from(Ipv4Addr::LOCALHOST),
        IpAddress::local(IpFamily::V4)
    );
    assert_eq!(
        IpAddress::from(Ipv6Addr::LOCALHOST),
        IpAddress::local(IpFamily::V6)
    );

    let mut sorted = [
        ip("::1"),
        ip("10.0.0.2"),
        ip("2001:db8::"),
        ip("10.0.0.1"),
        ip("::"),
    ];
    sorted.sort();
    let text: Vec<String> = sorted.iter().map(ToString::to_string).collect();
    assert_eq!(text, ["10.0.0.1", "10.0.0.2", "::", "::1", "2001:db8::"]);

    let set: HashSet<IpAddress> = [ip("10.0.0.1"), ip("10.0.0.1"), ip("::ffff:10.0.0.1")].into();
    assert_eq!(
        set.len(),
        2,
        "a mapped address is a different value from its IPv4 form"
    );
}

#[test]
fn b1_enumeration_includes_loopback() {
    let all = IpAddress::all_addresses(true).expect("interface query");
    assert!(
        all.contains(&IpAddress::local(IpFamily::V4)),
        "no 127.0.0.1 in {all:?}"
    );
    let unique: HashSet<_> = all.iter().collect();
    assert_eq!(unique.len(), all.len(), "duplicates in {all:?}");

    let v4_only = IpAddress::all_addresses(false).expect("interface query");
    assert!(v4_only.iter().all(|a| !a.is_ipv6()));
    assert!(v4_only.contains(&IpAddress::local(IpFamily::V4)));

    let local = IpAddress::local_address(false).expect("interface query");
    assert!(!local.is_null() && !local.is_ipv6());
    assert!(local.is_loopback() || v4_only.contains(&local));
}

#[test]
fn b1_interface_broadcast_address_covers_the_host() {
    let unknown = IpAddress::interface_broadcast_address(IpAddress::v4(203, 0, 113, 254));
    assert_eq!(unknown.expect("interface query"), None);
    for addr in IpAddress::all_addresses(false).expect("interface query") {
        let Some(b) = IpAddress::interface_broadcast_address(addr).expect("query") else {
            continue;
        };
        let (IpAddr::V4(a4), IpAddr::V4(b4)) = (addr.to_std(), b.to_std()) else {
            panic!("broadcast {b} for {addr} is not IPv4");
        };
        let (a, b) = (u32::from(a4), u32::from(b4));
        assert_eq!(a | b, b, "broadcast {b4} does not cover {a4}");
    }
}

proptest! {
    #[test]
    fn b1_parse_display_round_trip_v4(bits in any::<u32>()) {
        let a = IpAddress::from(Ipv4Addr::from(bits));
        prop_assert_eq!(IpAddress::parse(&a.to_string())?, a);
    }

    #[test]
    fn b1_parse_display_round_trip_v6(bits in any::<u128>(), zeros in any::<u8>()) {
        // Clear some groups so `::` compression is exercised often.
        let mask = (0..8).filter(|i| zeros & (1 << i) != 0)
            .fold(u128::MAX, |m, i| m & !(0xffff_u128 << (16 * i)));
        let a = IpAddress::from(Ipv6Addr::from(bits & mask));
        let text = a.to_string();
        prop_assert_eq!(IpAddress::parse(&text)?, a);
        prop_assert!(!text.contains(":::") && text == text.to_lowercase());
        let full = a.to_std().to_string();
        prop_assert_eq!(full, text);
    }
}

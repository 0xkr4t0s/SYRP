// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Behaviour tests for syrp-sys (WP-1.1-12): B1-B4, part of B5 and the B7 error codes.

use std::collections::HashSet;
use syrp_rt::{GuardedAllocator, no_alloc};
use syrp_sys::{CpuFeature, Diagnostic, RelativeTime, SysError, SystemStats, Time, Uuid};

#[global_allocator]
static ALLOC: GuardedAllocator = GuardedAllocator;

fn utc(y: i32, mo: i32, d: i32, h: i32, mi: i32, s: i32, ms: i32) -> Time {
    Time::from_components(y, mo, d, h, mi, s, ms, false)
}

#[test]
fn b1_relative_time_units_round_trip() {
    let t = RelativeTime::weeks(1.0);
    assert_eq!(t.in_days(), 7.0);
    assert_eq!(t.in_hours(), 168.0);
    assert_eq!(t.in_minutes(), 10_080.0);
    assert_eq!(t.in_seconds(), 604_800.0);
    assert_eq!(t.in_milliseconds(), 604_800_000);
    assert_eq!(RelativeTime::days(2.0), RelativeTime::hours(48.0));
    assert_eq!(RelativeTime::milliseconds(-250).in_seconds(), -0.25);
    assert_eq!(RelativeTime::minutes(0.5).in_weeks() * 604_800.0, 30.0);
}

#[test]
fn b1_relative_time_arithmetic_and_ordering() {
    let mut t = RelativeTime::seconds(10.0);
    t += RelativeTime::seconds(5.0);
    t -= RelativeTime::seconds(1.0);
    assert_eq!(t, RelativeTime::seconds(14.0));
    assert_eq!(-t, RelativeTime::seconds(-14.0));
    assert_eq!(t - RelativeTime::seconds(20.0), RelativeTime::seconds(-6.0));
    assert!(RelativeTime::minutes(1.0) > RelativeTime::seconds(59.0));
    assert!(RelativeTime::seconds(-1.0) < RelativeTime::default());
}

#[test]
fn b1_description_matches_documented_format() {
    let d = |s: f64, n| RelativeTime::seconds(s).description(n);
    assert_eq!(d(64.0, 2), "1 min 4 secs");
    assert_eq!(d(6300.0, 2), "1 hr 45 mins");
    assert_eq!(d(7500.0, 2), "2 hrs 5 mins");
    assert_eq!(d(19.0 * 86_400.0, 2), "2 weeks 5 days");
    assert_eq!(d(0.14, 2), "140 ms");
    assert_eq!(d(1.0, 2), "1 sec");
    assert_eq!(d(3604.0, 2), "1 hr");
    assert_eq!(d(3604.0, 3), "1 hr 4 secs");
    assert_eq!(d(400.0 * 86_400.0, 1), "1 year");
    assert_eq!(d(400.0 * 86_400.0, 3), "1 year 5 weeks");
    assert_eq!(d(-90.0, 2), "-1 min 30 secs");
    assert_eq!(d(64.0, 0), "1 min");
    assert_eq!(d(0.0004, 2), "0");
    assert_eq!(RelativeTime::seconds(64.0).to_string(), "1 min 4 secs");
}

#[test]
fn b2_utc_components_and_epoch() {
    let t = utc(2024, 1, 29, 13, 45, 30, 250);
    assert_eq!(t.to_millis(), 1_709_214_330_250);
    let d = t.utc();
    assert_eq!((d.year(), d.month(), d.day_of_month()), (2024, 1, 29));
    assert_eq!(
        (d.hours(), d.minutes(), d.seconds(), d.milliseconds()),
        (13, 45, 30, 250)
    );
    assert_eq!(d.day_of_week(), 4); // Thursday
    assert_eq!(d.day_of_year(), 59);
    assert_eq!(d.utc_offset_seconds(), 0);
    let before = Time::from_millis(-1).utc();
    assert_eq!(
        (before.year(), before.month(), before.day_of_month()),
        (1969, 11, 31)
    );
    assert_eq!(
        (before.hours(), before.milliseconds(), before.day_of_week()),
        (23, 999, 3)
    );
    // Carry-over of out-of-range fields.
    assert_eq!(utc(2023, 12, 1, 0, 0, 0, 0), utc(2024, 0, 1, 0, 0, 0, 0));
    assert_eq!(utc(2024, 2, 0, 24, 0, 0, 0), utc(2024, 2, 1, 0, 0, 0, 0));
    assert_eq!(utc(2024, -1, 1, 0, 0, 0, 0), utc(2023, 11, 1, 0, 0, 0, 0));
    assert_eq!(utc(1600, 1, 29, 0, 0, 0, 0).utc().day_of_month(), 29);
    let now = Time::now();
    assert!(now.utc().year() >= 2024);
    assert_eq!(Time::from_millis(now.to_millis()), now);
}

#[test]
fn b2_arithmetic_with_relative_time() {
    let t = utc(2024, 11, 31, 23, 59, 59, 999);
    let next = t + RelativeTime::milliseconds(1);
    assert_eq!(
        (
            next.utc().year(),
            next.utc().month(),
            next.utc().day_of_month()
        ),
        (2025, 0, 1)
    );
    assert_eq!(next - RelativeTime::milliseconds(1), t);
    assert_eq!(next - t, RelativeTime::milliseconds(1));
    let mut u = t;
    u += RelativeTime::days(1.0);
    u -= RelativeTime::hours(24.0);
    assert_eq!(u, t);
    assert!(next > t);
    assert_eq!(
        Time::from_millis(i64::MAX) + RelativeTime::days(1.0),
        Time::from_millis(i64::MAX)
    );
}

#[test]
fn b2_iso8601_parses_both_forms_and_round_trips() {
    let want = utc(2024, 0, 31, 11, 34, 56, 789);
    for text in [
        "2024-01-31T12:34:56.789+01:00",
        "20240131T123456.789+0100",
        "2024-01-31T11:34:56.789Z",
        "2024-01-31T11:34:56,7891Z",
        "2024-01-31T06:34:56.789-05",
    ] {
        assert_eq!(Time::from_iso8601(text), Ok(want), "{text}");
    }
    assert_eq!(
        Time::from_iso8601("2024-01-31T11:34Z"),
        Ok(utc(2024, 0, 31, 11, 34, 0, 0))
    );
    assert_eq!(
        Time::from_iso8601("20240131T1134Z"),
        Ok(utc(2024, 0, 31, 11, 34, 0, 0))
    );
    assert_eq!(
        Time::from_iso8601("2024-01-31"),
        Ok(Time::from_components(2024, 0, 31, 0, 0, 0, 0, true))
    );
    for t in [
        want,
        Time::from_millis(0),
        utc(1900, 1, 28, 1, 2, 3, 4),
        Time::now(),
    ] {
        assert_eq!(Time::from_iso8601(&t.to_iso8601(true)), Ok(t));
        assert_eq!(Time::from_iso8601(&t.to_iso8601(false)), Ok(t));
    }
    let text = Time::from_millis(0).to_iso8601(true);
    assert_eq!(text.len(), "1970-01-01T00:00:00.000+00:00".len());
    assert_eq!(
        Time::from_millis(0).to_iso8601(false).len(),
        "19700101T000000.000+0000".len()
    );
}

#[test]
fn b2_iso8601_rejects_malformed_text_with_a_code() {
    for (text, pos) in [
        ("", 0),
        ("24-01-01", 0),
        ("2024-13-01", 5),
        ("2023-02-29", 8),
        ("2024-01-31T24:00", 11),
        ("2024-01-31T12:60", 14),
        ("2024-01-31T12:30:00.", 20),
        ("2024-01-31T12:30+1", 17),
        ("2024-01-31 12:30", 10),
        ("2024-01-31T12:30Zjunk", 17),
        ("2024-0131", 7),
    ] {
        let err = Time::from_iso8601(text).unwrap_err();
        assert_eq!(err.code().as_str(), "SYRP-SYS-0001", "{text}");
        let SysError::InvalidIso8601 { position, .. } = err else {
            unreachable!()
        };
        assert_eq!(position, pos, "{text}");
    }
}

#[test]
fn b2_formatted_utc_fields() {
    // Format a local time built from local components, so the result is zone-independent
    // except for %z.
    let t = Time::from_components(2024, 2, 3, 14, 5, 9, 0, true); // Sunday 3 March 2024
    assert_eq!(
        t.formatted("%a %A %b %B %d %H %I %j %m %M %p %S %U %W %w %y %Y"),
        "Sun Sunday Mar March 03 14 02 063 03 05 PM 09 09 09 0 24 2024"
    );
    let week = |y, d| Time::from_components(y, 0, d, 12, 0, 0, 0, true).formatted("%U %W");
    assert_eq!(week(2024, 1), "00 01"); // a Monday
    assert_eq!(week(2023, 1), "01 00"); // a Sunday
    assert_eq!(
        t.formatted("%c|%x|%X|%%|%q|%"),
        "Sun Mar 03 14:05:09 2024|03/03/24|14:05:09|%|%q|%"
    );
    let z = t.formatted("%z");
    assert_eq!(z.len(), 5);
    assert!(z.starts_with('+') || z.starts_with('-'));
}

#[test]
fn b3_counters_are_monotonic() {
    let tps = Time::high_resolution_ticks_per_second();
    assert!(tps >= 1000);
    let (mut ticks, mut hi, mut ms) = (
        Time::high_resolution_ticks(),
        Time::millisecond_counter_hi_res(),
        Time::millisecond_counter(),
    );
    for _ in 0..10_000 {
        let (t, h, m) = (
            Time::high_resolution_ticks(),
            Time::millisecond_counter_hi_res(),
            Time::millisecond_counter(),
        );
        assert!(t >= ticks && h >= hi && m.wrapping_sub(ms) < 1 << 31);
        (ticks, hi, ms) = (t, h, m);
    }
    // The three views agree on the same clock.
    let ratio = Time::high_resolution_ticks() as f64 * 1000.0 / tps as f64;
    assert!((Time::millisecond_counter_hi_res() - ratio).abs() < 50.0);
    assert!((f64::from(Time::millisecond_counter()) - ratio % 4_294_967_296.0).abs() < 50.0);
}

#[test]
fn b3_counters_do_not_allocate() {
    let _ = Time::high_resolution_ticks(); // Any lazy first-use set-up happens here.
    let sum = no_alloc(|| {
        let mut acc = 0.0;
        for _ in 0..1000 {
            acc += Time::millisecond_counter_hi_res();
            acc += f64::from(Time::millisecond_counter());
            acc += Time::high_resolution_ticks() as f64;
            acc += Time::high_resolution_ticks_per_second() as f64;
        }
        acc
    });
    assert!(sum > 0.0);
}

#[test]
fn b3_wait_for_millisecond_counter() {
    let start = Time::millisecond_counter_hi_res();
    let target = Time::millisecond_counter().wrapping_add(20);
    Time::wait_for_millisecond_counter(target);
    assert!(Time::millisecond_counter().wrapping_sub(target) < 1 << 31);
    assert!(Time::millisecond_counter_hi_res() - start >= 19.0);
    let before = Time::millisecond_counter_hi_res();
    Time::wait_for_millisecond_counter(Time::millisecond_counter().wrapping_sub(1000));
    assert!(Time::millisecond_counter_hi_res() - before < 100.0);
}

#[test]
fn b4_random_uuids_are_version_4_and_distinct() {
    let ids: Vec<Uuid> = (0..1000).map(|_| Uuid::random().unwrap()).collect();
    for id in &ids {
        assert_eq!(id.version(), 4);
        assert_eq!(id.to_bytes()[8] & 0xc0, 0x80);
        assert!(!id.is_nil());
    }
    let unique: HashSet<Uuid> = ids.iter().copied().collect();
    assert_eq!(unique.len(), ids.len());
}

#[test]
fn b4_text_and_byte_forms_round_trip() {
    let bytes = [
        0x12, 0x3e, 0x45, 0x67, 0xe8, 0x9b, 0x12, 0xd3, 0xa4, 0x56, 0x42, 0x66, 0x14, 0x17, 0x40,
        0x00,
    ];
    let id = Uuid::from_bytes(bytes);
    assert_eq!(id.to_bytes(), bytes);
    assert_eq!(id.to_string(), "123e4567-e89b-12d3-a456-426614174000");
    assert_eq!(id.to_hyphenated_string(), id.to_string());
    assert_eq!(
        id.to_undelimited_string(),
        "123e4567e89b12d3a456426614174000"
    );
    assert_eq!(Uuid::parse("123E4567-E89B-12D3-A456-426614174000"), Ok(id));
    assert_eq!("123e4567e89b12d3a456426614174000".parse(), Ok(id));
    let r = Uuid::random().unwrap();
    assert_eq!(Uuid::parse(&r.to_undelimited_string()), Ok(r));
    assert_eq!(Uuid::nil(), Uuid::from_bytes([0; 16]));
    assert!(Uuid::nil().is_nil());
}

#[test]
fn b4_ordering_and_hashing_follow_bytes() {
    let mut b = [0_u8; 16];
    b[15] = 1;
    let low = Uuid::from_bytes(b);
    b[0] = 1;
    let high = Uuid::from_bytes(b);
    assert!(Uuid::nil() < low && low < high);
    let mut set = HashSet::new();
    assert!(set.insert(high));
    assert!(!set.insert(Uuid::from_bytes(high.to_bytes())));
}

#[test]
fn b4_parse_rejects_malformed_text_with_a_code() {
    for (text, pos) in [
        ("", 0),
        ("123e4567-e89b-12d3-a456-42661417400", 0),
        ("123e4567e89b12d3a456426614174000a", 0),
        ("123e4567+e89b-12d3-a456-426614174000", 8),
        ("123e4567-e89b-12d3-a456-42661417400g", 35),
        ("123e4567e89b12d3a45642661417400-", 31),
        ("{23e4567-e89b-12d3-a456-426614174000", 0),
    ] {
        let err = Uuid::parse(text).unwrap_err();
        assert_eq!(err.code().as_str(), "SYRP-SYS-0002", "{text}");
        let SysError::InvalidUuid { position, .. } = err else {
            unreachable!()
        };
        assert_eq!(position, pos, "{text}");
    }
}

#[test]
fn b7_errors_carry_sys_codes_and_hints() {
    let iso = Time::from_iso8601("x").unwrap_err();
    let uuid = Uuid::parse("x").unwrap_err();
    assert_eq!((iso.code().domain(), uuid.code().domain()), ("SYS", "SYS"));
    for err in [iso, uuid] {
        assert!(err.hint().is_some());
        assert!(err.to_string().starts_with(err.code().as_str()));
    }
    let catalogue = include_str!("../../../../errors/sys.toml");
    for code in ["SYRP-SYS-0001", "SYRP-SYS-0002", "SYRP-SYS-0003"] {
        assert!(catalogue.contains(&format!("code = \"{code}\"")), "{code}");
    }
}

#[test]
fn b5_machine_facts_are_plausible() {
    assert_eq!(SystemStats::is_64_bit(), size_of::<usize>() == 8);
    assert!(SystemStats::num_cpus() >= 1);
    let page = SystemStats::page_size();
    assert!(page.is_power_of_two() && page >= 4096, "{page}");
    if cfg!(any(target_os = "linux", target_os = "macos", windows)) {
        let mb = SystemStats::memory_size_mb().expect("memory size on a supported OS");
        assert!((64..1 << 30).contains(&mb), "{mb}");
    }
}

#[test]
fn b5_cpu_features_match_the_architecture() {
    let all = [
        CpuFeature::Sse,
        CpuFeature::Sse2,
        CpuFeature::Sse3,
        CpuFeature::Ssse3,
        CpuFeature::Sse41,
        CpuFeature::Sse42,
        CpuFeature::Avx,
        CpuFeature::Avx2,
        CpuFeature::Avx512F,
        CpuFeature::Neon,
    ];
    let x86 = cfg!(any(target_arch = "x86", target_arch = "x86_64"));
    for f in all {
        let has = SystemStats::has_cpu_feature(f);
        if f == CpuFeature::Neon {
            assert_eq!(has, cfg!(target_arch = "aarch64"));
        } else if !x86 {
            assert!(!has, "{f:?}");
        }
    }
    if cfg!(target_arch = "x86_64") {
        assert!(SystemStats::has_cpu_feature(CpuFeature::Sse2));
    }
    // Each level implies the one below it.
    for pair in all[..9].windows(2) {
        if SystemStats::has_cpu_feature(pair[1]) && pair[1] != CpuFeature::Avx512F {
            assert!(SystemStats::has_cpu_feature(pair[0]), "{pair:?}");
        }
    }
}

#[test]
fn b5_environment_variable_or_default() {
    let path = SystemStats::environment_variable("PATH", "none");
    assert_eq!(
        path,
        std::env::var("PATH").unwrap_or_else(|_| "none".into())
    );
    assert_eq!(
        SystemStats::environment_variable("SYRP_TEST_UNSET_42", "dflt"),
        "dflt"
    );
}

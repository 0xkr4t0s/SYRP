// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B2 local-time tests. A separate binary with a single test, because it sets `TZ`, which must
//! happen before any other thread reads the time zone.

use syrp_sys::Time;

#[cfg(unix)]
fn utc(y: i32, mo: i32, d: i32, h: i32, mi: i32) -> Time {
    Time::from_components(y, mo, d, h, mi, 0, 0, false)
}

#[test]
#[cfg(unix)]
fn b2_local_time_follows_tz() {
    // A POSIX rule string needs no zoneinfo files: UTC-5 (EST), UTC-4 (EDT) from the second
    // Sunday of March to the first Sunday of November, switching at 02:00 local time.
    // SAFETY: this binary has one test and sets TZ before anything reads the environment.
    unsafe { std::env::set_var("TZ", "EST5EDT,M3.2.0,M11.1.0") };
    let winter = utc(2024, 0, 15, 17, 0);
    assert_eq!(winter.utc_offset_seconds(), -5 * 3600);
    assert_eq!(winter.local().hours(), 12);
    assert_eq!(winter.local().utc_offset_seconds(), -5 * 3600);
    let summer = utc(2024, 6, 15, 17, 0);
    assert_eq!(summer.utc_offset_seconds(), -4 * 3600);
    assert_eq!(summer.local().hours(), 13);
    assert_eq!(
        Time::from_components(2024, 6, 15, 13, 0, 0, 0, true),
        summer
    );
    assert_eq!(
        Time::from_components(2024, 0, 15, 12, 0, 0, 0, true),
        winter
    );
    // Day boundaries move with the zone.
    let d = utc(2024, 2, 1, 3, 0).local();
    assert_eq!((d.month(), d.day_of_month(), d.hours()), (1, 29, 22));
    // Around the spring-forward gap (02:00 to 03:00 local on 10 March 2024).
    let after = Time::from_components(2024, 2, 10, 3, 30, 0, 0, true);
    assert_eq!(after, utc(2024, 2, 10, 7, 30));
    let before = Time::from_components(2024, 2, 10, 1, 30, 0, 0, true);
    assert_eq!(before, utc(2024, 2, 10, 6, 30));
}

#[test]
#[cfg(not(unix))]
fn b2_local_time_follows_tz() {
    eprintln!("skipped: setting the time zone through TZ is only tested on Unix");
}

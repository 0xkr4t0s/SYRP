// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B2 local-time tests. A separate binary with a single test, because it sets `TZ`, which must
//! happen before any other thread reads the time zone.

use syrp_sys::Time;

#[test]
#[cfg(unix)]
fn b2_local_time_follows_tz() {
    // A POSIX rule string needs no zoneinfo files: UTC-5 (EST), UTC-4 (EDT) from the second
    // Sunday of March to the first Sunday of November, switching at 02:00 local time.
    // SAFETY: this binary has one test and sets TZ before anything reads the environment.
    unsafe { std::env::set_var("TZ", "EST5EDT,M3.2.0,M11.1.0") };
    let winter = Time::from_iso8601("2024-01-15T17:00:00Z").unwrap();
    assert_eq!(winter.utc_offset_seconds(), -5 * 3600);
    assert_eq!(winter.local().hours(), 12);
    assert_eq!(winter.local().utc_offset_seconds(), -5 * 3600);
    let summer = Time::from_iso8601("2024-07-15T17:00:00Z").unwrap();
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
    let d = Time::from_iso8601("2024-03-01T03:00:00Z").unwrap().local();
    assert_eq!((d.month(), d.day_of_month(), d.hours()), (1, 29, 22));
    // Text without an offset is local; output carries the offset.
    assert_eq!(Time::from_iso8601("2024-07-15T13:00:00"), Ok(summer));
    assert_eq!(summer.to_iso8601(true), "2024-07-15T13:00:00.000-04:00");
    assert_eq!(summer.to_iso8601(false), "20240715T130000.000-0400");
    assert_eq!(summer.formatted("%H:%M %z"), "13:00 -0400");
    // Around the spring-forward gap (02:00 to 03:00 local on 10 March 2024).
    let after = Time::from_components(2024, 2, 10, 3, 30, 0, 0, true);
    assert_eq!(after, Time::from_iso8601("2024-03-10T07:30:00Z").unwrap());
    let before = Time::from_components(2024, 2, 10, 1, 30, 0, 0, true);
    assert_eq!(before, Time::from_iso8601("2024-03-10T06:30:00Z").unwrap());
}

#[test]
#[cfg(not(unix))]
fn b2_local_time_follows_tz() {
    eprintln!("skipped: setting the time zone through TZ is only tested on Unix");
}

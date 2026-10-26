// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Behaviour tests for syrp-sys (WP-1.1-12), increment 1: B1, B3 and the calendar part of B2.

use syrp_rt::{GuardedAllocator, no_alloc};
use syrp_sys::{RelativeTime, Time};

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

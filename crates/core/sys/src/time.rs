// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`Time`], its calendar view [`DateTime`], and the high-resolution counters.

use std::ops::{Add, AddAssign, Sub, SubAssign};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::{RelativeTime, os};

const MS_PER_DAY: i64 = 86_400_000;

/// An absolute point in time: milliseconds since 1970-01-01 00:00:00 UTC, as an `i64`.
///
/// Calendar fields come from [`Time::utc`] or [`Time::local`]. Local time uses the operating
/// system's time-zone database (on Unix, the `TZ` variable and the zoneinfo files; on Windows,
/// the system setting); instants the OS cannot convert, and targets other than Unix and
/// Windows, are treated as UTC.
///
/// ```
/// use syrp_sys::{RelativeTime, Time};
/// let t = Time::from_components(2024, 1, 29, 12, 0, 0, 0, false); // 29 February 2024
/// let later = t + RelativeTime::days(1.0);
/// assert_eq!(later.utc().month(), 2); // March (months count from 0)
/// assert_eq!(later.utc().day_of_month(), 1);
/// assert_eq!((later - t).in_hours(), 24.0);
/// ```
#[doc(alias = "juce::Time")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Time {
    millis: i64,
}

/// Calendar fields of a [`Time`] in one time zone, from [`Time::utc`] or [`Time::local`].
///
/// ```
/// use syrp_sys::Time;
/// let d = Time::from_millis(0).utc();
/// assert_eq!((d.year(), d.month(), d.day_of_month()), (1970, 0, 1));
/// assert_eq!(d.day_of_week(), 4); // Thursday
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DateTime {
    year: i32,
    month: u8,
    day: u8,
    weekday: u8,
    hours: u8,
    minutes: u8,
    seconds: u8,
    millis: u16,
    day_of_year: u16,
    offset: i32,
}

/// Days from 1970-01-01 to the given proleptic Gregorian date (`month` 1-12, `day` from 1).
pub(crate) fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468
}

/// The date (year, month 1-12, day from 1) that is `days` after 1970-01-01.
pub(crate) fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

/// Local minus UTC, in seconds, at `millis`.
fn offset_at(millis: i64) -> i32 {
    os::local_offset_seconds(millis.div_euclid(1000)).unwrap_or(0)
}

impl Time {
    /// The current system time (wall clock; it can jump if the clock is changed).
    ///
    /// ```
    /// # use syrp_sys::Time;
    /// assert!(Time::now().utc().year() >= 2024);
    /// ```
    #[doc(alias = "getCurrentTime")]
    #[must_use]
    pub fn now() -> Self {
        let millis = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(d) => i64::try_from(d.as_millis()).unwrap_or(i64::MAX),
            Err(e) => -i64::try_from(e.duration().as_millis()).unwrap_or(i64::MAX),
        };
        Self { millis }
    }

    /// The time `millis` milliseconds after the Unix epoch (negative is before it).
    ///
    /// ```
    /// # use syrp_sys::Time;
    /// assert_eq!(Time::from_millis(86_400_000).utc().day_of_month(), 2);
    /// ```
    #[must_use]
    pub const fn from_millis(millis: i64) -> Self {
        Self { millis }
    }

    /// Milliseconds since the Unix epoch.
    ///
    /// ```
    /// # use syrp_sys::Time;
    /// assert_eq!(Time::from_millis(42).to_millis(), 42);
    /// ```
    #[doc(alias = "toMilliseconds")]
    #[must_use]
    pub const fn to_millis(self) -> i64 {
        self.millis
    }

    /// The time at the given calendar fields, read as local time when `local` is true and as
    /// UTC otherwise.
    ///
    /// `month0` counts from 0 (January) and `day` from 1. Out-of-range fields carry over, so
    /// month 12 is January of the next year and day 0 is the last day of the previous month.
    /// Arithmetic saturates at the `i64` range. A local time that a daylight-saving change
    /// skips or repeats resolves to one of the nearby instants.
    ///
    /// ```
    /// # use syrp_sys::Time;
    /// let t = Time::from_components(2000, 0, 1, 0, 0, 0, 0, false);
    /// assert_eq!(t.to_millis(), 946_684_800_000);
    /// assert_eq!(Time::from_components(1999, 12, 1, 0, 0, 0, 0, false), t);
    /// ```
    #[allow(
        clippy::too_many_arguments,
        reason = "mirrors the calendar fields one to one"
    )]
    #[must_use]
    pub fn from_components(
        year: i32,
        month0: i32,
        day: i32,
        hours: i32,
        minutes: i32,
        seconds: i32,
        millis: i32,
        local: bool,
    ) -> Self {
        let y = i64::from(year) + i64::from(month0).div_euclid(12);
        let m = i64::from(month0).rem_euclid(12) + 1;
        let days = days_from_civil(y, m, 1) + i64::from(day) - 1;
        let naive = days
            .saturating_mul(MS_PER_DAY)
            .saturating_add(i64::from(hours) * 3_600_000)
            .saturating_add(i64::from(minutes) * 60_000)
            .saturating_add(i64::from(seconds) * 1000)
            .saturating_add(i64::from(millis));
        if !local {
            return Self { millis: naive };
        }
        let guess = naive.saturating_sub(i64::from(offset_at(naive)) * 1000);
        Self {
            millis: naive.saturating_sub(i64::from(offset_at(guess)) * 1000),
        }
    }

    /// Calendar fields in UTC.
    ///
    /// ```
    /// # use syrp_sys::Time;
    /// assert_eq!(Time::from_millis(-1).utc().year(), 1969);
    /// ```
    #[must_use]
    pub fn utc(self) -> DateTime {
        DateTime::new(self.millis, 0)
    }

    /// Calendar fields in the local time zone.
    ///
    /// On Unix the C library reads `TZ` once and caches the zone (glibc does not re-read it
    /// on every call), so changing `TZ` while the process runs may not take effect: set it
    /// before the first local-time call.
    ///
    /// ```
    /// # use syrp_sys::Time;
    /// let t = Time::now();
    /// assert_eq!(t.local().utc_offset_seconds(), t.utc_offset_seconds());
    /// ```
    #[must_use]
    pub fn local(self) -> DateTime {
        DateTime::new(self.millis, offset_at(self.millis))
    }

    /// Local time minus UTC at this instant, in seconds (positive east of Greenwich, and
    /// including any daylight-saving shift).
    ///
    /// ```
    /// # use syrp_sys::Time;
    /// assert!(Time::now().utc_offset_seconds().abs() <= 14 * 3600);
    /// ```
    #[doc(alias = "getUTCOffsetSeconds")]
    #[must_use]
    pub fn utc_offset_seconds(self) -> i32 {
        offset_at(self.millis)
    }

    /// A millisecond counter that only moves forwards, from a fixed point (usually system
    /// start-up). It wraps around after about 49.7 days, so compare readings with
    /// `wrapping_sub`.
    ///
    /// RT-safe: lock-free, allocation-free and non-blocking, like the other counters. On
    /// targets other than Linux, Apple and Windows the counters measure from first use, and
    /// that first call initialises a `OnceLock`, so make one call off the audio thread first.
    ///
    /// ```
    /// # use syrp_sys::Time;
    /// let a = Time::millisecond_counter();
    /// assert!(Time::millisecond_counter().wrapping_sub(a) < 1000);
    /// ```
    #[doc(alias = "getMillisecondCounter")]
    #[must_use]
    pub fn millisecond_counter() -> u32 {
        let (t, f) = (os::ticks(), os::ticks_per_second());
        let ms = (t / f).wrapping_mul(1000).wrapping_add((t % f) * 1000 / f);
        ms as u32 // Truncation is the documented wrap-around.
    }

    /// The same clock as [`Time::millisecond_counter`] with sub-millisecond resolution, as
    /// an `f64` that does not wrap. RT-safe.
    ///
    /// ```
    /// # use syrp_sys::Time;
    /// let a = Time::millisecond_counter_hi_res();
    /// assert!(Time::millisecond_counter_hi_res() >= a);
    /// ```
    #[doc(alias = "getMillisecondCounterHiRes")]
    #[must_use]
    pub fn millisecond_counter_hi_res() -> f64 {
        os::ticks() as f64 * 1000.0 / os::ticks_per_second() as f64
    }

    /// The raw monotonic clock, in ticks of [`Time::high_resolution_ticks_per_second`].
    /// RT-safe.
    ///
    /// ```
    /// # use syrp_sys::Time;
    /// let a = Time::high_resolution_ticks();
    /// assert!(Time::high_resolution_ticks() >= a);
    /// ```
    #[doc(alias = "getHighResolutionTicks")]
    #[must_use]
    pub fn high_resolution_ticks() -> i64 {
        os::ticks()
    }

    /// How many [`Time::high_resolution_ticks`] make one second (at least 1). RT-safe.
    ///
    /// ```
    /// # use syrp_sys::Time;
    /// assert!(Time::high_resolution_ticks_per_second() >= 1000);
    /// ```
    #[doc(alias = "getHighResolutionTicksPerSecond")]
    #[must_use]
    pub fn high_resolution_ticks_per_second() -> i64 {
        os::ticks_per_second()
    }

    /// Blocks the calling thread until [`Time::millisecond_counter`] reaches `target`,
    /// sleeping while it is more than a couple of milliseconds away and yielding after that.
    ///
    /// Returns at once if `target` is now or up to 2^31 ms in the past. Not RT-safe: it
    /// sleeps.
    ///
    /// ```
    /// # use syrp_sys::Time;
    /// let target = Time::millisecond_counter().wrapping_add(5);
    /// Time::wait_for_millisecond_counter(target);
    /// assert!(Time::millisecond_counter().wrapping_sub(target) < 1 << 31);
    /// ```
    #[doc(alias = "waitForMillisecondCounter")]
    pub fn wait_for_millisecond_counter(target: u32) {
        loop {
            let left = target.wrapping_sub(Self::millisecond_counter()) as i32;
            if left <= 0 {
                return;
            }
            if left > 2 {
                std::thread::sleep(Duration::from_millis(u64::from(left.unsigned_abs() - 2)));
            } else {
                std::thread::yield_now();
            }
        }
    }
}

impl DateTime {
    fn new(utc_millis: i64, offset: i32) -> Self {
        let ms = utc_millis.saturating_add(i64::from(offset) * 1000);
        let days = ms.div_euclid(MS_PER_DAY);
        let of_day = ms.rem_euclid(MS_PER_DAY);
        let (y, m, d) = civil_from_days(days);
        Self {
            year: y as i32, // |year| < 2^31 for every i64 millisecond count.
            month: (m - 1) as u8,
            day: d as u8,
            weekday: (days + 4).rem_euclid(7) as u8,
            hours: (of_day / 3_600_000) as u8,
            minutes: (of_day / 60_000 % 60) as u8,
            seconds: (of_day / 1000 % 60) as u8,
            millis: (of_day % 1000) as u16,
            day_of_year: (days - days_from_civil(y, 1, 1)) as u16,
            offset,
        }
    }

    /// The year, for example 2024.
    ///
    /// ```
    /// # use syrp_sys::Time;
    /// assert_eq!(Time::from_millis(0).utc().year(), 1970);
    /// ```
    #[doc(alias = "getYear")]
    #[must_use]
    pub const fn year(self) -> i32 {
        self.year
    }

    /// The month, from 0 (January) to 11 (December).
    ///
    /// ```
    /// # use syrp_sys::Time;
    /// assert_eq!(Time::from_components(2024, 11, 25, 0, 0, 0, 0, false).utc().month(), 11);
    /// ```
    #[doc(alias = "getMonth")]
    #[must_use]
    pub const fn month(self) -> u32 {
        self.month as u32
    }

    /// The day of the month, from 1.
    ///
    /// ```
    /// # use syrp_sys::Time;
    /// assert_eq!(Time::from_components(2024, 0, 31, 0, 0, 0, 0, false).utc().day_of_month(), 31);
    /// ```
    #[doc(alias = "getDayOfMonth")]
    #[must_use]
    pub const fn day_of_month(self) -> u32 {
        self.day as u32
    }

    /// The day of the week, from 0 (Sunday) to 6 (Saturday).
    ///
    /// ```
    /// # use syrp_sys::Time;
    /// assert_eq!(Time::from_components(2024, 0, 7, 0, 0, 0, 0, false).utc().day_of_week(), 0);
    /// ```
    #[doc(alias = "getDayOfWeek")]
    #[must_use]
    pub const fn day_of_week(self) -> u32 {
        self.weekday as u32
    }

    /// The day of the year, from 0 (1 January) to 365.
    ///
    /// ```
    /// # use syrp_sys::Time;
    /// assert_eq!(Time::from_components(2024, 11, 31, 0, 0, 0, 0, false).utc().day_of_year(), 365);
    /// ```
    #[doc(alias = "getDayOfYear")]
    #[must_use]
    pub const fn day_of_year(self) -> u32 {
        self.day_of_year as u32
    }

    /// The hour, from 0 to 23.
    ///
    /// ```
    /// # use syrp_sys::Time;
    /// assert_eq!(Time::from_components(2024, 0, 1, 23, 0, 0, 0, false).utc().hours(), 23);
    /// ```
    #[doc(alias = "getHours")]
    #[must_use]
    pub const fn hours(self) -> u32 {
        self.hours as u32
    }

    /// The minute, from 0 to 59.
    ///
    /// ```
    /// # use syrp_sys::Time;
    /// assert_eq!(Time::from_components(2024, 0, 1, 0, 59, 0, 0, false).utc().minutes(), 59);
    /// ```
    #[doc(alias = "getMinutes")]
    #[must_use]
    pub const fn minutes(self) -> u32 {
        self.minutes as u32
    }

    /// The second, from 0 to 59.
    ///
    /// ```
    /// # use syrp_sys::Time;
    /// assert_eq!(Time::from_components(2024, 0, 1, 0, 0, 7, 0, false).utc().seconds(), 7);
    /// ```
    #[doc(alias = "getSeconds")]
    #[must_use]
    pub const fn seconds(self) -> u32 {
        self.seconds as u32
    }

    /// The millisecond, from 0 to 999.
    ///
    /// ```
    /// # use syrp_sys::Time;
    /// assert_eq!(Time::from_millis(-1).utc().milliseconds(), 999);
    /// ```
    #[doc(alias = "getMilliseconds")]
    #[must_use]
    pub const fn milliseconds(self) -> u32 {
        self.millis as u32
    }

    /// The zone's offset from UTC in seconds: 0 for [`Time::utc`].
    ///
    /// ```
    /// # use syrp_sys::Time;
    /// assert_eq!(Time::now().utc().utc_offset_seconds(), 0);
    /// ```
    #[must_use]
    pub const fn utc_offset_seconds(self) -> i32 {
        self.offset
    }
}

impl Add<RelativeTime> for Time {
    type Output = Self;
    /// Adds the length, rounded to whole milliseconds (saturating).
    fn add(self, rhs: RelativeTime) -> Self {
        Self {
            millis: self.millis.saturating_add(rhs.in_milliseconds()),
        }
    }
}

impl Sub<RelativeTime> for Time {
    type Output = Self;
    /// Subtracts the length, rounded to whole milliseconds (saturating).
    fn sub(self, rhs: RelativeTime) -> Self {
        Self {
            millis: self.millis.saturating_sub(rhs.in_milliseconds()),
        }
    }
}

impl AddAssign<RelativeTime> for Time {
    fn add_assign(&mut self, rhs: RelativeTime) {
        *self = *self + rhs;
    }
}

impl SubAssign<RelativeTime> for Time {
    fn sub_assign(&mut self, rhs: RelativeTime) {
        *self = *self - rhs;
    }
}

impl Sub for Time {
    type Output = RelativeTime;
    /// The signed length from `rhs` to `self`.
    fn sub(self, rhs: Self) -> RelativeTime {
        RelativeTime::milliseconds(self.millis.saturating_sub(rhs.millis))
    }
}

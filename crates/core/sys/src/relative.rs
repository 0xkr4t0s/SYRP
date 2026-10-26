// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`RelativeTime`]: a signed duration in seconds.

use std::fmt::{self, Write as _};
use std::ops::{Add, AddAssign, Neg, Sub, SubAssign};

/// A signed length of time, stored as seconds in an `f64`.
///
/// Unlike [`std::time::Duration`] it can be negative, and it has the JUCE-style unit
/// constructors and accessors. Arithmetic is plain `f64` arithmetic on the seconds.
///
/// ```
/// use syrp_sys::RelativeTime;
/// let t = RelativeTime::hours(2.0) + RelativeTime::minutes(5.0);
/// assert_eq!(t.in_minutes(), 125.0);
/// assert!(t > RelativeTime::hours(2.0));
/// assert_eq!(t.description(2), "2 hrs 5 mins");
/// ```
#[doc(alias = "juce::RelativeTime")]
#[derive(Debug, Clone, Copy, Default, PartialEq, PartialOrd)]
pub struct RelativeTime {
    seconds: f64,
}

/// One unit of [`RelativeTime::description`]: length in milliseconds, singular, plural.
const UNITS: [(u64, &str, &str); 7] = [
    (365 * 86_400_000, "year", "years"),
    (7 * 86_400_000, "week", "weeks"),
    (86_400_000, "day", "days"),
    (3_600_000, "hr", "hrs"),
    (60_000, "min", "mins"),
    (1000, "sec", "secs"),
    (1, "ms", "ms"),
];

impl RelativeTime {
    /// A length of `ms` milliseconds.
    ///
    /// ```
    /// # use syrp_sys::RelativeTime;
    /// assert_eq!(RelativeTime::milliseconds(1500).in_seconds(), 1.5);
    /// ```
    #[must_use]
    pub fn milliseconds(ms: i64) -> Self {
        Self {
            seconds: ms as f64 / 1000.0,
        }
    }

    /// A length of `s` seconds.
    ///
    /// ```
    /// # use syrp_sys::RelativeTime;
    /// assert_eq!(RelativeTime::seconds(-2.5).in_milliseconds(), -2500);
    /// ```
    #[must_use]
    pub const fn seconds(s: f64) -> Self {
        Self { seconds: s }
    }

    /// A length of `m` minutes.
    ///
    /// ```
    /// # use syrp_sys::RelativeTime;
    /// assert_eq!(RelativeTime::minutes(1.5).in_seconds(), 90.0);
    /// ```
    #[must_use]
    pub fn minutes(m: f64) -> Self {
        Self { seconds: m * 60.0 }
    }

    /// A length of `h` hours.
    ///
    /// ```
    /// # use syrp_sys::RelativeTime;
    /// assert_eq!(RelativeTime::hours(2.0).in_minutes(), 120.0);
    /// ```
    #[must_use]
    pub fn hours(h: f64) -> Self {
        Self {
            seconds: h * 3600.0,
        }
    }

    /// A length of `d` days of 24 hours.
    ///
    /// ```
    /// # use syrp_sys::RelativeTime;
    /// assert_eq!(RelativeTime::days(1.0).in_hours(), 24.0);
    /// ```
    #[must_use]
    pub fn days(d: f64) -> Self {
        Self {
            seconds: d * 86_400.0,
        }
    }

    /// A length of `w` weeks of 7 days.
    ///
    /// ```
    /// # use syrp_sys::RelativeTime;
    /// assert_eq!(RelativeTime::weeks(2.0).in_days(), 14.0);
    /// ```
    #[must_use]
    pub fn weeks(w: f64) -> Self {
        Self {
            seconds: w * 604_800.0,
        }
    }

    /// The length in whole milliseconds, rounded to nearest (saturating; NaN gives 0).
    ///
    /// ```
    /// # use syrp_sys::RelativeTime;
    /// assert_eq!(RelativeTime::seconds(0.0016).in_milliseconds(), 2);
    /// ```
    #[doc(alias = "inMilliseconds")]
    #[must_use]
    pub fn in_milliseconds(self) -> i64 {
        (self.seconds * 1000.0).round() as i64
    }

    /// The length in seconds.
    ///
    /// ```
    /// # use syrp_sys::RelativeTime;
    /// assert_eq!(RelativeTime::minutes(2.0).in_seconds(), 120.0);
    /// ```
    #[doc(alias = "inSeconds")]
    #[must_use]
    pub const fn in_seconds(self) -> f64 {
        self.seconds
    }

    /// The length in minutes.
    ///
    /// ```
    /// # use syrp_sys::RelativeTime;
    /// assert_eq!(RelativeTime::seconds(90.0).in_minutes(), 1.5);
    /// ```
    #[must_use]
    pub fn in_minutes(self) -> f64 {
        self.seconds / 60.0
    }

    /// The length in hours.
    ///
    /// ```
    /// # use syrp_sys::RelativeTime;
    /// assert_eq!(RelativeTime::minutes(90.0).in_hours(), 1.5);
    /// ```
    #[must_use]
    pub fn in_hours(self) -> f64 {
        self.seconds / 3600.0
    }

    /// The length in days of 24 hours.
    ///
    /// ```
    /// # use syrp_sys::RelativeTime;
    /// assert_eq!(RelativeTime::hours(36.0).in_days(), 1.5);
    /// ```
    #[must_use]
    pub fn in_days(self) -> f64 {
        self.seconds / 86_400.0
    }

    /// The length in weeks of 7 days.
    ///
    /// ```
    /// # use syrp_sys::RelativeTime;
    /// assert_eq!(RelativeTime::days(3.5).in_weeks(), 0.5);
    /// ```
    #[must_use]
    pub fn in_weeks(self) -> f64 {
        self.seconds / 604_800.0
    }

    /// A short readable description such as `"1 min 4 secs"`, `"1 hr 45 mins"`,
    /// `"2 weeks 5 days"` or `"140 ms"`.
    ///
    /// The length is first rounded to whole milliseconds. The units, largest first, are
    /// `year`/`years` (365 days), `week`/`weeks`, `day`/`days`, `hr`/`hrs`, `min`/`mins`,
    /// `sec`/`secs` and `ms`. The description starts at the largest unit that is not zero and
    /// looks at `max_units` units from there (at least one); units in that window whose count
    /// is zero are left out, so 1 hour and 4 seconds with `max_units == 2` reads `"1 hr"`.
    /// Counts and units are separated by single spaces, a negative length starts with `-`,
    /// and a length that rounds to zero is `"0"`.
    ///
    /// ```
    /// # use syrp_sys::RelativeTime;
    /// assert_eq!(RelativeTime::seconds(64.0).description(2), "1 min 4 secs");
    /// assert_eq!(RelativeTime::days(19.0).description(2), "2 weeks 5 days");
    /// assert_eq!(RelativeTime::milliseconds(140).description(2), "140 ms");
    /// assert_eq!(RelativeTime::seconds(-3725.0).description(3), "-1 hr 2 mins 5 secs");
    /// assert_eq!(RelativeTime::seconds(0.0).description(2), "0");
    /// ```
    #[doc(alias = "getDescription")]
    #[must_use]
    pub fn description(self, max_units: usize) -> String {
        let ms = self.in_milliseconds();
        if ms == 0 {
            return "0".to_owned();
        }
        let mut rest = ms.unsigned_abs();
        let mut out = String::new();
        if ms < 0 {
            out.push('-');
        }
        let first = UNITS
            .iter()
            .position(|u| rest >= u.0)
            .unwrap_or(UNITS.len() - 1);
        let last = (first + max_units.max(1)).min(UNITS.len());
        let mut sep = "";
        for &(len, one, many) in &UNITS[first..last] {
            let n = rest / len;
            rest %= len;
            if n > 0 {
                let unit = if n == 1 { one } else { many };
                let _ = write!(out, "{sep}{n} {unit}");
                sep = " ";
            }
        }
        out
    }
}

impl fmt::Display for RelativeTime {
    /// Formats as [`RelativeTime::description`] with two units.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.description(2))
    }
}

impl Add for RelativeTime {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            seconds: self.seconds + rhs.seconds,
        }
    }
}

impl Sub for RelativeTime {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self {
            seconds: self.seconds - rhs.seconds,
        }
    }
}

impl Neg for RelativeTime {
    type Output = Self;
    fn neg(self) -> Self {
        Self {
            seconds: -self.seconds,
        }
    }
}

impl AddAssign for RelativeTime {
    fn add_assign(&mut self, rhs: Self) {
        self.seconds += rhs.seconds;
    }
}

impl SubAssign for RelativeTime {
    fn sub_assign(&mut self, rhs: Self) {
        self.seconds -= rhs.seconds;
    }
}

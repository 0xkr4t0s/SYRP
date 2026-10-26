// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Text forms of [`Time`]: ISO 8601 and strftime-style formatting.

use std::fmt::Write as _;

use crate::time::days_from_civil;
use crate::{DateTime, SysError, Time};

const DAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Writes `offset` seconds as `+hh:mm` (or `+hhmm` without the colon).
fn push_offset(out: &mut String, offset: i32, colon: bool) {
    let sign = if offset < 0 { '-' } else { '+' };
    let mins = offset.unsigned_abs() / 60;
    let sep = if colon { ":" } else { "" };
    let _ = write!(out, "{sign}{:02}{sep}{:02}", mins / 60, mins % 60);
}

impl Time {
    /// This time as ISO 8601 in local time with its UTC offset, to the millisecond:
    /// `2024-01-31T12:34:56.789+01:00` with separators, `20240131T123456.789+0100` without.
    ///
    /// Years outside 0000-9999 are written with as many digits as they need, and do not
    /// parse back.
    ///
    /// ```
    /// # use syrp_sys::Time;
    /// let t = Time::from_iso8601("2024-01-31T12:34:56.789+01:00").unwrap();
    /// assert_eq!(Time::from_iso8601(&t.to_iso8601(true)).unwrap(), t);
    /// assert_eq!(Time::from_iso8601(&t.to_iso8601(false)).unwrap(), t);
    /// ```
    #[doc(alias = "toISO8601")]
    #[must_use]
    pub fn to_iso8601(self, separators: bool) -> String {
        let d = self.local();
        let (ds, ts) = if separators { ("-", ":") } else { ("", "") };
        let mut out = format!(
            "{:04}{ds}{:02}{ds}{:02}T{:02}{ts}{:02}{ts}{:02}.{:03}",
            d.year(),
            d.month() + 1,
            d.day_of_month(),
            d.hours(),
            d.minutes(),
            d.seconds(),
            d.milliseconds()
        );
        push_offset(&mut out, d.utc_offset_seconds(), separators);
        out
    }

    /// Parses ISO 8601 text such as `2024-01-31`, `2024-01-31T12:34`,
    /// `2024-01-31T12:34:56.789Z`, `20240131T123456+0100` or `2024-01-31T12:34:56-05:00`.
    ///
    /// Accepted: a four-digit year, month and day, with or without `-` between them; then
    /// optionally `T` and hours and minutes, with optional seconds and a fraction after `.` or
    /// `,` (read to the millisecond; digits past the third are truncated, not rounded), with or without `:` between them;
    /// then optionally `Z` or an offset `+hh`, `+hhmm` or `+hh:mm` (or `-`). Text without an
    /// offset is local time; a date without a time is midnight.
    ///
    /// # Errors
    /// [`SysError::InvalidIso8601`] (`SYRP-SYS-0001`) for any other text, or for fields out
    /// of range (a day the month does not have, hour 24, minute 60, or a leap second `:60`, which
    /// is rejected rather than folded into the next minute).
    ///
    /// ```
    /// # use syrp_sys::Time;
    /// let t = Time::from_iso8601("1970-01-02T00:00:01.5Z").unwrap();
    /// assert_eq!(t.to_millis(), 86_401_500);
    /// assert!(Time::from_iso8601("2023-02-29").is_err());
    /// ```
    #[doc(alias = "fromISO8601")]
    pub fn from_iso8601(text: &str) -> Result<Self, SysError> {
        Parser {
            s: text.as_bytes(),
            pos: 0,
        }
        .parse()
    }

    /// This time in local time, formatted with strftime-style specifiers (C locale).
    ///
    /// `%a`/`%A` weekday (`Mon`/`Monday`), `%b`/`%B` month (`Jan`/`January`), `%d` day
    /// 01-31, `%H` hour 00-23, `%I` hour 01-12, `%j` day of year 001-366, `%m` month 01-12,
    /// `%M` minute, `%p` `AM`/`PM`, `%S` second, `%U` week of the year from the first Sunday
    /// (00-53), `%W` the same from the first Monday, `%w` weekday 0-6 from Sunday, `%y` year
    /// 00-99, `%Y` full year, `%c` as `%a %b %d %H:%M:%S %Y`, `%x` as `%m/%d/%y`, `%X` as
    /// `%H:%M:%S`, `%z` and `%Z` the UTC offset as `+hhmm` (no zone names), `%%` a percent
    /// sign. Any other `%` sequence is copied unchanged.
    ///
    /// ```
    /// # use syrp_sys::Time;
    /// let t = Time::from_components(2024, 0, 31, 0, 0, 0, 0, true);
    /// assert_eq!(t.formatted("%A %d %B %Y, %I %p (day %j) 100%%"),
    ///            "Wednesday 31 January 2024, 12 AM (day 031) 100%");
    /// ```
    #[must_use]
    pub fn formatted(self, format: &str) -> String {
        let d = self.local();
        let mut out = String::with_capacity(format.len() + 16);
        let mut chars = format.chars();
        while let Some(c) = chars.next() {
            if c != '%' {
                out.push(c);
                continue;
            }
            match chars.next() {
                Some(spec) if !push_spec(&mut out, d, spec) => {
                    out.push('%');
                    out.push(spec);
                }
                Some(_) => {}
                None => out.push('%'),
            }
        }
        out
    }
}

/// Appends the expansion of `%spec`; returns false when `spec` is not a known specifier.
fn push_spec(out: &mut String, d: DateTime, spec: char) -> bool {
    let wday = d.day_of_week();
    let yday = d.day_of_year();
    let hour12 = (d.hours() + 11) % 12 + 1;
    let _ = match spec {
        'a' => write!(out, "{}", &DAYS[wday as usize][..3]),
        'A' => write!(out, "{}", DAYS[wday as usize]),
        'b' => write!(out, "{}", &MONTHS[d.month() as usize][..3]),
        'B' => write!(out, "{}", MONTHS[d.month() as usize]),
        'c' => {
            for (i, part) in ['a', 'b', 'd', 'X', 'Y'].into_iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                push_spec(out, d, part);
            }
            Ok(())
        }
        'd' => write!(out, "{:02}", d.day_of_month()),
        'H' => write!(out, "{:02}", d.hours()),
        'I' => write!(out, "{hour12:02}"),
        'j' => write!(out, "{:03}", yday + 1),
        'm' => write!(out, "{:02}", d.month() + 1),
        'M' => write!(out, "{:02}", d.minutes()),
        'p' => write!(out, "{}", if d.hours() < 12 { "AM" } else { "PM" }),
        'S' => write!(out, "{:02}", d.seconds()),
        'U' => write!(out, "{:02}", (yday + 7 - wday) / 7),
        'W' => write!(out, "{:02}", (yday + 7 - (wday + 6) % 7) / 7),
        'w' => write!(out, "{wday}"),
        'x' => write!(
            out,
            "{:02}/{:02}/{:02}",
            d.month() + 1,
            d.day_of_month(),
            d.year().rem_euclid(100)
        ),
        'X' => write!(
            out,
            "{:02}:{:02}:{:02}",
            d.hours(),
            d.minutes(),
            d.seconds()
        ),
        'y' => write!(out, "{:02}", d.year().rem_euclid(100)),
        'Y' => write!(out, "{}", d.year()),
        'z' | 'Z' => {
            push_offset(out, d.utc_offset_seconds(), false);
            Ok(())
        }
        '%' => write!(out, "%"),
        _ => return false,
    };
    true
}

/// A cursor over ISO 8601 text.
struct Parser<'a> {
    s: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn fail<T>(&self, reason: &'static str) -> Result<T, SysError> {
        Err(SysError::InvalidIso8601 {
            position: self.pos,
            reason,
        })
    }

    /// Consumes `b` if it is next.
    fn eat(&mut self, b: u8) -> bool {
        let hit = self.s.get(self.pos) == Some(&b);
        self.pos += usize::from(hit);
        hit
    }

    fn peek_digit(&self) -> bool {
        self.s.get(self.pos).is_some_and(u8::is_ascii_digit)
    }

    /// Reads exactly `n` digits as a number in `lo..=hi`.
    fn num(&mut self, n: usize, lo: i64, hi: i64, what: &'static str) -> Result<i64, SysError> {
        let digits = self
            .s
            .get(self.pos..self.pos + n)
            .filter(|d| d.iter().all(u8::is_ascii_digit));
        let Some(digits) = digits else {
            return self.fail(what);
        };
        let v = digits.iter().fold(0, |v, d| v * 10 + i64::from(d - b'0'));
        if !(lo..=hi).contains(&v) {
            return self.fail(what);
        }
        self.pos += n;
        Ok(v)
    }

    fn parse(mut self) -> Result<Time, SysError> {
        let year = self.num(4, 0, 9999, "expected a four-digit year")?;
        let dash = self.eat(b'-');
        let month = self.num(2, 1, 12, "expected a month 01-12")?;
        if dash && !self.eat(b'-') {
            return self.fail("expected '-' before the day");
        }
        let next_month = if month == 12 {
            days_from_civil(year + 1, 1, 1)
        } else {
            days_from_civil(year, month + 1, 1)
        };
        let month_len = next_month - days_from_civil(year, month, 1);
        let day = self.num(2, 1, month_len, "expected a day that exists in that month")?;
        let (mut h, mut m, mut sec, mut ms) = (0, 0, 0, 0);
        let mut zone = None;
        if self.eat(b'T') {
            h = self.num(2, 0, 23, "expected an hour 00-23")?;
            let colon = self.eat(b':');
            m = self.num(2, 0, 59, "expected a minute 00-59")?;
            if (colon && self.eat(b':')) || (!colon && self.peek_digit()) {
                sec = self.num(2, 0, 59, "expected a second 00-59")?;
                if self.eat(b'.') || self.eat(b',') {
                    if !self.peek_digit() {
                        return self.fail("expected digits after the decimal mark");
                    }
                    let mut scale = 100;
                    while self.peek_digit() {
                        ms += i64::from(self.s[self.pos] - b'0') * scale;
                        scale /= 10;
                        self.pos += 1;
                    }
                }
            }
            if self.eat(b'Z') {
                zone = Some(0);
            } else if self
                .s
                .get(self.pos)
                .is_some_and(|&c| c == b'+' || c == b'-')
            {
                let sign = if self.s[self.pos] == b'-' { -1 } else { 1 };
                self.pos += 1;
                let oh = self.num(2, 0, 23, "expected an offset hour 00-23")?;
                let colon = self.eat(b':');
                let om = if colon || self.peek_digit() {
                    self.num(2, 0, 59, "expected offset minutes 00-59")?
                } else {
                    0
                };
                zone = Some(sign * (oh * 60 + om) * 60);
            }
        }
        if self.pos != self.s.len() {
            return self.fail("unexpected text after the date and time");
        }
        // Every field was range-checked above, so these narrowings are lossless.
        let fields = [year, month - 1, day, h, m, sec, ms].map(|v| v as i32);
        let [y, mo, d, h, mi, s, ms] = fields;
        Ok(match zone {
            None => Time::from_components(y, mo, d, h, mi, s, ms, true),
            Some(off) => {
                Time::from_components(y, mo, d, h, mi, s, ms, false)
                    - crate::RelativeTime::seconds(off as f64)
            }
        })
    }
}

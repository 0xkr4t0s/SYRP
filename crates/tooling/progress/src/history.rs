// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Merged PRs from the first-parent git history (spec B3).

use std::path::Path;
use std::process::Command;

use serde::Serialize;

use crate::{ProgressError, wp_prefix};

/// One PR merged to the branch, read from its merge or squash commit.
///
/// ```
/// let pr = syrp_progress::MergedPr { n: 41, t: "2026-10-26T13:40:00Z".into(), title: "board".into(), wp: None };
/// assert_eq!(pr.n, 41);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MergedPr {
    /// PR number.
    pub n: u32,
    /// Commit time, UTC, `YYYY-MM-DDTHH:MM:SSZ`.
    pub t: String,
    /// Commit subject without the ` (#n)` suffix.
    pub title: String,
    /// The WP id from a leading `[WP-...]`, if any.
    pub wp: Option<String>,
}

/// Separator between the date and the subject in the log format.
const SEP: char = '\u{1f}';

/// Parses `git log --reverse --first-parent --format=%cI%x1f%s` output.
///
/// Lines whose subject does not end in `(#<n>)` are skipped. Times are converted to UTC.
///
/// ```
/// let log = "2026-10-27T00:12:38+11:00\u{1f}[WP-0.2-02] contracts: freeze (#36)\n\
///            2026-10-26T20:00:00+11:00\u{1f}Initial commit\n";
/// let prs = syrp_progress::parse_history(log);
/// assert_eq!(prs.len(), 1);
/// assert_eq!(prs[0].t, "2026-10-26T13:12:38Z");
/// assert_eq!(prs[0].wp.as_deref(), Some("WP-0.2-02"));
/// ```
pub fn parse_history(log: &str) -> Vec<MergedPr> {
    log.lines()
        .filter_map(|line| {
            let (date, subject) = line.split_once(SEP)?;
            let subject = subject.trim_end();
            let open = subject.rfind(" (#")?;
            let n: u32 = subject[open + 3..].strip_suffix(')')?.parse().ok()?;
            let title = subject[..open].to_string();
            Some(MergedPr {
                n,
                t: to_utc(date.trim())?,
                wp: wp_prefix(&title).map(str::to_string),
                title,
            })
        })
        .collect()
}

/// Converts an RFC 3339 time with any offset to UTC with a `Z` suffix.
fn to_utc(s: &str) -> Option<String> {
    let num = |a: usize, b: usize| s.get(a..b)?.parse::<i64>().ok();
    let (y, mo, d, h, mi, sec) = (
        num(0, 4)?,
        num(5, 7)?,
        num(8, 10)?,
        num(11, 13)?,
        num(14, 16)?,
        num(17, 19)?,
    );
    let offset = match s.get(19..)? {
        "Z" | "+00:00" | "-00:00" => 0,
        o if o.len() == 6 => {
            let sign = if o.starts_with('-') { -1 } else { 1 };
            sign * (o[1..3].parse::<i64>().ok()? * 60 + o[4..6].parse::<i64>().ok()?)
        }
        _ => return None,
    };
    let mins = days_from_civil(y, mo, d) * 1440 + h * 60 + mi - offset;
    let (days, m) = (mins.div_euclid(1440), mins.rem_euclid(1440));
    let (y, mo, d) = civil_from_days(days);
    Some(format!(
        "{y:04}-{mo:02}-{d:02}T{:02}:{:02}:{sec:02}Z",
        m / 60,
        m % 60
    ))
}

// Proleptic Gregorian day counts relative to 1970-01-01, using 400-year eras.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

fn git(root: &Path, args: &[&str]) -> Result<String, ProgressError> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| ProgressError::Git {
            detail: e.to_string(),
        })?;
    if !out.status.success() {
        return Err(ProgressError::Git {
            detail: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The short HEAD hash and the merged PRs, oldest first.
pub(crate) fn read(root: &Path) -> Result<(String, Vec<MergedPr>), ProgressError> {
    let commit = git(root, &["rev-parse", "--short", "HEAD"])?
        .trim()
        .to_string();
    let log = git(
        root,
        &["log", "--reverse", "--first-parent", "--format=%cI%x1f%s"],
    )?;
    Ok((commit, parse_history(&log)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b3_utc_conversion_crosses_day_and_year() {
        assert_eq!(
            to_utc("2027-01-01T05:30:00+11:00").as_deref(),
            Some("2026-12-31T18:30:00Z")
        );
        assert_eq!(
            to_utc("2026-02-28T22:00:00-03:00").as_deref(),
            Some("2026-03-01T01:00:00Z")
        );
        assert_eq!(to_utc("garbage"), None);
    }
}

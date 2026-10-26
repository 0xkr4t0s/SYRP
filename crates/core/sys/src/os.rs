// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The only platform-specific code in the crate: the monotonic clock and the local UTC offset.
//!
//! Linux and Apple use `clock_gettime(CLOCK_MONOTONIC)`, Windows the performance counter; other
//! targets fall back to `std::time::Instant` measured from first use. Local time uses the C
//! library's time-zone database (`localtime_r`, or `localtime_s` on Windows); other targets
//! report UTC.

/// The monotonic clock in ticks of [`ticks_per_second`]. Lock-free and allocation-free.
#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
#[allow(
    clippy::useless_conversion,
    reason = "time_t and c_long are 32-bit on some targets"
)]
pub(crate) fn ticks() -> i64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `ts` is a live, aligned, non-null local the call only writes to, and
    // CLOCK_MONOTONIC is supported on every target this branch compiles for.
    let rc = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    debug_assert_eq!(rc, 0, "clock_gettime(CLOCK_MONOTONIC) failed");
    i64::from(ts.tv_sec)
        .wrapping_mul(1_000_000_000)
        .wrapping_add(i64::from(ts.tv_nsec))
}

/// Tick rate of [`ticks`].
#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
pub(crate) fn ticks_per_second() -> i64 {
    1_000_000_000
}

/// The monotonic clock in ticks of [`ticks_per_second`]. Lock-free and allocation-free.
#[cfg(windows)]
pub(crate) fn ticks() -> i64 {
    let mut count = 0_i64;
    // SAFETY: `count` is a live, aligned, non-null local the call only writes to. The call
    // cannot fail on Windows XP or later.
    unsafe { windows_sys::Win32::System::Performance::QueryPerformanceCounter(&mut count) };
    count
}

/// Tick rate of [`ticks`].
#[cfg(windows)]
pub(crate) fn ticks_per_second() -> i64 {
    let mut freq = 0_i64;
    // SAFETY: `freq` is a live, aligned, non-null local the call only writes to. The call
    // cannot fail on Windows XP or later.
    unsafe { windows_sys::Win32::System::Performance::QueryPerformanceFrequency(&mut freq) };
    freq.max(1)
}

/// Fallback clock: nanoseconds since the first call in this process.
#[cfg(not(any(
    windows,
    target_os = "linux",
    target_os = "android",
    target_vendor = "apple"
)))]
pub(crate) fn ticks() -> i64 {
    static BASE: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let base = *BASE.get_or_init(std::time::Instant::now);
    i64::try_from(base.elapsed().as_nanos()).unwrap_or(i64::MAX)
}

/// Tick rate of [`ticks`].
#[cfg(not(any(
    windows,
    target_os = "linux",
    target_os = "android",
    target_vendor = "apple"
)))]
pub(crate) fn ticks_per_second() -> i64 {
    1_000_000_000
}

/// Local time minus UTC, in seconds, at `utc_seconds` after the epoch; `None` when the OS
/// cannot convert that instant.
#[cfg(any(unix, windows))]
pub(crate) fn local_offset_seconds(utc_seconds: i64) -> Option<i32> {
    let t = libc::time_t::try_from(utc_seconds).ok()?;
    // SAFETY: `libc::tm` is plain C data (integers, and on some Unixes a nullable pointer), so
    // the all-zero bit pattern is a valid value.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    #[cfg(unix)]
    {
        // SAFETY: `t` and `tm` are live, aligned, non-null locals; `localtime_r` is the
        // re-entrant form, reads only `t` and writes only `tm`.
        let out = unsafe { libc::localtime_r(&t, &mut tm) };
        if out.is_null() {
            return None;
        }
    }
    #[cfg(windows)]
    {
        // SAFETY: `t` and `tm` are live, aligned, non-null locals; `localtime_s` reads only
        // `t` and writes only `tm`.
        if unsafe { libc::localtime_s(&mut tm, &t) } != 0 {
            return None;
        }
    }
    let days = crate::time::days_from_civil(
        i64::from(tm.tm_year) + 1900,
        i64::from(tm.tm_mon) + 1,
        i64::from(tm.tm_mday),
    );
    let local = days * 86_400
        + i64::from(tm.tm_hour) * 3600
        + i64::from(tm.tm_min) * 60
        + i64::from(tm.tm_sec);
    i32::try_from(local - utc_seconds).ok()
}

/// Fallback: no time-zone database, so local time is UTC.
#[cfg(not(any(unix, windows)))]
pub(crate) fn local_offset_seconds(_utc_seconds: i64) -> Option<i32> {
    None
}

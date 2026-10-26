// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Platform queries behind [`SystemStats`](crate::SystemStats): OS version, CPU description
//! and user and machine names. Each returns `None` when the platform does not say.

/// Leading `major[.minor[.patch]]` numbers of a version string such as `6.8.0-45-generic`.
#[cfg(unix)]
pub(crate) fn parse_version(text: &str) -> Option<[u32; 3]> {
    let mut out = [0; 3];
    let mut parts = text.split('.');
    for (i, slot) in out.iter_mut().enumerate() {
        let Some(part) = parts.next() else { break };
        let digits = part
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(part.len());
        match part[..digits].parse() {
            Ok(n) => *slot = n,
            Err(_) if i > 0 => break,
            Err(_) => return None,
        }
        if digits < part.len() {
            break;
        }
    }
    Some(out)
}

/// Reads a NUL-terminated C string the OS just wrote into `buf`.
#[cfg(unix)]
fn c_text(buf: &[u8]) -> Option<String> {
    let s = std::ffi::CStr::from_bytes_until_nul(buf)
        .ok()?
        .to_string_lossy();
    (!s.is_empty()).then(|| s.into_owned())
}

/// The value of the `sysctl` named `name` (which must end in a NUL), into `out`.
#[cfg(target_vendor = "apple")]
fn sysctl<T: ?Sized>(name: &std::ffi::CStr, out: &mut T) -> Option<usize> {
    let mut len = std::mem::size_of_val(out);
    // SAFETY: `name` is NUL-terminated; `out` is live, aligned, non-null and `len` bytes
    // long, and the call writes at most `len` bytes to it; no new value is set (null, 0).
    let rc = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            (out as *mut T).cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    (rc == 0).then_some(len)
}

/// The OS version: kernel release on Linux, product version on macOS, NT version on Windows.
#[cfg(target_vendor = "apple")]
pub(crate) fn os_version() -> Option<[u32; 3]> {
    let mut buf = [0_u8; 64];
    sysctl(c"kern.osproductversion", &mut buf[..])?;
    parse_version(&c_text(&buf)?)
}

/// The OS version: kernel release on Linux, product version on macOS, NT version on Windows.
#[cfg(all(unix, not(target_vendor = "apple")))]
pub(crate) fn os_version() -> Option<[u32; 3]> {
    // SAFETY: `utsname` is plain C character arrays, so all-zero is a valid value.
    let mut uts: libc::utsname = unsafe { std::mem::zeroed() };
    // SAFETY: `uts` is a live, aligned, non-null local of the type `uname` fills.
    if unsafe { libc::uname(&mut uts) } != 0 {
        return None;
    }
    let release: Vec<u8> = uts.release.iter().map(|&c| c.to_ne_bytes()[0]).collect();
    parse_version(&c_text(&release)?)
}

/// The OS version: kernel release on Linux, product version on macOS, NT version on Windows.
#[cfg(windows)]
pub(crate) fn os_version() -> Option<[u32; 3]> {
    use windows_sys::Win32::System::SystemInformation::OSVERSIONINFOW;
    // SAFETY: OSVERSIONINFOW is plain C data, so all-zero is a valid value.
    let mut info: OSVERSIONINFOW = unsafe { std::mem::zeroed() };
    info.dwOSVersionInfoSize = std::mem::size_of::<OSVERSIONINFOW>() as u32;
    // SAFETY: `info` is a live, aligned, non-null local with its size field set, as
    // `RtlGetVersion` requires; it only writes to it. Unlike GetVersionEx it is not
    // affected by application manifests.
    let status = unsafe { windows_sys::Wdk::System::SystemServices::RtlGetVersion(&mut info) };
    (status == 0).then_some([info.dwMajorVersion, info.dwMinorVersion, info.dwBuildNumber])
}

/// Text of `/proc/cpuinfo` (Linux only).
#[cfg(any(target_os = "linux", target_os = "android"))]
fn cpuinfo() -> String {
    std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default()
}

/// The number of distinct physical cores.
#[cfg(any(target_os = "linux", target_os = "android"))]
pub(crate) fn physical_cpus() -> Option<usize> {
    let mut cores = std::collections::BTreeSet::new();
    let mut package = "";
    for line in cpuinfo().lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        match key.trim() {
            "physical id" => package = value.trim(),
            "core id" => _ = cores.insert(format!("{package}/{}", value.trim())),
            _ => {}
        }
    }
    (!cores.is_empty()).then_some(cores.len())
}

/// The number of distinct physical cores.
#[cfg(target_vendor = "apple")]
pub(crate) fn physical_cpus() -> Option<usize> {
    let mut n = 0_i32;
    sysctl(c"hw.physicalcpu", &mut n)?;
    usize::try_from(n).ok().filter(|&n| n > 0)
}

/// The number of distinct physical cores.
#[cfg(windows)]
pub(crate) fn physical_cpus() -> Option<usize> {
    use windows_sys::Win32::System::SystemInformation::{
        GetLogicalProcessorInformation, RelationProcessorCore,
        SYSTEM_LOGICAL_PROCESSOR_INFORMATION as Info,
    };
    let mut bytes = 0_u32;
    // SAFETY: a null buffer with a zero length only asks for the needed length in `bytes`.
    unsafe { GetLogicalProcessorInformation(std::ptr::null_mut(), &mut bytes) };
    let count = bytes as usize / std::mem::size_of::<Info>();
    // SAFETY: `Info` is plain C data (integers and a union of integers); all-zero is valid.
    let mut infos = vec![unsafe { std::mem::zeroed::<Info>() }; count];
    // SAFETY: `infos` is a live, aligned, non-null buffer of `bytes` bytes (`count` whole
    // entries), and the call writes at most `bytes` bytes to it.
    if unsafe { GetLogicalProcessorInformation(infos.as_mut_ptr(), &mut bytes) } == 0 {
        return None;
    }
    let used = &infos[..(bytes as usize / std::mem::size_of::<Info>()).min(count)];
    let cores = used
        .iter()
        .filter(|i| i.Relationship == RelationProcessorCore)
        .count();
    (cores > 0).then_some(cores)
}

/// The CPU vendor id and brand string from `cpuid`.
#[cfg(target_arch = "x86_64")]
pub(crate) fn cpu_vendor_and_model() -> (Option<String>, Option<String>) {
    use std::arch::x86_64::__cpuid;
    let text = |words: &[u32]| {
        let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        let s = String::from_utf8_lossy(&bytes)
            .trim_matches(['\0', ' '])
            .to_owned();
        (!s.is_empty()).then_some(s)
    };
    let leaf0 = __cpuid(0);
    let vendor = text(&[leaf0.ebx, leaf0.edx, leaf0.ecx]);
    let model = (__cpuid(0x8000_0000).eax >= 0x8000_0004).then(|| {
        let words: Vec<u32> = (0x8000_0002..=0x8000_0004)
            .map(__cpuid)
            .flat_map(|r| [r.eax, r.ebx, r.ecx, r.edx])
            .collect();
        text(&words)
    });
    (vendor, model.flatten())
}

/// The CPU vendor and model, where the OS describes them.
#[cfg(not(target_arch = "x86_64"))]
pub(crate) fn cpu_vendor_and_model() -> (Option<String>, Option<String>) {
    #[cfg(target_vendor = "apple")]
    {
        let mut buf = [0_u8; 128];
        let model = sysctl(c"machdep.cpu.brand_string", &mut buf[..]).and(c_text(&buf));
        let vendor = model
            .as_ref()
            .filter(|m| m.starts_with("Apple"))
            .map(|_| "Apple".into());
        (vendor, model)
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let info = cpuinfo();
        let field = |name: &str| {
            info.lines()
                .filter_map(|l| l.split_once(':'))
                .find(|(k, _)| k.trim() == name)
                .map(|(_, v)| v.trim().to_owned())
                .filter(|v| !v.is_empty())
        };
        (
            field("vendor_id"),
            field("model name").or_else(|| field("Hardware")),
        )
    }
    #[cfg(not(any(target_vendor = "apple", target_os = "linux", target_os = "android")))]
    {
        (None, None)
    }
}

/// The nominal (maximum) CPU clock in MHz.
#[cfg(any(target_os = "linux", target_os = "android"))]
pub(crate) fn cpu_speed_mhz() -> Option<u32> {
    let khz = std::fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq");
    if let Some(khz) = khz.ok().and_then(|t| t.trim().parse::<u32>().ok()) {
        return Some(khz / 1000).filter(|&m| m > 0);
    }
    let info = cpuinfo();
    let line = info.lines().find(|l| l.starts_with("cpu MHz"))?;
    let mhz: f64 = line.split_once(':')?.1.trim().parse().ok()?;
    Some(mhz.round() as u32).filter(|&m| m > 0)
}

/// The nominal (maximum) CPU clock in MHz.
#[cfg(target_vendor = "apple")]
pub(crate) fn cpu_speed_mhz() -> Option<u32> {
    let mut hz = 0_u64;
    sysctl(c"hw.cpufrequency", &mut hz)?; // Not reported on Apple silicon.
    u32::try_from(hz / 1_000_000).ok().filter(|&m| m > 0)
}

/// UTF-16 with a terminating NUL, for Windows calls.
#[cfg(windows)]
pub(crate) fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Calls a Windows `Get...NameW`-style function with a 256-character buffer.
#[cfg(windows)]
fn wide_query(call: impl FnOnce(*mut u16, &mut u32) -> bool) -> Option<String> {
    let mut buf = [0_u16; 257];
    let mut len = 257_u32;
    if !call(buf.as_mut_ptr(), &mut len) {
        return None;
    }
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let s = String::from_utf16_lossy(&buf[..end]);
    (!s.is_empty()).then_some(s)
}

/// The nominal (maximum) CPU clock in MHz.
#[cfg(windows)]
pub(crate) fn cpu_speed_mhz() -> Option<u32> {
    use windows_sys::Win32::System::Registry::{
        HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RegGetValueW,
    };
    let key = wide(r"HARDWARE\DESCRIPTION\System\CentralProcessor\0");
    let value = wide("~MHz");
    let (mut mhz, mut size) = (0_u32, 4_u32);
    // SAFETY: `key` and `value` are NUL-terminated UTF-16; `mhz` is a live, aligned 4-byte
    // local and `size` says so; the type pointer may be null; the call writes at most 4 bytes.
    let rc = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&raw mut mhz).cast(),
            &mut size,
        )
    };
    (rc == 0 && mhz > 0).then_some(mhz)
}

/// `(login name, full name)` of the effective user from the password database.
#[cfg(unix)]
pub(crate) fn user_names() -> (Option<String>, Option<String>) {
    let mut buf = vec![0_u8; 4096];
    loop {
        // SAFETY: `passwd` is integers and nullable pointers, so all-zero is a valid value.
        let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
        let mut found: *mut libc::passwd = std::ptr::null_mut();
        // SAFETY: `geteuid` cannot fail. `pwd`, `found` and `buf` are live, aligned and
        // non-null, `buf.len()` is the buffer's true length, and the call writes only to them.
        let rc = unsafe {
            libc::getpwuid_r(
                libc::geteuid(),
                &mut pwd,
                buf.as_mut_ptr().cast(),
                buf.len(),
                &mut found,
            )
        };
        if rc == libc::ERANGE && buf.len() < 1 << 20 {
            buf.resize(buf.len() * 4, 0);
            continue;
        }
        if rc != 0 || found.is_null() {
            return (None, None);
        }
        let read = |p: *const libc::c_char| {
            // SAFETY: on success both fields point into `buf` at NUL-terminated strings (or
            // are null, checked first), and `buf` outlives this closure.
            let s =
                (!p.is_null()).then(|| unsafe { std::ffi::CStr::from_ptr(p) }.to_string_lossy());
            s.map(|s| s.split(',').next().unwrap_or_default().trim().to_owned())
                .filter(|s| !s.is_empty())
        };
        return (read(pwd.pw_name), read(pwd.pw_gecos));
    }
}

/// `(login name, full name)` of the current user.
#[cfg(windows)]
pub(crate) fn user_names() -> (Option<String>, Option<String>) {
    use windows_sys::Win32::Security::Authentication::Identity::{GetUserNameExW, NameDisplay};
    use windows_sys::Win32::System::WindowsProgramming::GetUserNameW;
    // SAFETY: `p` is a live buffer of `len` UTF-16 units; the call writes at most that many.
    let login = wide_query(|p, len| unsafe { GetUserNameW(p, len) } != 0);
    // SAFETY: as above.
    let full = wide_query(|p, len| unsafe { GetUserNameExW(NameDisplay, p, len) });
    (login, full)
}

/// The machine's host name.
#[cfg(unix)]
pub(crate) fn computer_name() -> Option<String> {
    let mut buf = [0_u8; 256];
    // SAFETY: `buf` is a live, non-null buffer and its true length is passed (minus one, so
    // the result stays NUL-terminated even if the OS truncates without terminating).
    if unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len() - 1) } != 0 {
        return None;
    }
    c_text(&buf)
}

/// The machine's host name.
#[cfg(windows)]
pub(crate) fn computer_name() -> Option<String> {
    use windows_sys::Win32::System::SystemInformation::{
        ComputerNameDnsHostname, GetComputerNameExW,
    };
    // SAFETY: `p` is a live buffer of `len` UTF-16 units; the call writes at most that many.
    wide_query(|p, len| unsafe { GetComputerNameExW(ComputerNameDnsHostname, p, len) } != 0)
}

/// Fallbacks for targets without a supported OS API.
#[cfg(not(any(unix, windows)))]
mod fallback {
    pub(crate) fn os_version() -> Option<[u32; 3]> {
        None
    }
    pub(crate) fn user_names() -> (Option<String>, Option<String>) {
        (None, None)
    }
    pub(crate) fn computer_name() -> Option<String> {
        None
    }
}
#[cfg(not(any(unix, windows)))]
pub(crate) use fallback::*;

/// Fallback: unknown on other targets.
#[cfg(not(any(
    windows,
    target_vendor = "apple",
    target_os = "linux",
    target_os = "android"
)))]
pub(crate) fn cpu_speed_mhz() -> Option<u32> {
    None
}

/// Fallback: unknown on other targets.
#[cfg(not(any(
    windows,
    target_vendor = "apple",
    target_os = "linux",
    target_os = "android"
)))]
pub(crate) fn physical_cpus() -> Option<usize> {
    None
}

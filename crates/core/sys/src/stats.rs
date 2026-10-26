// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`SystemStats`]: facts about the machine and the process environment.

use std::fmt;

use crate::{os, os_info};

/// A CPU instruction-set extension that [`SystemStats::has_cpu_feature`] can test for.
///
/// ```
/// use syrp_sys::{CpuFeature, SystemStats};
/// if SystemStats::has_cpu_feature(CpuFeature::Avx2) {
///     // Pick an AVX2 kernel.
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CpuFeature {
    /// x86 SSE.
    Sse,
    /// x86 SSE2.
    Sse2,
    /// x86 SSE3.
    Sse3,
    /// x86 SSSE3.
    Ssse3,
    /// x86 SSE4.1.
    Sse41,
    /// x86 SSE4.2.
    Sse42,
    /// x86 AVX.
    Avx,
    /// x86 AVX2.
    Avx2,
    /// x86 AVX-512 Foundation.
    Avx512F,
    /// Arm NEON (Advanced SIMD).
    Neon,
}

/// An operating-system version: up to three leading numbers, missing ones as 0.
///
/// ```
/// use syrp_sys::SystemStats;
/// if let Some(v) = SystemStats::operating_system_type().version() {
///     assert_eq!(v.to_string(), format!("{}.{}.{}", v.major(), v.minor(), v.patch()));
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OsVersion {
    parts: [u32; 3],
}

impl OsVersion {
    /// The first number (for example 14 for macOS 14.4.1, 6 for Linux 6.8.0, 10 for Windows
    /// 10 and 11).
    ///
    /// ```
    /// # use syrp_sys::SystemStats;
    /// let _major = SystemStats::operating_system_type().version().map(|v| v.major());
    /// ```
    #[must_use]
    pub const fn major(self) -> u32 {
        self.parts[0]
    }

    /// The second number.
    ///
    /// ```
    /// # use syrp_sys::SystemStats;
    /// let _minor = SystemStats::operating_system_type().version().map(|v| v.minor());
    /// ```
    #[must_use]
    pub const fn minor(self) -> u32 {
        self.parts[1]
    }

    /// The third number: the patch level, or the build number on Windows (22000 and above
    /// is Windows 11).
    ///
    /// ```
    /// # use syrp_sys::SystemStats;
    /// let _patch = SystemStats::operating_system_type().version().map(|v| v.patch());
    /// ```
    #[must_use]
    pub const fn patch(self) -> u32 {
        self.parts[2]
    }
}

impl fmt::Display for OsVersion {
    /// Writes `major.minor.patch`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [a, b, c] = self.parts;
        write!(f, "{a}.{b}.{c}")
    }
}

/// The operating system family, with its version when the OS reports one: the kernel
/// release on Linux, the product version on macOS and the NT version and build on Windows.
///
/// ```
/// use syrp_sys::{OperatingSystemType, SystemStats};
/// let os = SystemStats::operating_system_type();
/// assert_eq!(matches!(os, OperatingSystemType::Linux(_)), cfg!(target_os = "linux"));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum OperatingSystemType {
    /// Linux (including Android), with the kernel version.
    Linux(Option<OsVersion>),
    /// macOS, with the product version.
    MacOs(Option<OsVersion>),
    /// Windows, with major, minor and build numbers.
    Windows(Option<OsVersion>),
    /// Any other target.
    Other,
}

impl OperatingSystemType {
    /// The version carried by the variant, if any.
    ///
    /// ```
    /// # use syrp_sys::OperatingSystemType;
    /// assert_eq!(OperatingSystemType::Other.version(), None);
    /// ```
    #[must_use]
    pub const fn version(self) -> Option<OsVersion> {
        match self {
            Self::Linux(v) | Self::MacOs(v) | Self::Windows(v) => v,
            Self::Other => None,
        }
    }
}

/// Queries about the machine and the process environment. Every function is a cheap query
/// that may be called from any thread, but none is RT-safe.
///
/// ```
/// use syrp_sys::SystemStats;
/// assert!(SystemStats::num_cpus() >= 1);
/// assert!(SystemStats::page_size() >= 512);
/// ```
#[doc(alias = "juce::SystemStats")]
#[derive(Debug)]
#[non_exhaustive]
pub struct SystemStats;

impl SystemStats {
    /// Whether this build of the process uses 64-bit pointers.
    ///
    /// ```
    /// # use syrp_sys::SystemStats;
    /// assert_eq!(SystemStats::is_64_bit(), cfg!(target_pointer_width = "64"));
    /// ```
    #[doc(alias = "isOperatingSystem64Bit")]
    #[must_use]
    pub const fn is_64_bit() -> bool {
        cfg!(target_pointer_width = "64")
    }

    /// The number of logical CPUs this process may run on (at least 1). This follows
    /// [`std::thread::available_parallelism`], so CPU affinity masks and container quotas can
    /// make it smaller than the machine's total.
    ///
    /// ```
    /// # use syrp_sys::SystemStats;
    /// assert!(SystemStats::num_cpus() >= 1);
    /// ```
    #[doc(alias = "getNumCpus")]
    #[must_use]
    pub fn num_cpus() -> usize {
        std::thread::available_parallelism().map_or(1, usize::from)
    }

    /// Installed physical memory in megabytes (2^20 bytes), or `None` when the OS does not
    /// report it.
    ///
    /// ```
    /// # use syrp_sys::SystemStats;
    /// if let Some(mb) = SystemStats::memory_size_mb() {
    ///     assert!(mb > 0);
    /// }
    /// ```
    #[doc(alias = "getMemorySizeInMegabytes")]
    #[must_use]
    pub fn memory_size_mb() -> Option<u64> {
        os::memory_bytes().map(|b| b >> 20).filter(|&mb| mb > 0)
    }

    /// The virtual-memory page size in bytes, or 4096 when the OS does not report it.
    ///
    /// ```
    /// # use syrp_sys::SystemStats;
    /// assert!(SystemStats::page_size().is_power_of_two());
    /// ```
    #[doc(alias = "getPageSize")]
    #[must_use]
    pub fn page_size() -> usize {
        os::page_size().unwrap_or(4096)
    }

    /// Whether the CPU running this process supports `feature`. Features of another
    /// architecture (NEON on x86, SSE on Arm) report `false`. Detection is done at run time
    /// and cached by the standard library.
    ///
    /// ```
    /// # use syrp_sys::{CpuFeature, SystemStats};
    /// #[cfg(target_arch = "x86_64")]
    /// assert!(SystemStats::has_cpu_feature(CpuFeature::Sse2)); // part of x86-64
    /// #[cfg(target_arch = "aarch64")]
    /// assert!(SystemStats::has_cpu_feature(CpuFeature::Neon));
    /// ```
    #[doc(alias = "hasSSE2", alias = "hasAVX2", alias = "hasNeon")]
    #[must_use]
    pub fn has_cpu_feature(feature: CpuFeature) -> bool {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            use std::arch::is_x86_feature_detected as has;
            match feature {
                CpuFeature::Sse => has!("sse"),
                CpuFeature::Sse2 => has!("sse2"),
                CpuFeature::Sse3 => has!("sse3"),
                CpuFeature::Ssse3 => has!("ssse3"),
                CpuFeature::Sse41 => has!("sse4.1"),
                CpuFeature::Sse42 => has!("sse4.2"),
                CpuFeature::Avx => has!("avx"),
                CpuFeature::Avx2 => has!("avx2"),
                CpuFeature::Avx512F => has!("avx512f"),
                CpuFeature::Neon => false,
            }
        }
        #[cfg(target_arch = "aarch64")]
        {
            feature == CpuFeature::Neon && std::arch::is_aarch64_feature_detected!("neon")
        }
        #[cfg(not(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64")))]
        {
            let _ = feature;
            false
        }
    }

    /// The value of the environment variable `name`, or `default` when it is unset or not
    /// valid Unicode.
    ///
    /// ```
    /// # use syrp_sys::SystemStats;
    /// let v = SystemStats::environment_variable("SYRP_SURELY_UNSET_VARIABLE", "fallback");
    /// assert_eq!(v, "fallback");
    /// ```
    #[doc(alias = "getEnvironmentVariable")]
    #[must_use]
    pub fn environment_variable(name: &str, default: &str) -> String {
        std::env::var(name).unwrap_or_else(|_| default.to_owned())
    }

    /// The operating system family and version.
    ///
    /// ```
    /// # use syrp_sys::SystemStats;
    /// let os = SystemStats::operating_system_type();
    /// assert!(SystemStats::operating_system_name().len() >= 5, "{os:?}");
    /// ```
    #[doc(alias = "getOperatingSystemType")]
    #[must_use]
    pub fn operating_system_type() -> OperatingSystemType {
        let v = os_info::os_version().map(|parts| OsVersion { parts });
        if cfg!(any(target_os = "linux", target_os = "android")) {
            OperatingSystemType::Linux(v)
        } else if cfg!(target_os = "macos") {
            OperatingSystemType::MacOs(v)
        } else if cfg!(windows) {
            OperatingSystemType::Windows(v)
        } else {
            OperatingSystemType::Other
        }
    }

    /// A readable name: `Linux`, `macOS`, `Windows` or `Other`, followed by the version when
    /// known, for example `"macOS 14.4.1"` or `"Windows 10.0.22631"`. Windows 11 reports
    /// itself as 10.0 with a build of 22000 or later, so it reads as `"Windows 10.0.x"`.
    ///
    /// ```
    /// # use syrp_sys::SystemStats;
    /// let name = SystemStats::operating_system_name();
    /// assert!(["Linux", "macOS", "Windows", "Other"].iter().any(|f| name.starts_with(f)));
    /// ```
    #[doc(alias = "getOperatingSystemName")]
    #[must_use]
    pub fn operating_system_name() -> String {
        let os = Self::operating_system_type();
        let family = match os {
            OperatingSystemType::Linux(_) => "Linux",
            OperatingSystemType::MacOs(_) => "macOS",
            OperatingSystemType::Windows(_) => "Windows",
            OperatingSystemType::Other => "Other",
        };
        match os.version() {
            Some(v) => format!("{family} {v}"),
            None => family.to_owned(),
        }
    }

    /// The number of physical CPU cores, or [`SystemStats::num_cpus`] when the OS does not
    /// report cores (on Linux this reads `/proc/cpuinfo`, which some Arm kernels leave out).
    ///
    /// ```
    /// # use syrp_sys::SystemStats;
    /// assert!(SystemStats::num_physical_cpus() >= 1);
    /// ```
    #[doc(alias = "getNumPhysicalCpus")]
    #[must_use]
    pub fn num_physical_cpus() -> usize {
        os_info::physical_cpus().unwrap_or_else(Self::num_cpus)
    }

    /// The CPU vendor, such as `"GenuineIntel"` or `"AuthenticAMD"` (from `cpuid` on x86-64)
    /// or `"Apple"`; `None` when unknown.
    ///
    /// ```
    /// # use syrp_sys::SystemStats;
    /// if let Some(v) = SystemStats::cpu_vendor() { assert!(!v.is_empty()); }
    /// ```
    #[doc(alias = "getCpuVendor")]
    #[must_use]
    pub fn cpu_vendor() -> Option<String> {
        os_info::cpu_vendor_and_model().0
    }

    /// The CPU model (brand string), such as `"Apple M2"`; `None` when unknown.
    ///
    /// ```
    /// # use syrp_sys::SystemStats;
    /// if let Some(m) = SystemStats::cpu_model() { assert!(!m.is_empty()); }
    /// ```
    #[doc(alias = "getCpuModel")]
    #[must_use]
    pub fn cpu_model() -> Option<String> {
        os_info::cpu_vendor_and_model().1
    }

    /// The nominal CPU clock in MHz, best effort: `None` when unknown (always on Apple
    /// silicon and in many virtual machines).
    ///
    /// ```
    /// # use syrp_sys::SystemStats;
    /// if let Some(mhz) = SystemStats::cpu_speed_mhz() { assert!(mhz > 0); }
    /// ```
    #[doc(alias = "getCpuSpeedInMegahertz")]
    #[must_use]
    pub fn cpu_speed_mhz() -> Option<u32> {
        os_info::cpu_speed_mhz()
    }

    /// The current user's login name, from the password database (Unix) or `GetUserNameW`
    /// (Windows), falling back to `USER`, `LOGNAME` or `USERNAME`; empty when unknown.
    ///
    /// ```
    /// # use syrp_sys::SystemStats;
    /// let _name: String = SystemStats::user_login_name();
    /// ```
    #[doc(alias = "getLogonName")]
    #[must_use]
    pub fn user_login_name() -> String {
        os_info::user_names().0.unwrap_or_else(|| {
            ["USER", "LOGNAME", "USERNAME"]
                .iter()
                .find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty()))
                .unwrap_or_default()
        })
    }

    /// The current user's full name (the first field of the GECOS entry on Unix, the display
    /// name on Windows), or [`SystemStats::user_login_name`] when none is set.
    ///
    /// ```
    /// # use syrp_sys::SystemStats;
    /// let _name: String = SystemStats::full_user_name();
    /// ```
    #[doc(alias = "getFullUserName")]
    #[must_use]
    pub fn full_user_name() -> String {
        os_info::user_names()
            .1
            .unwrap_or_else(Self::user_login_name)
    }

    /// The machine's host name; empty when unknown.
    ///
    /// ```
    /// # use syrp_sys::SystemStats;
    /// let _host: String = SystemStats::computer_name();
    /// ```
    #[doc(alias = "getComputerName")]
    #[must_use]
    pub fn computer_name() -> String {
        os_info::computer_name().unwrap_or_default()
    }

    /// A text backtrace of the calling thread, one frame per line as the standard library
    /// formats it. Symbol names need debug info; release builds may show addresses only.
    ///
    /// ```
    /// # use syrp_sys::SystemStats;
    /// assert!(!SystemStats::stack_backtrace().is_empty());
    /// ```
    #[doc(alias = "getStackBacktrace")]
    #[must_use]
    pub fn stack_backtrace() -> String {
        std::backtrace::Backtrace::force_capture().to_string()
    }
}

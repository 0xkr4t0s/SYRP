// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`SystemStats`]: facts about the machine and the process environment.

use crate::os;

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
}

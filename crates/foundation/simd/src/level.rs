// SPDX-License-Identifier: MIT OR Apache-2.0
//! Runtime selection of the instruction set ([`Level`]) and the [`dispatch!`](crate::dispatch)
//! entry point.

use std::sync::OnceLock;

/// An instruction-set level that kernels and lane-generic code can be compiled for.
///
/// [`Level::detect`] picks the widest level the CPU supports once per process. `Scalar` is the
/// reference path: plain Rust with no SIMD dispatch. More levels (for example AVX-512 lane packs)
/// can be added without a breaking change, so the enum is non-exhaustive.
///
/// ```
/// use syrp_simd::Level;
/// let level = Level::detect();
/// assert!(level.is_supported());
/// assert!(Level::supported().any(|l| l == Level::Scalar));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Level {
    /// The scalar reference path. Always supported.
    Scalar,
    /// x86 SSE2 (the x86-64 baseline).
    Sse2,
    /// x86-64-v2: SSE4.2, POPCNT, CMPXCHG16B.
    Sse4_2,
    /// x86-64-v3: AVX2 and FMA.
    Avx2,
    /// Ice Lake-class AVX-512. `F32x16`/`F64x8` packs for it come in a later WP.
    Avx512,
    /// AArch64 NEON.
    Neon,
    /// WebAssembly 128-bit SIMD.
    WasmSimd128,
}

/// The environment variable read once by [`Level::detect`]; the value `scalar` forces
/// [`Level::Scalar`].
pub const ENV_VAR: &str = "SYRP_SIMD";

impl Level {
    /// Every level, narrowest first within each architecture.
    const ALL: [Level; 7] = [
        Level::Scalar,
        Level::Sse2,
        Level::Sse4_2,
        Level::Avx2,
        Level::Avx512,
        Level::Neon,
        Level::WasmSimd128,
    ];

    /// The level used by this process: the widest level the CPU supports, or `Scalar` when the
    /// `SYRP_SIMD` environment variable is `scalar` (any other value is ignored).
    ///
    /// The first call reads the environment and probes the CPU (it may allocate); later calls
    /// are one atomic load, lock-free and allocation-free.
    ///
    /// ```
    /// let level = syrp_simd::Level::detect();
    /// assert_eq!(level, syrp_simd::Level::detect());
    /// ```
    pub fn detect() -> Level {
        static LEVEL: OnceLock<Level> = OnceLock::new();
        *LEVEL.get_or_init(|| {
            let env = std::env::var_os(ENV_VAR);
            resolve(env.as_deref().and_then(|v| v.to_str()), Self::best())
        })
    }

    /// The widest level this CPU supports, ignoring `SYRP_SIMD`.
    ///
    /// ```
    /// assert!(syrp_simd::Level::best().is_supported());
    /// ```
    pub fn best() -> Level {
        Self::ALL
            .into_iter()
            .rev()
            .find(|l| l.is_supported())
            .unwrap_or(Level::Scalar)
    }

    /// Every level this CPU can run, `Scalar` first. Tests use it to compare each level with
    /// the scalar reference.
    ///
    /// ```
    /// assert_eq!(syrp_simd::Level::supported().next(), Some(syrp_simd::Level::Scalar));
    /// ```
    pub fn supported() -> impl Iterator<Item = Level> {
        Self::ALL.into_iter().filter(|l| l.is_supported())
    }

    /// Whether this CPU can run code compiled for this level.
    ///
    /// ```
    /// assert!(syrp_simd::Level::Scalar.is_supported());
    /// ```
    pub fn is_supported(self) -> bool {
        self == Level::Scalar || self.to_fearless().is_some()
    }

    /// The `fearless_simd` level to dispatch on, or `None` for [`Level::Scalar`] and for levels
    /// this CPU does not support. Used by [`dispatch!`](crate::dispatch); the CPU probe is
    /// cached, so after the first call this is an atomic load and a match.
    ///
    /// ```
    /// assert!(syrp_simd::Level::Scalar.to_fearless().is_none());
    /// ```
    pub fn to_fearless(self) -> Option<fearless_simd::Level> {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"))]
        use fearless_simd::Simd;
        static BEST: OnceLock<fearless_simd::Level> = OnceLock::new();
        let best = *BEST.get_or_init(fearless_simd::Level::new);
        match self {
            Level::Scalar => None,
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Level::Sse2 => best.as_sse2().map(|t| t.level()),
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Level::Sse4_2 => best.as_sse4_2().map(|t| t.level()),
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Level::Avx2 => best.as_avx2().map(|t| t.level()),
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            Level::Avx512 => best.as_avx512().map(|t| t.level()),
            #[cfg(target_arch = "aarch64")]
            Level::Neon => best.as_neon().map(|t| t.level()),
            #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
            Level::WasmSimd128 => best.as_wasm_simd128().map(|t| t.level()),
            #[allow(unreachable_patterns, reason = "the arms above depend on the target")]
            _ => None,
        }
    }
}

/// Applies the `SYRP_SIMD` override to the detected level.
fn resolve(env: Option<&str>, best: Level) -> Level {
    match env {
        Some(v) if v.trim().eq_ignore_ascii_case("scalar") => Level::Scalar,
        _ => best,
    }
}

/// Runs `$simd_body` compiled for `$level`'s instruction set, or `$scalar_body` for
/// [`Level::Scalar`](crate::Level::Scalar) and for levels this CPU does not support.
///
/// `$simd` binds a `fearless_simd` token (`impl fearless_simd::Simd`) that the body can use
/// for explicit vectors; lane-generic code over [`Sample`](crate::Sample) can ignore it. This
/// is the entry point from ADR 0003 D5: pick the level once, then call a monomorphised kernel.
///
/// **Only code inlined into the body is compiled for the level.** Every function on the path
/// from the body to the `Sample` operations must be `#[inline(always)]`. A function that is not
/// inlined is compiled once for the baseline and every arm calls that one copy: no 256-bit
/// instructions, and `mul_add` becomes a software `fma` call per lane.
///
/// ```
/// use syrp_simd::{dispatch, F32x8, Level, Sample};
/// #[inline(always)] // required: see above
/// fn gain<S: Sample>(xs: &mut [S], g: S) {
///     for x in xs {
///         *x *= g;
///     }
/// }
/// let mut voices = [F32x8::one(); 4];
/// dispatch!(Level::detect(), _simd => gain(&mut voices, F32x8::splat(0.5)), scalar => gain(&mut voices, F32x8::splat(0.5)));
/// assert_eq!(voices[3].lane(7), 0.5);
/// ```
#[macro_export]
macro_rules! dispatch {
    ($level:expr, $simd:pat => $simd_body:expr, scalar => $scalar_body:expr $(,)?) => {
        match $crate::Level::to_fearless($level) {
            ::core::option::Option::Some(level) => {
                $crate::fearless_simd::dispatch!(level, $simd => $simd_body)
            }
            ::core::option::Option::None => $scalar_body,
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b2_syrp_simd_scalar_forces_the_scalar_path() {
        assert_eq!(resolve(Some("scalar"), Level::Avx2), Level::Scalar);
        assert_eq!(resolve(Some(" SCALAR "), Level::Neon), Level::Scalar);
        assert_eq!(resolve(Some("avx2"), Level::Sse2), Level::Sse2);
        assert_eq!(resolve(None, Level::Avx2), Level::Avx2);
    }

    #[test]
    fn b2_best_level_matches_fearless_detection() {
        let best = Level::best();
        assert_eq!(best.to_fearless().is_some(), best != Level::Scalar);
        assert!(Level::supported().all(Level::is_supported));
        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            best == Level::Avx2,
            fearless_simd::Level::new().as_avx2().is_some()
                && fearless_simd::Level::new().as_avx512().is_none()
        );
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0
//! [`DenormalGuard`]: flush denormals to zero for a scope.

use core::marker::PhantomData;

/// Sets flush-to-zero for the current thread while it is alive and restores the previous
/// floating-point control state when dropped.
///
/// - x86_64: sets FTZ (bit 15) and DAZ (bit 6) in `MXCSR`.
/// - aarch64: sets FZ (bit 24) in `FPCR`.
/// - Every other target, and under Miri: a no-op; [`DenormalGuard::is_active`] returns `false`.
///
/// The guard changes per-thread state, so it is neither `Send` nor `Sync`; guards must be
/// dropped in reverse order of creation (normal scoping does that). Creating and dropping it
/// never allocates or locks, so it is safe on the audio thread. The Rust compiler assumes the
/// default floating-point environment when it folds constants, so only values computed at run
/// time see the flushed behaviour.
///
/// ```
/// use std::hint::black_box;
/// let tiny = black_box(f32::MIN_POSITIVE);
/// {
///     let guard = syrp_simd::DenormalGuard::new();
///     let r = black_box(tiny) * black_box(0.5);
///     if guard.is_active() {
///         assert_eq!(r, 0.0);
///     }
/// }
/// assert!(black_box(tiny) * black_box(0.5) > 0.0);
/// ```
#[derive(Debug)]
#[must_use = "the previous state is restored as soon as the guard is dropped"]
pub struct DenormalGuard {
    previous: Option<u64>,
    _not_send: PhantomData<*const ()>,
}

impl DenormalGuard {
    /// Enables flush-to-zero (and denormals-are-zero where the CPU has it) until the guard is
    /// dropped.
    ///
    /// ```
    /// let _guard = syrp_simd::DenormalGuard::new();
    /// ```
    #[inline]
    pub fn new() -> Self {
        let previous = imp::read();
        if let Some(state) = previous {
            imp::write(state | imp::FLUSH_BITS);
        }
        Self {
            previous,
            _not_send: PhantomData,
        }
    }

    /// True on targets where the guard changes the control state (x86_64 and aarch64 outside
    /// Miri), false where it is a no-op.
    ///
    /// ```
    /// let guard = syrp_simd::DenormalGuard::new();
    /// assert_eq!(guard.is_active(), cfg!(all(any(target_arch = "x86_64", target_arch = "aarch64"), not(miri))));
    /// ```
    #[inline]
    pub fn is_active(&self) -> bool {
        self.previous.is_some()
    }
}

impl Default for DenormalGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for DenormalGuard {
    #[inline]
    fn drop(&mut self) {
        if let Some(state) = self.previous {
            imp::write(state);
        }
    }
}

#[cfg(all(target_arch = "x86_64", not(miri)))]
mod imp {
    use core::arch::asm;

    /// FTZ (bit 15) and DAZ (bit 6) of MXCSR.
    pub(super) const FLUSH_BITS: u64 = (1 << 15) | (1 << 6);

    #[inline]
    pub(super) fn read() -> Option<u64> {
        let mut csr: u32 = 0;
        // SAFETY: `stmxcsr` stores the 4-byte MXCSR into `csr`, a valid, writable `u32`.
        // SSE is part of the x86_64 baseline, so the instruction always exists.
        unsafe { asm!("stmxcsr [{}]", in(reg) &mut csr, options(nostack, preserves_flags)) };
        Some(u64::from(csr))
    }

    #[inline]
    pub(super) fn write(state: u64) {
        let csr = state as u32;
        // SAFETY: `ldmxcsr` loads MXCSR from `csr`, a valid `u32`. Every value written comes
        // from `read` with only the FTZ/DAZ bits added, so no reserved bit is set (which
        // would fault), and DAZ is only set where `read` showed MXCSR is present.
        unsafe { asm!("ldmxcsr [{}]", in(reg) &csr, options(nostack, readonly, preserves_flags)) };
    }
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
mod imp {
    use core::arch::asm;

    /// FZ (bit 24) of FPCR.
    pub(super) const FLUSH_BITS: u64 = 1 << 24;

    #[inline]
    pub(super) fn read() -> Option<u64> {
        let fpcr: u64;
        // SAFETY: reading FPCR is allowed at EL0 and has no side effects.
        unsafe { asm!("mrs {}, fpcr", out(reg) fpcr, options(nomem, nostack, preserves_flags)) };
        Some(fpcr)
    }

    #[inline]
    pub(super) fn write(state: u64) {
        // SAFETY: writing FPCR is allowed at EL0; the value is a previously read FPCR with at
        // most the FZ bit added, so only rounding-control state changes.
        unsafe { asm!("msr fpcr, {}", in(reg) state, options(nomem, nostack, preserves_flags)) };
    }
}

#[cfg(not(all(any(target_arch = "x86_64", target_arch = "aarch64"), not(miri))))]
mod imp {
    pub(super) const FLUSH_BITS: u64 = 0;

    #[inline]
    pub(super) fn read() -> Option<u64> {
        None
    }

    #[inline]
    pub(super) fn write(_state: u64) {}
}

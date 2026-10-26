// SPDX-License-Identifier: MIT OR Apache-2.0
//! [`DenormalGuard`]: flush denormals to zero for a scope.

use core::marker::PhantomData;

/// Sets flush-to-zero for the current thread while it is alive and restores the previous
/// floating-point control state when dropped.
///
/// - x86_64: sets FTZ (MXCSR bit 15), and DAZ (bit 6) when the CPU reports DAZ support in
///   `MXCSR_MASK`.
/// - aarch64: sets FZ (FPCR bit 24).
/// - Every other target, and under Miri: a no-op; [`DenormalGuard::is_active`] returns `false`.
///
/// Rust assumes the default floating-point environment everywhere, so creating a guard is
/// `unsafe`: see [`DenormalGuard::new`]. ADR 0003 D5 still asks for it around every
/// `process` call, so the intent is one audited call site in the host or processor wrapper.
///
/// The guard changes per-thread state, so it is neither `Send` nor `Sync`; guards must be
/// dropped in reverse order of creation (normal scoping does that). Creating and dropping it
/// never allocates or locks, so it is [`RtSafe`](syrp_rt::RtSafe).
///
/// ```
/// use std::hint::black_box;
/// let tiny = black_box(f32::MIN_POSITIVE);
/// {
///     // SAFETY: nothing in this scope derives memory accesses from float results.
///     let guard = unsafe { syrp_simd::DenormalGuard::new() };
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

// SAFETY: `new`, `is_active` and `drop` read or write one control register (or do nothing) and
// at most load or store one atomic flag; they never allocate, lock, block or do I/O.
unsafe impl syrp_rt::RtSafe for DenormalGuard {}

impl DenormalGuard {
    /// Enables flush-to-zero (and denormals-are-zero where the CPU has it) until the guard is
    /// dropped.
    ///
    /// # Safety
    /// Rust treats running code with a non-default floating-point environment as undefined
    /// behaviour (the standard library says so for FTZ/DAZ in `_mm_setcsr`), and this guard
    /// leaves one in effect for its scope. While it is alive the compiler may still fold or
    /// reorder float operations as if denormals were kept, so float results in the scope may
    /// differ from compile-time evaluation (and between builds). The caller must make sure that,
    /// until the guard is dropped:
    ///
    /// 1. no memory safety depends on a float result: no float-derived index, length or
    ///    pointer offset reaches an unchecked access (`get_unchecked`, raw pointers, `unsafe`
    ///    code that trusts the value);
    /// 2. no other code that relies on exact IEEE denormal results (for example a library
    ///    asserting bit-exact output) runs on this thread;
    /// 3. the guard is dropped on the thread that created it, in reverse order of creation
    ///    (guaranteed by `!Send` and normal scoping; `mem::forget` leaves the mode set).
    ///
    /// ```
    /// // SAFETY: the scope only does audio arithmetic; no float result feeds an index.
    /// let _guard = unsafe { syrp_simd::DenormalGuard::new() };
    /// ```
    #[inline]
    pub unsafe fn new() -> Self {
        let previous = imp::read();
        if let Some(state) = previous {
            imp::write(state | imp::flush_bits());
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
    /// // SAFETY: no float result is used at all in this scope.
    /// let guard = unsafe { syrp_simd::DenormalGuard::new() };
    /// assert_eq!(guard.is_active(), cfg!(all(any(target_arch = "x86_64", target_arch = "aarch64"), not(miri))));
    /// ```
    #[inline]
    pub fn is_active(&self) -> bool {
        self.previous.is_some()
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
    use core::sync::atomic::{AtomicU8, Ordering};

    const FTZ: u64 = 1 << 15;
    const DAZ: u64 = 1 << 6;

    /// FTZ, plus DAZ when `MXCSR_MASK` says the CPU supports it.
    #[inline]
    pub(super) fn flush_bits() -> u64 {
        // 0 = not probed yet, 1 = no DAZ, 2 = DAZ supported.
        static DAZ_SUPPORT: AtomicU8 = AtomicU8::new(0);
        let known = match DAZ_SUPPORT.load(Ordering::Relaxed) {
            0 => {
                let v = if mxcsr_mask() & DAZ as u32 != 0 { 2 } else { 1 };
                DAZ_SUPPORT.store(v, Ordering::Relaxed);
                v
            }
            v => v,
        };
        if known == 2 { FTZ | DAZ } else { FTZ }
    }

    /// `MXCSR_MASK` from an `FXSAVE` image (bytes 28..32). Intel SDM vol. 1 §11.6.6: a stored
    /// mask of 0 means the default mask `0xFFBF`, i.e. DAZ unsupported.
    #[cold]
    fn mxcsr_mask() -> u32 {
        #[repr(C, align(16))]
        struct FxArea([u8; 512]);
        let mut area = FxArea([0; 512]);
        // SAFETY: `fxsave` writes exactly 512 bytes to a 16-byte aligned address; `area` is a
        // writable, 16-byte aligned 512-byte buffer. FXSR is part of the x86_64 baseline and the
        // instruction is allowed at user level. It only reads FPU/SSE state.
        unsafe {
            asm!("fxsave [{}]", in(reg) area.0.as_mut_ptr(), options(nostack, preserves_flags))
        };
        match u32::from_le_bytes([area.0[28], area.0[29], area.0[30], area.0[31]]) {
            0 => 0xFFBF,
            mask => mask,
        }
    }

    #[inline]
    pub(super) fn read() -> Option<u64> {
        let mut csr: u32 = 0;
        // SAFETY: `stmxcsr` stores the 4-byte MXCSR into `csr`, a valid, writable `u32`.
        // SSE is part of the x86_64 baseline, so the instruction always exists. Reading the
        // register does not change the floating-point environment.
        unsafe { asm!("stmxcsr [{}]", in(reg) &mut csr, options(nostack, preserves_flags)) };
        Some(u64::from(csr))
    }

    #[inline]
    pub(super) fn write(state: u64) {
        let csr = state as u32;
        // SAFETY: (instruction) `ldmxcsr` loads MXCSR from `csr`, a valid `u32`. Every value is
        // a value `read` returned, with at most FTZ and, only if `MXCSR_MASK` reports it, DAZ
        // added, so no unsupported bit is set (which would raise #GP).
        // (environment) this leaves a non-default FP mode in effect after the asm
        // block; that is the obligation `DenormalGuard::new` passes to its caller. Restoring
        // the saved value in `drop` returns to the caller's prior mode.
        unsafe { asm!("ldmxcsr [{}]", in(reg) &csr, options(nostack, readonly, preserves_flags)) };
    }
}

#[cfg(all(target_arch = "aarch64", not(miri)))]
mod imp {
    use core::arch::asm;

    /// FZ (bit 24) of FPCR.
    #[inline]
    pub(super) fn flush_bits() -> u64 {
        1 << 24
    }

    #[inline]
    pub(super) fn read() -> Option<u64> {
        let fpcr: u64;
        // SAFETY: reading FPCR is allowed at EL0 and has no side effects.
        unsafe { asm!("mrs {}, fpcr", out(reg) fpcr, options(nomem, nostack, preserves_flags)) };
        Some(fpcr)
    }

    #[inline]
    pub(super) fn write(state: u64) {
        // SAFETY: (instruction) writing FPCR is allowed at EL0; the value is a previously read
        // FPCR with at most the FZ bit added, which every AArch64 FPU implements.
        // (environment) this leaves a non-default FP mode in effect after the asm
        // block; that is the obligation `DenormalGuard::new` passes to its caller.
        unsafe { asm!("msr fpcr, {}", in(reg) state, options(nomem, nostack, preserves_flags)) };
    }
}

#[cfg(not(all(any(target_arch = "x86_64", target_arch = "aarch64"), not(miri))))]
mod imp {
    #[inline]
    pub(super) fn flush_bits() -> u64 {
        0
    }

    #[inline]
    pub(super) fn read() -> Option<u64> {
        None
    }

    #[inline]
    pub(super) fn write(_state: u64) {}
}

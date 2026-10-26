// SPDX-License-Identifier: MIT OR Apache-2.0
//! B4: `DenormalGuard` flushes denormals for its scope and restores the state on drop.

use std::hint::black_box;
use syrp_simd::DenormalGuard;

/// Produces a denormal result at run time from a normal input.
fn halve_min_positive() -> f32 {
    black_box(f32::MIN_POSITIVE) * black_box(0.5)
}

/// Feeds a denormal input to a multiply by one.
fn denormal_times_one() -> f64 {
    black_box(f64::MIN_POSITIVE / 4.0) * black_box(1.0)
}

#[test]
fn b4_guard_flushes_denormals_and_restores_on_drop() {
    assert!(halve_min_positive() > 0.0);
    {
        let guard = DenormalGuard::new();
        assert_eq!(
            guard.is_active(),
            cfg!(all(
                any(target_arch = "x86_64", target_arch = "aarch64"),
                not(miri)
            ))
        );
        if guard.is_active() {
            assert_eq!(halve_min_positive(), 0.0);
        }
        if cfg!(all(target_arch = "x86_64", not(miri))) {
            assert_eq!(denormal_times_one(), 0.0, "DAZ");
        }
    }
    assert!(halve_min_positive() > 0.0);
    assert!(denormal_times_one() > 0.0);
}

#[test]
fn b4_nested_guards_restore_in_order() {
    {
        let outer = DenormalGuard::new();
        {
            let _inner = DenormalGuard::default();
        }
        if outer.is_active() {
            assert_eq!(
                halve_min_positive(),
                0.0,
                "inner drop must keep the outer state"
            );
        }
    }
    assert!(halve_min_positive() > 0.0);
}

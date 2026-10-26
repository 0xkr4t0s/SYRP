// SPDX-License-Identifier: MIT OR Apache-2.0
//! B2: runtime level detection and the `dispatch!` entry point.

use syrp_simd::{F32x8, Level, Sample, dispatch};

#[inline(always)]
fn gain<S: Sample>(xs: &mut [S], g: S) {
    for x in xs {
        *x = x.mul_add(g, S::one());
    }
}

#[test]
fn b2_detect_picks_a_supported_level_once() {
    let level = Level::detect();
    assert!(level.is_supported());
    assert_eq!(level, Level::detect());
    let forced = std::env::var("SYRP_SIMD").is_ok_and(|v| v.trim().eq_ignore_ascii_case("scalar"));
    assert_eq!(
        level == Level::Scalar,
        forced || Level::best() == Level::Scalar
    );
}

#[test]
fn b2_dispatch_runs_every_supported_level_with_the_same_result() {
    let reference = {
        let mut v = [F32x8::splat(3.0); 3];
        gain(&mut v, F32x8::splat(0.5));
        v
    };
    for level in Level::supported() {
        let mut v = [F32x8::splat(3.0); 3];
        let ran_simd = dispatch!(level, _simd => { gain(&mut v, F32x8::splat(0.5)); true },
            scalar => { gain(&mut v, F32x8::splat(0.5)); false });
        assert_eq!(ran_simd, level != Level::Scalar, "{level:?}");
        assert_eq!(v, reference, "{level:?}");
    }
}

#[test]
fn b2_unsupported_levels_fall_back_to_scalar() {
    let foreign = if cfg!(target_arch = "aarch64") {
        Level::Avx2
    } else {
        Level::Neon
    };
    assert!(!foreign.is_supported());
    assert!(foreign.to_fearless().is_none());
    assert!(!dispatch!(foreign, _s => true, scalar => false));
}

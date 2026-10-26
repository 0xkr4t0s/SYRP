// SPDX-License-Identifier: MIT OR Apache-2.0
//! B7: kernels, `DenormalGuard`, `Level::detect` and lane-generic code under `dispatch!` never
//! allocate.
//! Locks are ruled out by construction: the crate has no `Mutex`, `RwLock` or blocking call,
//! and `Level::detect` is one atomic load once initialised.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use syrp_simd::{DenormalGuard, F32x8, F64x4, Level, Sample, dispatch, kernels};

struct Counting;

thread_local! {
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
}

// SAFETY: forwards to `System` unchanged; the counter is a const-initialised thread-local
// `Cell`, which never allocates.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.with(|c| c.set(c.get() + 1));
        // SAFETY: the caller's contract for `alloc` is passed through.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller's contract for `dealloc` is passed through.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

#[inline(always)]
fn ramp<S: Sample>(xs: &mut [S], step: S) -> S::Mask {
    for x in xs.iter_mut() {
        *x = x.mul_add(S::one(), step).min(S::one()).sqrt();
    }
    xs[0].simd_ge(S::one())
}

#[test]
fn b7_detect_and_dispatched_sample_code_do_not_allocate() {
    let _ = Level::detect();
    let mut f32s = [F32x8::zero(); 16];
    let mut f64s = [F64x4::zero(); 16];
    for level in Level::supported() {
        let before = ALLOCS.with(Cell::get);
        {
            let _ = Level::detect();
            dispatch!(level, _s => ramp(&mut f32s, F32x8::splat(0.25)),
                scalar => ramp(&mut f32s, F32x8::splat(0.25)));
            dispatch!(level, _s => ramp(&mut f64s, F64x4::splat(0.5)),
                scalar => ramp(&mut f64s, F64x4::splat(0.5)));
        }
        assert_eq!(ALLOCS.with(Cell::get), before, "{level:?} allocated");
    }
}

#[test]
fn b7_kernels_and_denormal_guard_do_not_allocate() {
    let mut out = vec![0.0_f32; 67];
    let (a, b) = (vec![1.5_f32; 67], vec![-0.5_f32; 67]);
    let (mut out64, a64) = (vec![0.0_f64; 13], vec![2.0_f64; 13]);
    let _ = Level::detect();
    for level in Level::supported() {
        let before = ALLOCS.with(Cell::get);
        {
            let _guard = DenormalGuard::new();
            kernels::add(level, &mut out, &a);
            kernels::add_into(level, &mut out, &a, &b);
            kernels::mul(level, &mut out, &a);
            kernels::mul_into(level, &mut out, &a, &b);
            kernels::mul_add(level, &mut out, &a, &b);
            kernels::mul_add_into(level, &mut out, &a, &b, &a);
            kernels::copy_with_gain(level, &mut out, &a, 0.5);
            assert!(kernels::find_min_max(level, &out).is_some());
            assert!(kernels::abs_max(level, &out) > 0.0);
            kernels::mul_add(level, &mut out64, &a64, &a64);
            assert!(kernels::abs_max(level, &out64) > 0.0);
        }
        assert_eq!(ALLOCS.with(Cell::get), before, "{level:?} allocated");
    }
}

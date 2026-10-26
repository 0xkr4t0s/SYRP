// SPDX-License-Identifier: MIT OR Apache-2.0
//! B1: the `Sample` trait over scalars and lane packs.

use syrp_simd::{F32x4, F32x8, F64x2, F64x4, Mask, Sample};

/// A lane-generic envelope step with a branch written as a mask.
fn step<S: Sample>(level: S, target: S, rate: S) -> S {
    let rising = level.simd_lt(target);
    let up = (level + rate).min(target);
    let down = (level - rate).max(target);
    S::select(rising, up, down)
}

fn check_generic<S: Sample<Element = E>, E: Sample<Element = E> + Into<f64>>(lanes: usize) {
    assert_eq!(S::LANES, lanes);
    assert_eq!(S::zero(), S::splat(E::zero()));
    assert_eq!(S::one() * S::splat(E::one()), S::one());
    let two = S::one() + S::one();
    assert_eq!((two * two).sqrt(), two);
    assert_eq!((-two).abs(), two);
    assert_eq!((two + two) / two - two, S::zero());
    assert!(two.simd_gt(S::one()).all() && two.simd_ge(two).all() && two.simd_le(two).all());
    assert!(two.simd_eq(S::one()).none());
    assert_eq!(step(S::zero(), two, S::one()), S::one());
    assert_eq!(step(two + two, two, S::one()), two + S::one());
    let mut acc = S::one();
    acc += two;
    acc -= S::one();
    acc *= two;
    acc /= two;
    assert_eq!(acc, two);
}

#[test]
fn b1_sample_is_implemented_for_scalars_and_every_pack() {
    check_generic::<f32, f32>(1);
    check_generic::<f64, f64>(1);
    check_generic::<F32x4, f32>(4);
    check_generic::<F32x8, f32>(8);
    check_generic::<F64x2, f64>(2);
    check_generic::<F64x4, f64>(4);
}

#[test]
fn b1_packs_expose_lanes_without_public_fields() {
    let p = F32x8::from_array([0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]);
    assert_eq!(p.lane(5), 5.0);
    assert_eq!(F32x8::from_array(p.to_array()), p);
    let m = p.simd_lt(F32x8::splat(3.0));
    assert_eq!(
        m.to_array(),
        [true, true, true, false, false, false, false, false]
    );
    assert!(m.lane(0) && !m.lane(7));
    assert_eq!((m | !m), syrp_simd::Mask32x8::splat(true));
    let sel = F32x8::select(m, F32x8::zero(), p);
    assert_eq!(sel.to_array(), [0.0, 0.0, 0.0, 3.0, 4.0, 5.0, 6.0, 7.0]);
    assert_eq!(F64x4::from_array([1.0, -2.0, 3.0, -4.0]).abs().lane(3), 4.0);
    assert_eq!(
        F64x2::from_array([1.0, 9.0])
            .max(F64x2::splat(5.0))
            .to_array(),
        [5.0, 9.0]
    );
    assert_eq!(
        F32x4::from_array([1.0, 9.0, 4.0, 6.0])
            .min(F32x4::splat(5.0))
            .lane(1),
        5.0
    );
}

#[test]
fn b1_packs_are_aligned_to_their_width() {
    assert_eq!(align_of::<F32x4>(), 16);
    assert_eq!(align_of::<F32x8>(), 32);
    assert_eq!(align_of::<F64x2>(), 16);
    assert_eq!(align_of::<F64x4>(), 32);
}

#[test]
fn b1_mul_add_is_fused_for_every_sample_type() {
    // (1 + e)^2 - (1 + 2e) is e^2 exactly; an unfused multiply-add rounds it to 0.
    let a = 1.0 + f32::EPSILON;
    let c = -(1.0 + 2.0 * f32::EPSILON);
    let exact = f32::EPSILON * f32::EPSILON;
    assert_eq!(Sample::mul_add(a, a, c), exact);
    assert_eq!(
        F32x4::splat(a).mul_add(F32x4::splat(a), F32x4::splat(c)),
        F32x4::splat(exact)
    );
    assert_eq!(
        F32x8::splat(a).mul_add(F32x8::splat(a), F32x8::splat(c)),
        F32x8::splat(exact)
    );
    let a = 1.0 + f64::EPSILON;
    let c = -(1.0 + 2.0 * f64::EPSILON);
    let exact = f64::EPSILON * f64::EPSILON;
    assert_eq!(Sample::mul_add(a, a, c), exact);
    assert_eq!(
        F64x2::splat(a).mul_add(F64x2::splat(a), F64x2::splat(c)),
        F64x2::splat(exact)
    );
    assert_eq!(
        F64x4::splat(a).mul_add(F64x4::splat(a), F64x4::splat(c)),
        F64x4::splat(exact)
    );
}

#[test]
fn b1_sample_types_are_send_sync_static() {
    fn assert_bounds<T: Copy + Send + Sync + 'static>() {}
    assert_bounds::<F32x4>();
    assert_bounds::<F32x8>();
    assert_bounds::<F64x2>();
    assert_bounds::<F64x4>();
}

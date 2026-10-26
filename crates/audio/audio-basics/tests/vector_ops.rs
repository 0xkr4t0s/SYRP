// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B5: every dispatch level of `vector_ops` agrees with the scalar reference
//! (`Level::Scalar`) on unaligned lengths and offsets. The operations round once per element
//! (or select an input), so the bound is exact equality, inside the `f32-arith` tolerance.

use proptest::prelude::*;
use syrp_audio_basics::{BufferSample, vector_ops as v};
use syrp_simd::Level;

type Op<'a, E> = &'a dyn Fn(&mut [E]);
type Outputs<E> = (Vec<Vec<E>>, [Option<E>; 2], Option<(E, E)>);

fn run<E: BufferSample>(level: Level, a: &[E], b: &[E], k: E) -> Outputs<E> {
    let ops: [Op<'_, E>; 14] = [
        &|d| v::copy_with_multiply(level, d, b, k),
        &|d| v::add_with_multiply(level, d, b, k),
        &|d| v::add(level, d, b),
        &|d| v::add_scalar(level, d, k),
        &|d| v::subtract(level, d, b),
        &|d| v::multiply(level, d, b),
        &|d| v::multiply_scalar(level, d, k),
        &|d| v::negate(level, d),
        &|d| v::abs(level, d),
        &|d| v::min(level, d, b),
        &|d| v::min_scalar(level, d, k),
        &|d| v::max(level, d, b),
        &|d| v::max_scalar(level, d, k),
        &|d| v::clip(level, d, -k.abs(), k.abs()),
    ];
    let outs = ops
        .iter()
        .map(|op| {
            let mut d = a.to_vec();
            op(&mut d);
            d
        })
        .collect();
    let extremes = [v::find_minimum(level, a), v::find_maximum(level, a)];
    (outs, extremes, v::find_min_max(level, a))
}

fn check_levels<E: BufferSample>(a: &[E], b: &[E], k: E) {
    let want = run(Level::Scalar, a, b, k);
    for level in Level::supported() {
        assert_eq!(run(level, a, b, k), want, "{level:?}, len {}", a.len());
    }
}

proptest! {
    #[test]
    fn b5_every_level_matches_the_scalar_reference(
        data in prop::collection::vec(-1.0e3_f64..1.0e3, 0..90),
        off in 0usize..9,
        k in -4.0_f64..4.0,
        fixed in prop::collection::vec(any::<i32>(), 0..40),
    ) {
        let half = data.len() / 2;
        let off = off.min(half);
        let (a, b) = (&data[off..half], &data[half..2 * half - off]);
        check_levels(a, b, k);
        let (a32, b32): (Vec<f32>, Vec<f32>) = (
            a.iter().map(|&x| x as f32).collect(),
            b.iter().map(|&x| x as f32).collect(),
        );
        check_levels(&a32, &b32, k as f32);
        let m = 1.0 / 2_147_483_648.0;
        let mut want = vec![0.0_f32; fixed.len()];
        v::convert_fixed_to_float(Level::Scalar, &mut want, &fixed, m);
        for level in Level::supported() {
            let mut got = vec![1.0_f32; fixed.len()];
            v::convert_fixed_to_float(level, &mut got, &fixed, m);
            prop_assert_eq!(&got, &want);
            prop_assert!(want.iter().zip(&fixed).all(|(&w, &x)| w == x as f32 * m));
        }
    }
}

#[test]
fn b5_reference_semantics() {
    let l = Level::detect();
    let mut d = [1.0_f32, -2.0, 3.0];
    v::clip(l, &mut d, -1.5, 1.5);
    assert_eq!(d, [1.0, -1.5, 1.5]);
    v::fill(&mut d, 0.25);
    assert_eq!(d, [0.25; 3]);
    v::copy(&mut d, &[1.0, 2.0, 3.0]);
    assert_eq!(d, [1.0, 2.0, 3.0]);
    v::clear(&mut d);
    assert_eq!(d, [0.0; 3]);
    assert_eq!(v::find_minimum::<f64>(l, &[]), None);
    assert_eq!(v::find_min_max::<f32>(l, &[]), None);
    assert_eq!(v::find_maximum(l, &[f32::NAN, 1.0]), Some(1.0));
}

#[test]
#[should_panic(expected = "equal lengths")]
fn b5_mismatched_lengths_panic() {
    v::subtract(Level::detect(), &mut [0.0_f32; 3], &[0.0; 2]);
}

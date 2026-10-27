// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Tests for work/specs/WP-2.1-10.md B6 (second part): `Matrix` and `Polynomial`.

use syrp_dsp_core::{DspError, Matrix, Polynomial};
use syrp_error::Diagnostic;
use syrp_rt::no_alloc;
use syrp_simd::F64x4;
use syrp_testkit::NoiseRng;

#[global_allocator]
static ALLOC: syrp_rt::GuardedAllocator = syrp_rt::GuardedAllocator;

fn random(rng: &mut NoiseRng, rows: usize, columns: usize) -> Matrix<f64> {
    let data: Vec<f64> = (0..rows * columns)
        .map(|_| f64::from(rng.next_bipolar(1.0)))
        .collect();
    Matrix::from_row_major(rows, columns, &data).unwrap()
}

#[test]
fn b6_matrix_builders_follow_their_definitions() {
    let i = Matrix::<f64>::identity(4).unwrap();
    for r in 0..4 {
        for c in 0..4 {
            assert_eq!(i[(r, c)], if r == c { 1.0 } else { 0.0 });
        }
    }
    let v = [5.0, 4.0, 3.0, 2.0, 1.0, 0.0, -1.0];
    let t = Matrix::toeplitz(&v, 4).unwrap();
    let h = Matrix::hankel(&v, 3, 2).unwrap();
    for r in 0..4 {
        for c in 0..4 {
            assert_eq!(t[(r, c)], v[r.abs_diff(c)]);
        }
    }
    for r in 0..3 {
        for c in 0..3 {
            assert_eq!(h[(r, c)], v[2 + r + c]);
        }
    }
    assert!(t.is_square() && !t.is_vector() && !t.is_null_matrix());
    let empty = Matrix::<f32>::new(0, 3).unwrap();
    assert!(empty.is_null_matrix());
}

#[test]
fn b6_matrix_shape_errors_are_coded() {
    let cases = [
        Matrix::from_row_major(2, 3, &[0.0_f64; 5]).unwrap_err(),
        Matrix::toeplitz(&[1.0_f64, 2.0], 3).unwrap_err(),
        Matrix::hankel(&[1.0_f64; 5], 3, 1).unwrap_err(),
    ];
    for err in cases {
        assert_eq!(err, DspError::MatrixShape);
        assert_eq!(err.code().as_str(), "SYRP-DSP-0004");
    }
    assert!(Matrix::hankel(&[1.0_f64; 6], 3, 1).is_ok());
    let err = Matrix::<f32>::new(usize::MAX, 2).unwrap_err();
    assert_eq!(err.code().as_str(), "SYRP-DSP-0001");
}

#[test]
fn b6_matrix_arithmetic_matches_element_formulas() {
    let mut rng = NoiseRng::new(11);
    let (a, b, c) = (
        random(&mut rng, 3, 4),
        random(&mut rng, 3, 4),
        random(&mut rng, 4, 2),
    );
    let sum = &a + &b;
    let diff = &a - &b;
    let scaled = &a * 2.5;
    let product = &a * &c;
    for r in 0..3 {
        for k in 0..4 {
            assert_eq!(sum[(r, k)], a[(r, k)] + b[(r, k)]);
            assert_eq!(diff[(r, k)], a[(r, k)] - b[(r, k)]);
            assert_eq!(scaled[(r, k)], a[(r, k)] * 2.5);
        }
        for col in 0..2 {
            let expected: f64 = (0..4).map(|k| a[(r, k)] * c[(k, col)]).sum();
            assert!((product[(r, col)] - expected).abs() <= 1e-15);
        }
    }
    let mut acc = a.clone();
    acc += &b;
    acc -= &b;
    acc *= 2.0;
    assert!(Matrix::compare(&acc, &(&a * 2.0), 1e-15));
    // (AC)ᵀ = CᵀAᵀ, and transposing twice is the identity.
    assert!(Matrix::compare(
        &product.transpose(),
        &(&c.transpose() * &a.transpose()),
        1e-15
    ));
    assert_eq!(a.transpose().transpose(), a);
    let had = Matrix::hadamard(&a, &b);
    assert_eq!(had[(2, 3)], a[(2, 3)] * b[(2, 3)]);
}

#[test]
#[should_panic(expected = "matrix product shapes do not match")]
fn b6_matrix_product_of_mismatched_shapes_panics() {
    let a = Matrix::<f32>::new(2, 3).unwrap();
    let _ = &a * &a;
}

#[test]
fn b6_matrix_solve_recovers_the_solution() {
    let mut rng = NoiseRng::new(5);
    for n in 1..=8 {
        // Diagonally dominant, so well conditioned.
        let mut a = random(&mut rng, n, n);
        for i in 0..n {
            a[(i, i)] += n as f64;
        }
        let x = random(&mut rng, n, 1);
        let mut b = &a * &x;
        assert!(a.solve(&mut b), "n = {n}");
        assert!(Matrix::compare(&b, &x, 1e-12), "n = {n}");
    }
    // A zero leading entry needs a row swap.
    let a = Matrix::from_row_major(2, 2, &[0.0_f64, 1.0, 1.0, 0.0]).unwrap();
    let mut b = Matrix::from_row_major(2, 1, &[2.0, 3.0]).unwrap();
    assert!(a.solve(&mut b));
    assert_eq!(b.data(), &[3.0, 2.0]);
    let mut wrong = Matrix::<f64>::new(3, 1).unwrap();
    assert!(
        !a.solve(&mut wrong),
        "b must have as many rows as the matrix"
    );
    let mut swapped = Matrix::from_row_major(2, 2, &[1.0_f32, 2.0, 3.0, 4.0]).unwrap();
    swapped.swap_rows(0, 1).swap_columns(0, 1);
    assert_eq!(swapped.data(), &[4.0, 3.0, 2.0, 1.0]);
}

/// `Σ c[i] x^i`, term by term.
fn direct(c: &[f64], x: f64) -> f64 {
    c.iter()
        .enumerate()
        .map(|(i, &k)| k * x.powi(i as i32))
        .sum()
}

#[test]
fn b6_polynomial_evaluates_lowest_power_first() {
    let c = [0.5, -0.3, 0.2, 0.05];
    let p = Polynomial::new(&c).unwrap();
    assert_eq!(p.order(), 3);
    for k in -20..=20 {
        let x = f64::from(k) * 0.25;
        assert!((p.evaluate(x) - direct(&c, x)).abs() <= 1e-12, "x {x}");
    }
    let lanes = no_alloc(|| p.evaluate(F64x4::from_array([-1.0, 0.0, 0.5, 2.0])));
    for (l, x) in [-1.0, 0.0, 0.5, 2.0].into_iter().enumerate() {
        assert_eq!(lanes.to_array()[l], p.evaluate(x));
    }
    assert_eq!(Polynomial::<f64>::default().evaluate(3.0), 0.0);
}

#[test]
fn b6_polynomial_sum_product_and_gain_agree_with_evaluation() {
    let p = Polynomial::new(&[1.0, 2.0, -1.0]).unwrap();
    let q = Polynomial::new(&[-3.0, 0.5]).unwrap();
    let (s, m, g) = (
        p.sum_with(&q).unwrap(),
        p.product_with(&q).unwrap(),
        p.with_gain(-2.0).unwrap(),
    );
    assert_eq!(m.order(), 3);
    for k in -10..=10 {
        let x = f64::from(k) * 0.3;
        assert!((s.evaluate(x) - (p.evaluate(x) + q.evaluate(x))).abs() <= 1e-12);
        assert!((m.evaluate(x) - p.evaluate(x) * q.evaluate(x)).abs() <= 1e-12);
        assert!((g.evaluate(x) + 2.0 * p.evaluate(x)).abs() <= 1e-12);
    }
    assert_eq!(
        p.product_with(&Polynomial::default())
            .unwrap()
            .coefficients(),
        &[] as &[f64]
    );
    let mut e = p.clone();
    e[0] = 7.0;
    assert_eq!(e.coefficients(), &[7.0, 2.0, -1.0]);
}

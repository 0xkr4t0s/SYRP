// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`Matrix`]: a small dense matrix for filter design and linear systems.

use std::ops::{Add, AddAssign, Index, IndexMut, Mul, MulAssign, Sub, SubAssign};

use crate::error::vec_with_capacity;
use crate::{DspError, DspFloat};

/// A dense `rows x columns` matrix stored in row-major order (JUCE's `dsp::Matrix`).
///
/// It is meant for design-time maths (filter design, small linear systems), not the audio
/// thread: building one allocates. Constructors check their shapes and return
/// [`DspError`]; arithmetic operators panic when the shapes do not match, like slice
/// indexing. Element `(r, c)` is read with `m[(r, c)]`.
///
/// ```
/// use syrp_dsp_core::Matrix;
/// let a = Matrix::from_row_major(2, 2, &[2.0, 1.0, 1.0, 3.0])?;
/// let mut b = Matrix::from_row_major(2, 1, &[3.0, 5.0])?;
/// assert!(a.solve(&mut b)); // 2x + y = 3, x + 3y = 5
/// assert!(Matrix::compare(&b, &Matrix::from_row_major(2, 1, &[0.8, 1.4])?, 1e-12));
/// assert_eq!((&a * &Matrix::identity(2)?)[(1, 1)], 3.0);
/// # Ok::<(), syrp_dsp_core::DspError>(())
/// ```
#[doc(alias = "dsp::Matrix")]
#[derive(Debug, Clone, PartialEq)]
pub struct Matrix<T> {
    rows: usize,
    columns: usize,
    data: Vec<T>,
}

impl<T: DspFloat> Matrix<T> {
    /// A `rows x columns` matrix of zeros.
    ///
    /// # Errors
    /// [`DspError::CannotAllocate`] if `rows * columns` overflows or cannot be allocated.
    ///
    /// ```
    /// let m = syrp_dsp_core::Matrix::<f32>::new(2, 3)?;
    /// assert_eq!((m.num_rows(), m.num_columns(), m[(1, 2)]), (2, 3, 0.0));
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    pub fn new(rows: usize, columns: usize) -> Result<Self, DspError> {
        let len = rows.checked_mul(columns).ok_or(DspError::CannotAllocate {
            what: "matrix",
            len: usize::MAX,
        })?;
        let mut data = vec_with_capacity("matrix", len)?;
        data.resize(len, T::zero());
        Ok(Self {
            rows,
            columns,
            data,
        })
    }

    /// A matrix whose elements are `data`, row after row.
    ///
    /// # Errors
    /// [`DspError::MatrixShape`] (`SYRP-DSP-0004`) unless `data.len() == rows * columns`.
    ///
    /// ```
    /// use syrp_dsp_core::Matrix;
    /// assert_eq!(Matrix::from_row_major(1, 2, &[1.0_f64, 2.0])?[(0, 1)], 2.0);
    /// assert!(Matrix::from_row_major(2, 2, &[1.0_f64]).is_err());
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    pub fn from_row_major(rows: usize, columns: usize, data: &[T]) -> Result<Self, DspError> {
        if rows.checked_mul(columns) != Some(data.len()) {
            return Err(DspError::MatrixShape);
        }
        let mut m = Self::new(rows, columns)?;
        m.data.copy_from_slice(data);
        Ok(m)
    }

    /// The `size x size` identity matrix.
    ///
    /// # Errors
    /// [`DspError::CannotAllocate`] if it cannot be allocated.
    ///
    /// ```
    /// let i = syrp_dsp_core::Matrix::<f32>::identity(3)?;
    /// assert_eq!((i[(1, 1)], i[(0, 1)]), (1.0, 0.0));
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    pub fn identity(size: usize) -> Result<Self, DspError> {
        let mut m = Self::new(size, size)?;
        (0..size).for_each(|i| m[(i, i)] = T::one());
        Ok(m)
    }

    /// The symmetric `size x size` Toeplitz matrix of `vector`: element `(r, c)` is
    /// `vector[|r - c|]`.
    ///
    /// # Errors
    /// [`DspError::MatrixShape`] if `vector` has fewer than `size` elements.
    ///
    /// ```
    /// let t = syrp_dsp_core::Matrix::toeplitz(&[1.0_f64, 2.0, 3.0], 3)?;
    /// assert_eq!((t[(0, 2)], t[(2, 0)], t[(1, 1)]), (3.0, 3.0, 1.0));
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    pub fn toeplitz(vector: &[T], size: usize) -> Result<Self, DspError> {
        if vector.len() < size {
            return Err(DspError::MatrixShape);
        }
        let mut m = Self::new(size, size)?;
        for r in 0..size {
            for c in 0..size {
                m[(r, c)] = vector[r.abs_diff(c)];
            }
        }
        Ok(m)
    }

    /// The `size x size` Hankel matrix of `vector` from `offset`: element `(r, c)` is
    /// `vector[offset + r + c]`.
    ///
    /// # Errors
    /// [`DspError::MatrixShape`] if `vector` has fewer than `offset + 2 * (size - 1) + 1`
    /// elements.
    ///
    /// ```
    /// let h = syrp_dsp_core::Matrix::hankel(&[0.0_f32, 1.0, 2.0, 3.0, 4.0], 2, 1)?;
    /// assert_eq!((h[(0, 0)], h[(0, 1)], h[(1, 1)]), (1.0, 2.0, 3.0));
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    pub fn hankel(vector: &[T], size: usize, offset: usize) -> Result<Self, DspError> {
        let needed = (2 * size).saturating_sub(1).saturating_add(offset);
        if size > 0 && vector.len() < needed {
            return Err(DspError::MatrixShape);
        }
        let mut m = Self::new(size, size)?;
        for r in 0..size {
            for c in 0..size {
                m[(r, c)] = vector[offset + r + c];
            }
        }
        Ok(m)
    }

    /// The number of rows.
    ///
    /// ```
    /// assert_eq!(syrp_dsp_core::Matrix::<f32>::new(4, 1)?.num_rows(), 4);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    #[must_use]
    pub fn num_rows(&self) -> usize {
        self.rows
    }

    /// The number of columns.
    ///
    /// ```
    /// assert_eq!(syrp_dsp_core::Matrix::<f32>::new(4, 1)?.num_columns(), 1);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    #[must_use]
    pub fn num_columns(&self) -> usize {
        self.columns
    }

    /// The elements in row-major order.
    ///
    /// ```
    /// let m = syrp_dsp_core::Matrix::from_row_major(2, 1, &[5.0_f32, 6.0])?;
    /// assert_eq!(m.data(), &[5.0, 6.0]);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    #[doc(alias = "getRawDataPointer")]
    #[must_use]
    pub fn data(&self) -> &[T] {
        &self.data
    }

    /// The elements in row-major order, mutably.
    ///
    /// ```
    /// let mut m = syrp_dsp_core::Matrix::<f32>::new(1, 2)?;
    /// m.data_mut()[1] = 7.0;
    /// assert_eq!(m[(0, 1)], 7.0);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    pub fn data_mut(&mut self) -> &mut [T] {
        &mut self.data
    }

    /// Sets every element to zero.
    ///
    /// ```
    /// let mut m = syrp_dsp_core::Matrix::<f64>::identity(2)?;
    /// m.clear();
    /// assert!(m.data().iter().all(|&v| v == 0.0));
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    pub fn clear(&mut self) {
        self.data.fill(T::zero());
    }

    /// Swaps rows `a` and `b`. Panics if either is out of range.
    ///
    /// ```
    /// let mut m = syrp_dsp_core::Matrix::from_row_major(2, 1, &[1.0_f32, 2.0])?;
    /// m.swap_rows(0, 1);
    /// assert_eq!(m.data(), &[2.0, 1.0]);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    pub fn swap_rows(&mut self, a: usize, b: usize) -> &mut Self {
        assert!(a < self.rows && b < self.rows, "row out of range");
        for c in 0..self.columns {
            self.data.swap(a * self.columns + c, b * self.columns + c);
        }
        self
    }

    /// Swaps columns `a` and `b`. Panics if either is out of range.
    ///
    /// ```
    /// let mut m = syrp_dsp_core::Matrix::from_row_major(1, 2, &[1.0_f32, 2.0])?;
    /// m.swap_columns(0, 1);
    /// assert_eq!(m.data(), &[2.0, 1.0]);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    pub fn swap_columns(&mut self, a: usize, b: usize) -> &mut Self {
        assert!(a < self.columns && b < self.columns, "column out of range");
        for r in 0..self.rows {
            self.data.swap(r * self.columns + a, r * self.columns + b);
        }
        self
    }

    /// The transpose.
    ///
    /// ```
    /// let m = syrp_dsp_core::Matrix::from_row_major(1, 2, &[1.0_f64, 2.0])?;
    /// assert_eq!(m.transpose().num_rows(), 2);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    #[must_use]
    pub fn transpose(&self) -> Self {
        let mut t = self.with_shape(self.columns, self.rows);
        for r in 0..self.rows {
            for c in 0..self.columns {
                t[(c, r)] = self[(r, c)];
            }
        }
        t
    }

    /// The element-wise (Hadamard) product of two matrices of the same shape. Panics on a
    /// shape mismatch.
    ///
    /// ```
    /// use syrp_dsp_core::Matrix;
    /// let a = Matrix::from_row_major(1, 2, &[2.0_f32, 3.0])?;
    /// assert_eq!(Matrix::hadamard(&a, &a).data(), &[4.0, 9.0]);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    #[doc(alias = "hadarmard")]
    #[must_use]
    pub fn hadamard(a: &Self, b: &Self) -> Self {
        a.zip(b, |x, y| x * y)
    }

    /// True for a square matrix.
    ///
    /// ```
    /// assert!(syrp_dsp_core::Matrix::<f32>::identity(2)?.is_square());
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    #[must_use]
    pub fn is_square(&self) -> bool {
        self.rows == self.columns
    }

    /// True for a one-row or one-column matrix.
    ///
    /// ```
    /// assert!(syrp_dsp_core::Matrix::<f32>::new(1, 5)?.is_vector());
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    #[must_use]
    pub fn is_vector(&self) -> bool {
        self.is_one_column_vector() || self.is_one_row_vector()
    }

    /// True for a matrix with exactly one column.
    ///
    /// ```
    /// assert!(syrp_dsp_core::Matrix::<f32>::new(3, 1)?.is_one_column_vector());
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    #[must_use]
    pub fn is_one_column_vector(&self) -> bool {
        self.columns == 1
    }

    /// True for a matrix with exactly one row.
    ///
    /// ```
    /// assert!(syrp_dsp_core::Matrix::<f32>::new(1, 3)?.is_one_row_vector());
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    #[must_use]
    pub fn is_one_row_vector(&self) -> bool {
        self.rows == 1
    }

    /// True for a matrix with no elements (zero rows or columns). Syrp reads JUCE's "null
    /// matrix" as this shape test; use [`compare`](Self::compare) against zeros for the
    /// all-zero test.
    ///
    /// ```
    /// assert!(syrp_dsp_core::Matrix::<f32>::new(0, 4)?.is_null_matrix());
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    #[must_use]
    pub fn is_null_matrix(&self) -> bool {
        self.data.is_empty()
    }

    /// True when `a` and `b` have the same shape and every pair of elements differs by at
    /// most `tolerance`.
    ///
    /// ```
    /// use syrp_dsp_core::Matrix;
    /// let a = Matrix::from_row_major(1, 1, &[1.0_f64])?;
    /// let b = Matrix::from_row_major(1, 1, &[1.001_f64])?;
    /// assert!(Matrix::compare(&a, &b, 0.01) && !Matrix::compare(&a, &b, 0.0));
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    #[must_use]
    pub fn compare(a: &Self, b: &Self, tolerance: T) -> bool {
        a.rows == b.rows
            && a.columns == b.columns
            && a.data
                .iter()
                .zip(&b.data)
                .all(|(&x, &y)| (x - y).abs() <= tolerance)
    }

    /// Solves `self · x = b` for a square `self` and a one-column `b` with as many rows,
    /// writing `x` into `b`. Uses Gaussian elimination with partial pivoting. Returns
    /// `false`, leaving `b` unchanged, when the shapes are wrong or the matrix is singular
    /// (a pivot is zero or not finite).
    ///
    /// ```
    /// use syrp_dsp_core::Matrix;
    /// let singular = Matrix::from_row_major(2, 2, &[1.0_f64, 2.0, 2.0, 4.0])?;
    /// let mut b = Matrix::from_row_major(2, 1, &[1.0, 1.0])?;
    /// assert!(!singular.solve(&mut b));
    /// assert_eq!(b.data(), &[1.0, 1.0]);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    #[must_use]
    pub fn solve(&self, b: &mut Self) -> bool {
        let n = self.rows;
        if !self.is_square() || b.rows != n || b.columns != 1 {
            return false;
        }
        let mut a = self.clone();
        let mut x = b.data.clone();
        for col in 0..n {
            let pivot = (col..n)
                .max_by(|&i, &j| {
                    a[(i, col)]
                        .abs()
                        .to_f64()
                        .total_cmp(&a[(j, col)].abs().to_f64())
                })
                .unwrap_or(col);
            let p = a[(pivot, col)];
            if p == T::zero() || !p.to_f64().is_finite() {
                return false;
            }
            a.swap_rows(col, pivot);
            x.swap(col, pivot);
            for r in col + 1..n {
                let f = a[(r, col)] / a[(col, col)];
                for c in col..n {
                    let v = a[(col, c)];
                    a[(r, c)] -= f * v;
                }
                let v = x[col];
                x[r] -= f * v;
            }
        }
        for r in (0..n).rev() {
            let tail = (r + 1..n).fold(T::zero(), |acc, c| acc + a[(r, c)] * x[c]);
            x[r] = (x[r] - tail) / a[(r, r)];
        }
        b.data = x;
        true
    }

    /// A zero matrix of the given shape, for operator results (the shape is that of an
    /// existing matrix, so it cannot overflow).
    fn with_shape(&self, rows: usize, columns: usize) -> Self {
        Self {
            rows,
            columns,
            data: vec![T::zero(); rows * columns],
        }
    }

    /// `f` applied to each pair of elements of two same-shaped matrices.
    fn zip(&self, other: &Self, f: impl Fn(T, T) -> T) -> Self {
        assert!(
            self.rows == other.rows && self.columns == other.columns,
            "matrix shapes differ: {}x{} and {}x{}",
            self.rows,
            self.columns,
            other.rows,
            other.columns
        );
        let data = self
            .data
            .iter()
            .zip(&other.data)
            .map(|(&a, &b)| f(a, b))
            .collect();
        Self {
            rows: self.rows,
            columns: self.columns,
            data,
        }
    }
}

impl<T> Index<(usize, usize)> for Matrix<T> {
    type Output = T;
    fn index(&self, (r, c): (usize, usize)) -> &T {
        assert!(
            r < self.rows && c < self.columns,
            "element ({r}, {c}) out of range"
        );
        &self.data[r * self.columns + c]
    }
}

impl<T> IndexMut<(usize, usize)> for Matrix<T> {
    fn index_mut(&mut self, (r, c): (usize, usize)) -> &mut T {
        assert!(
            r < self.rows && c < self.columns,
            "element ({r}, {c}) out of range"
        );
        &mut self.data[r * self.columns + c]
    }
}

impl<T: DspFloat> Add for &Matrix<T> {
    type Output = Matrix<T>;
    fn add(self, rhs: Self) -> Matrix<T> {
        self.zip(rhs, |a, b| a + b)
    }
}

impl<T: DspFloat> Sub for &Matrix<T> {
    type Output = Matrix<T>;
    fn sub(self, rhs: Self) -> Matrix<T> {
        self.zip(rhs, |a, b| a - b)
    }
}

impl<T: DspFloat> Mul<T> for &Matrix<T> {
    type Output = Matrix<T>;
    fn mul(self, scalar: T) -> Matrix<T> {
        let mut m = self.clone();
        m *= scalar;
        m
    }
}

impl<T: DspFloat> Mul for &Matrix<T> {
    type Output = Matrix<T>;
    /// The matrix product; panics unless `self.num_columns() == rhs.num_rows()`.
    fn mul(self, rhs: Self) -> Matrix<T> {
        assert_eq!(self.columns, rhs.rows, "matrix product shapes do not match");
        let mut m = self.with_shape(self.rows, rhs.columns);
        for r in 0..self.rows {
            for c in 0..rhs.columns {
                m[(r, c)] =
                    (0..self.columns).fold(T::zero(), |acc, k| acc + self[(r, k)] * rhs[(k, c)]);
            }
        }
        m
    }
}

impl<T: DspFloat> AddAssign<&Matrix<T>> for Matrix<T> {
    fn add_assign(&mut self, rhs: &Matrix<T>) {
        *self = &*self + rhs;
    }
}

impl<T: DspFloat> SubAssign<&Matrix<T>> for Matrix<T> {
    fn sub_assign(&mut self, rhs: &Matrix<T>) {
        *self = &*self - rhs;
    }
}

impl<T: DspFloat> MulAssign<T> for Matrix<T> {
    fn mul_assign(&mut self, scalar: T) {
        self.data.iter_mut().for_each(|v| *v *= scalar);
    }
}

// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`Polynomial`]: a polynomial with real coefficients.

use std::ops::{Index, IndexMut};

use crate::error::vec_with_capacity;
use crate::{DspError, DspFloat, DspSample};

/// A polynomial `c[0] + c[1] x + c[2] x² + ...` (JUCE's `dsp::Polynomial`).
///
/// Coefficient `i` multiplies `x^i` (lowest power first; the JUCE documentation does not
/// state the order, so this is Syrp's choice). The empty polynomial evaluates to zero.
/// Evaluation is lane-generic and never allocates; building one allocates.
///
/// ```
/// use syrp_dsp_core::Polynomial;
/// use syrp_simd::F32x4;
/// let p = Polynomial::new(&[1.0_f32, -2.0, 1.0])?; // (x - 1)²
/// assert_eq!(p.evaluate(3.0_f32), 4.0);
/// assert_eq!(p.evaluate(F32x4::from_array([0.0, 1.0, 2.0, -1.0])).to_array(), [1.0, 0.0, 1.0, 4.0]);
/// assert_eq!(p.order(), 2);
/// # Ok::<(), syrp_dsp_core::DspError>(())
/// ```
#[doc(alias = "dsp::Polynomial")]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Polynomial<T> {
    coefficients: Vec<T>,
}

impl<T: DspFloat> Polynomial<T> {
    /// A polynomial with a copy of `coefficients`, lowest power first.
    ///
    /// # Errors
    /// [`DspError::CannotAllocate`] if the copy cannot be allocated.
    ///
    /// ```
    /// let p = syrp_dsp_core::Polynomial::new(&[0.5_f64, -0.3, 0.2])?;
    /// assert_eq!(p[1], -0.3);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    pub fn new(coefficients: &[T]) -> Result<Self, DspError> {
        let mut c = vec_with_capacity("polynomial", coefficients.len())?;
        c.extend_from_slice(coefficients);
        Ok(Self { coefficients: c })
    }

    /// The coefficients, lowest power first.
    ///
    /// ```
    /// let p = syrp_dsp_core::Polynomial::new(&[1.0_f32, 2.0])?;
    /// assert_eq!(p.coefficients(), &[1.0, 2.0]);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    #[must_use]
    pub fn coefficients(&self) -> &[T] {
        &self.coefficients
    }

    /// The order (degree): the number of coefficients minus one, or 0 when there are none.
    ///
    /// ```
    /// use syrp_dsp_core::Polynomial;
    /// assert_eq!(Polynomial::new(&[1.0_f32, 0.0, 0.0, 5.0])?.order(), 3);
    /// assert_eq!(Polynomial::<f32>::default().order(), 0);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    #[doc(alias = "getOrder")]
    #[must_use]
    pub fn order(&self) -> usize {
        self.coefficients.len().saturating_sub(1)
    }

    /// The value at each lane of `x`, by Horner's rule.
    ///
    /// ```
    /// let p = syrp_dsp_core::Polynomial::new(&[2.0_f64, 0.0, 1.0])?;
    /// assert_eq!(p.evaluate(3.0_f64), 11.0);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    #[inline]
    #[must_use]
    pub fn evaluate<S: DspSample<Element = T>>(&self, x: S) -> S {
        self.coefficients
            .iter()
            .rev()
            .fold(S::zero(), |acc, &c| acc * x + S::splat(c))
    }

    /// The polynomial with every coefficient multiplied by `gain`.
    ///
    /// # Errors
    /// [`DspError::CannotAllocate`] if the result cannot be allocated.
    ///
    /// ```
    /// let p = syrp_dsp_core::Polynomial::new(&[1.0_f32, 2.0])?.with_gain(3.0)?;
    /// assert_eq!(p.coefficients(), &[3.0, 6.0]);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    pub fn with_gain(&self, gain: T) -> Result<Self, DspError> {
        let mut p = Self::new(&self.coefficients)?;
        p.coefficients.iter_mut().for_each(|c| *c *= gain);
        Ok(p)
    }

    /// The sum of `self` and `other`.
    ///
    /// # Errors
    /// [`DspError::CannotAllocate`] if the result cannot be allocated.
    ///
    /// ```
    /// use syrp_dsp_core::Polynomial;
    /// let s = Polynomial::new(&[1.0_f64, 1.0])?.sum_with(&Polynomial::new(&[0.0, 1.0, 1.0])?)?;
    /// assert_eq!(s.coefficients(), &[1.0, 2.0, 1.0]);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    #[doc(alias = "getSumWith")]
    pub fn sum_with(&self, other: &Self) -> Result<Self, DspError> {
        let (long, short) = if self.coefficients.len() >= other.coefficients.len() {
            (self, other)
        } else {
            (other, self)
        };
        let mut s = Self::new(&long.coefficients)?;
        for (a, &b) in s.coefficients.iter_mut().zip(&short.coefficients) {
            *a += b;
        }
        Ok(s)
    }

    /// The product of `self` and `other`; empty if either is empty.
    ///
    /// # Errors
    /// [`DspError::CannotAllocate`] if the result cannot be allocated.
    ///
    /// ```
    /// use syrp_dsp_core::Polynomial;
    /// let x_minus_one = Polynomial::new(&[-1.0_f32, 1.0])?;
    /// let sq = x_minus_one.product_with(&x_minus_one)?;
    /// assert_eq!(sq.coefficients(), &[1.0, -2.0, 1.0]);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    #[doc(alias = "getProductWith")]
    pub fn product_with(&self, other: &Self) -> Result<Self, DspError> {
        let (a, b) = (&self.coefficients, &other.coefficients);
        if a.is_empty() || b.is_empty() {
            return Ok(Self::default());
        }
        let len = a.len() + b.len() - 1;
        let mut c = vec_with_capacity("polynomial", len)?;
        c.resize(len, T::zero());
        for (i, &x) in a.iter().enumerate() {
            for (j, &y) in b.iter().enumerate() {
                c[i + j] += x * y;
            }
        }
        Ok(Self { coefficients: c })
    }
}

impl<T> Index<usize> for Polynomial<T> {
    type Output = T;
    fn index(&self, i: usize) -> &T {
        &self.coefficients[i]
    }
}

impl<T> IndexMut<usize> for Polynomial<T> {
    fn index_mut(&mut self, i: usize) -> &mut T {
        &mut self.coefficients[i]
    }
}

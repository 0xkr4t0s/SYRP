// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`LookupTable`] and [`LookupTableTransform`]: functions replaced by interpolated tables.

use crate::error::vec_with_capacity;
use crate::{DspError, DspFloat, DspSample};

/// A function of the index range `0..num_points` stored as a table and read back with
/// linear interpolation (JUCE's `dsp::LookupTable`).
///
/// Values are stored as `T` (`f32` or `f64`); reads are lane-generic, so an `F32x8` of
/// indices reads eight values at once. Reads never allocate.
///
/// ```
/// use syrp_dsp_core::LookupTable;
/// use syrp_simd::F32x4;
/// let squares = LookupTable::<f32>::new(|i| i * i, 5)?; // 0, 1, 4, 9, 16
/// assert_eq!(squares.get_unchecked(1.5_f32), 2.5);
/// assert_eq!(squares.get(-3.0_f32), 0.0); // clamped to the first point
/// let lanes = squares.get(F32x4::from_array([0.0, 2.0, 3.5, 99.0]));
/// assert_eq!(lanes.to_array(), [0.0, 4.0, 12.5, 16.0]);
/// # Ok::<(), syrp_dsp_core::DspError>(())
/// ```
#[doc(alias = "dsp::LookupTable")]
#[derive(Debug, Clone, PartialEq)]
pub struct LookupTable<T> {
    /// `num_points` values plus a copy of the last, so `get_unchecked` just below the top
    /// index needs no bounds branch.
    data: Vec<T>,
}

impl<T: DspFloat> LookupTable<T> {
    /// A table of `f(0), f(1), .., f(num_points - 1)`.
    ///
    /// # Errors
    /// [`DspError::InvalidTable`] (`SYRP-DSP-0003`) if `num_points < 2`, and
    /// [`DspError::CannotAllocate`] if the table cannot be allocated.
    ///
    /// ```
    /// let t = syrp_dsp_core::LookupTable::<f64>::new(|i| 2.0 * i, 3)?;
    /// assert_eq!(t.num_points(), 3);
    /// assert!(syrp_dsp_core::LookupTable::<f64>::new(|i| i, 1).is_err());
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    pub fn new(f: impl FnMut(T) -> T, num_points: usize) -> Result<Self, DspError> {
        let mut table = Self { data: Vec::new() };
        table.initialise(f, num_points)?;
        Ok(table)
    }

    /// Refills the table with `f` over `num_points` points. On error the table is
    /// unchanged. Allocates when the table grows.
    ///
    /// # Errors
    /// As [`new`](Self::new).
    ///
    /// ```
    /// let mut t = syrp_dsp_core::LookupTable::<f32>::new(|i| i, 2)?;
    /// t.initialise(|i| -i, 4)?;
    /// assert_eq!(t.get(3.0_f32), -3.0);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    pub fn initialise(
        &mut self,
        mut f: impl FnMut(T) -> T,
        num_points: usize,
    ) -> Result<(), DspError> {
        if num_points < 2 {
            return Err(DspError::InvalidTable { num_points });
        }
        let len = num_points.checked_add(1).ok_or(DspError::CannotAllocate {
            what: "lookup table",
            len: usize::MAX,
        })?;
        let mut data = vec_with_capacity("lookup table", len)?;
        data.extend((0..num_points).map(|i| f(T::from_f64(i as f64))));
        data.push(data[num_points - 1]);
        self.data = data;
        Ok(())
    }

    /// The number of stored points.
    ///
    /// ```
    /// assert_eq!(syrp_dsp_core::LookupTable::<f32>::new(|i| i, 16)?.num_points(), 16);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    #[doc(alias = "getNumPoints")]
    #[must_use]
    pub fn num_points(&self) -> usize {
        self.data.len().saturating_sub(1)
    }

    /// The interpolated value at each lane of `index`, which must be in
    /// `0.0..num_points() as f32` (not checked beyond debug assertions; out of range panics or
    /// reads a neighbouring value, never undefined behaviour).
    ///
    /// ```
    /// let t = syrp_dsp_core::LookupTable::<f64>::new(|i| 10.0 * i, 3)?;
    /// assert_eq!(t.get_unchecked(0.25_f64), 2.5);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    #[inline]
    #[must_use]
    pub fn get_unchecked<S: DspSample<Element = T>>(&self, index: S) -> S {
        S::from_fn(|l| self.read(index.extract(l)))
    }

    /// The interpolated value at each lane of `index`, clamped to `0..=num_points() - 1`:
    /// an index below the range reads the first point and one above it the last.
    ///
    /// ```
    /// let t = syrp_dsp_core::LookupTable::<f32>::new(|i| i + 1.0, 4)?;
    /// assert_eq!((t.get(-1.0_f32), t.get(1e9_f32)), (1.0, 4.0));
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    #[inline]
    #[must_use]
    pub fn get<S: DspSample<Element = T>>(&self, index: S) -> S {
        let top = T::from_f64(self.num_points() as f64 - 1.0);
        self.get_unchecked(index.max(S::zero()).min(S::splat(top)))
    }

    /// One interpolated read; `index` must be in `0..num_points()`.
    #[inline(always)]
    fn read(&self, index: T) -> T {
        debug_assert!(
            index.to_f64() >= 0.0 && index.to_f64() < self.num_points() as f64,
            "lookup index {index:?} out of range"
        );
        let i = index.to_f64() as usize;
        let frac = index - T::from_f64(i as f64);
        let (a, b) = (self.data[i], self.data[i + 1]);
        a + (b - a) * frac
    }
}

/// A function over an input range `min..=max`, stored as a [`LookupTable`] and read back
/// with linear interpolation (JUCE's `dsp::LookupTableTransform`).
///
/// Point `i` of `num_points` holds `f(min + i * (max - min) / (num_points - 1))`.
/// [`process_sample`](Self::process_sample) clips its input to the range first. Reads are
/// lane-generic and never allocate.
///
/// ```
/// use syrp_dsp_core::LookupTableTransform;
/// let t = LookupTableTransform::<f64>::new(f64::sin, -3.2, 3.2, 1024)?;
/// assert!((t.process_sample(1.0_f64) - 1.0_f64.sin()).abs() < 1e-5);
/// assert_eq!(t.process_sample(100.0_f64), t.process_sample(3.2_f64)); // clipped
/// # Ok::<(), syrp_dsp_core::DspError>(())
/// ```
#[doc(alias = "dsp::LookupTableTransform")]
#[derive(Debug, Clone, PartialEq)]
pub struct LookupTableTransform<T> {
    table: LookupTable<T>,
    min: T,
    max: T,
    /// `(num_points - 1) / (max - min)`: input to index.
    scaler: T,
    /// `-min * scaler`.
    offset: T,
}

impl<T: DspFloat> LookupTableTransform<T> {
    /// A table of `f` over `min..=max` with `num_points` points.
    ///
    /// # Errors
    /// [`DspError::InvalidTable`] (`SYRP-DSP-0003`) if `num_points < 2` or the range is not
    /// finite with `min < max`; [`DspError::CannotAllocate`] if the table cannot be allocated.
    ///
    /// ```
    /// use syrp_dsp_core::LookupTableTransform;
    /// assert!(LookupTableTransform::<f32>::new(|x| x, 1.0, 1.0, 8).is_err());
    /// assert!(LookupTableTransform::<f32>::new(|x| x, 0.0, 1.0, 8).is_ok());
    /// ```
    pub fn new(f: impl FnMut(T) -> T, min: T, max: T, num_points: usize) -> Result<Self, DspError> {
        let mut t = Self {
            table: LookupTable { data: Vec::new() },
            min,
            max,
            scaler: T::zero(),
            offset: T::zero(),
        };
        t.initialise(f, min, max, num_points)?;
        Ok(t)
    }

    /// Refills the table with `f` over `min..=max`. On error nothing changes.
    ///
    /// # Errors
    /// As [`new`](Self::new).
    ///
    /// ```
    /// use syrp_dsp_core::LookupTableTransform;
    /// let mut t = LookupTableTransform::<f32>::new(|x| x, 0.0, 1.0, 2)?;
    /// t.initialise(|x| 2.0 * x, 0.0, 10.0, 11)?;
    /// assert_eq!(t.process_sample(2.5_f32), 5.0);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    pub fn initialise(
        &mut self,
        mut f: impl FnMut(T) -> T,
        min: T,
        max: T,
        num_points: usize,
    ) -> Result<(), DspError> {
        let (lo, hi) = (min.to_f64(), max.to_f64());
        if !(lo.is_finite() && hi.is_finite() && lo < hi) {
            return Err(DspError::InvalidTable { num_points });
        }
        let step = (hi - lo) / (num_points.max(2) - 1) as f64;
        self.table
            .initialise(|i| f(T::from_f64(lo + i.to_f64() * step)), num_points)?;
        let scaler = (num_points - 1) as f64 / (hi - lo);
        self.min = min;
        self.max = max;
        self.scaler = T::from_f64(scaler);
        self.offset = T::from_f64(-lo * scaler);
        Ok(())
    }

    /// The approximation at each lane of `x`, which must be in `min..=max` (not checked
    /// beyond debug assertions).
    ///
    /// ```
    /// use syrp_dsp_core::LookupTableTransform;
    /// let t = LookupTableTransform::<f32>::new(|x| 3.0 * x, -1.0, 1.0, 3)?;
    /// assert_eq!(t.process_sample_unchecked(0.5_f32), 1.5);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    #[inline]
    #[must_use]
    pub fn process_sample_unchecked<S: DspSample<Element = T>>(&self, x: S) -> S {
        let index = x * S::splat(self.scaler) + S::splat(self.offset);
        let top = T::from_f64(self.table.num_points() as f64 - 1.0);
        // Rounding can land a hair outside the index range at the ends.
        self.table
            .get_unchecked(index.max(S::zero()).min(S::splat(top)))
    }

    /// The approximation at each lane of `x`, clipped to `min..=max` first.
    ///
    /// ```
    /// use syrp_dsp_core::LookupTableTransform;
    /// let t = LookupTableTransform::<f32>::new(|x| x * x, 0.0, 2.0, 3)?;
    /// assert_eq!(t.process_sample(-5.0_f32), 0.0);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    #[inline]
    #[must_use]
    pub fn process_sample<S: DspSample<Element = T>>(&self, x: S) -> S {
        self.process_sample_unchecked(x.max(S::splat(self.min)).min(S::splat(self.max)))
    }

    /// [`process_sample`](Self::process_sample) over `input` into `output`, over the length
    /// both have.
    ///
    /// ```
    /// use syrp_dsp_core::LookupTableTransform;
    /// let t = LookupTableTransform::<f64>::new(|x| x + 1.0, 0.0, 1.0, 2)?;
    /// let mut out = [0.0; 3];
    /// t.process(&[0.0, 0.5, 7.0], &mut out);
    /// assert_eq!(out, [1.0, 1.5, 2.0]);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    pub fn process<S: DspSample<Element = T>>(&self, input: &[S], output: &mut [S]) {
        for (o, &i) in output.iter_mut().zip(input) {
            *o = self.process_sample(i);
        }
    }

    /// [`process_sample_unchecked`](Self::process_sample_unchecked) over `input` into
    /// `output`, over the length both have.
    ///
    /// ```
    /// use syrp_dsp_core::LookupTableTransform;
    /// let t = LookupTableTransform::<f64>::new(|x| -x, 0.0, 1.0, 2)?;
    /// let mut out = [0.0; 2];
    /// t.process_unchecked(&[0.25, 1.0], &mut out);
    /// assert_eq!(out, [-0.25, -1.0]);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    pub fn process_unchecked<S: DspSample<Element = T>>(&self, input: &[S], output: &mut [S]) {
        for (o, &i) in output.iter_mut().zip(input) {
            *o = self.process_sample_unchecked(i);
        }
    }

    /// The largest relative error of a `num_points` table of `f` over `min..=max`, measured
    /// at `num_test_points` evenly spaced inputs including both ends (`0` means
    /// `100 * num_points`). The relative error at `x` is `|approx - f(x)| / |f(x)|`, or the
    /// absolute error where `f(x)` is zero (Syrp's choice). Builds a table, so it allocates.
    ///
    /// # Errors
    /// As [`new`](Self::new).
    ///
    /// ```
    /// use syrp_dsp_core::LookupTableTransform;
    /// let coarse = LookupTableTransform::<f64>::calculate_max_relative_error(f64::exp, 0.0, 1.0, 8, 0)?;
    /// let fine = LookupTableTransform::<f64>::calculate_max_relative_error(f64::exp, 0.0, 1.0, 64, 0)?;
    /// assert!(fine < coarse && fine < 1e-4);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    pub fn calculate_max_relative_error(
        mut f: impl FnMut(T) -> T,
        min: T,
        max: T,
        num_points: usize,
        num_test_points: usize,
    ) -> Result<T, DspError> {
        let table = Self::new(&mut f, min, max, num_points)?;
        let tests = if num_test_points == 0 {
            num_points.saturating_mul(100)
        } else {
            num_test_points
        };
        let (lo, hi) = (min.to_f64(), max.to_f64());
        let mut worst = 0.0_f64;
        for i in 0..tests {
            let t = if tests == 1 {
                0.0
            } else {
                i as f64 / (tests - 1) as f64
            };
            let x = T::from_f64(lo + t * (hi - lo));
            let (exact, approx) = (f(x).to_f64(), table.process_sample(x).to_f64());
            let err = (approx - exact).abs();
            worst = worst.max(if exact == 0.0 { err } else { err / exact.abs() });
        }
        Ok(T::from_f64(worst))
    }
}

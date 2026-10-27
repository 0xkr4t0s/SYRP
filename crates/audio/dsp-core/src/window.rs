// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`WindowingFunction`]: window tables for spectral analysis and FIR design.

use std::f64::consts::TAU;

use crate::error::vec_with_capacity;
use crate::special::bessel_i0;
use crate::{DspError, DspFloat};

/// The window shapes [`WindowingFunction`] can fill (JUCE's `WindowingMethod`).
///
/// Every shape is symmetric: with `N` points and `x = i / (N - 1)` for point `i`,
///
/// | Method | `w[i]` |
/// |---|---|
/// | `Rectangular` | `1` |
/// | `Triangular` | `1 - |2x - 1|` (Bartlett: zero at both ends) |
/// | `Hann` | `0.5 - 0.5 cos(2πx)` |
/// | `Hamming` | `0.54 - 0.46 cos(2πx)` |
/// | `Blackman` | `0.42 - 0.5 cos(2πx) + 0.08 cos(4πx)` |
/// | `BlackmanHarris` | `0.35875 - 0.48829 cos(2πx) + 0.14128 cos(4πx) - 0.01168 cos(6πx)` |
/// | `FlatTop` | `a0 - a1 cos(2πx) + a2 cos(4πx) - a3 cos(6πx) + a4 cos(8πx)` |
/// | `Kaiser` | `I0(β √(1 - (2x - 1)²)) / I0(β)` |
///
/// These are the standard published formulas; the flat-top coefficients are
/// `a0..a4 = 0.21557895, 0.41663158, 0.277263158, 0.083578947, 0.006947368`. A one-point
/// window is `[1]`. The JUCE documentation names the methods but not their formulas, so the
/// exact variant (for example Bartlett's triangle) is Syrp's choice.
///
/// ```
/// use syrp_dsp_core::{WindowingFunction, WindowingMethod};
/// let w = WindowingFunction::<f64>::new(3, WindowingMethod::Hann, false, 0.0)?;
/// assert_eq!(w.table(), &[0.0, 1.0, 0.0]);
/// # Ok::<(), syrp_dsp_core::DspError>(())
/// ```
#[doc(alias = "dsp::WindowingFunction::WindowingMethod")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum WindowingMethod {
    /// All ones.
    Rectangular,
    /// Bartlett's triangle.
    Triangular,
    /// The Hann (raised cosine) window.
    Hann,
    /// The Hamming window.
    Hamming,
    /// The three-term Blackman window.
    Blackman,
    /// The four-term Blackman-Harris window.
    BlackmanHarris,
    /// The five-term flat-top window.
    FlatTop,
    /// The Kaiser window; `beta` sets its shape (bandwidth and attenuation grow with it).
    Kaiser,
}

impl WindowingMethod {
    /// Point `i` of an `n`-point window (`n >= 2`), before normalisation.
    fn value(self, i: usize, n: usize, beta: f64) -> f64 {
        let x = i as f64 / (n - 1) as f64;
        let c = |k: f64| (k * TAU * x).cos();
        match self {
            Self::Rectangular => 1.0,
            Self::Triangular => 1.0 - (2.0 * x - 1.0).abs(),
            Self::Hann => 0.5 - 0.5 * c(1.0),
            Self::Hamming => 0.54 - 0.46 * c(1.0),
            Self::Blackman => 0.42 - 0.5 * c(1.0) + 0.08 * c(2.0),
            Self::BlackmanHarris => {
                0.35875 - 0.48829 * c(1.0) + 0.14128 * c(2.0) - 0.01168 * c(3.0)
            }
            Self::FlatTop => {
                0.215_578_95 - 0.416_631_58 * c(1.0) + 0.277_263_158 * c(2.0)
                    - 0.083_578_947 * c(3.0)
                    + 0.006_947_368 * c(4.0)
            }
            Self::Kaiser => {
                let r = 2.0 * x - 1.0;
                bessel_i0(beta * (1.0 - r * r).max(0.0).sqrt()) / bessel_i0(beta)
            }
        }
    }
}

/// A precomputed window table (JUCE's `dsp::WindowingFunction`).
///
/// With `normalise`, the table is scaled so its values sum to one, which gives the window
/// a DC amplitude response of one (the meaning the JUCE documentation gives). `beta` is used
/// only by [`WindowingMethod::Kaiser`].
///
/// [`new`](Self::new) and [`fill_windowing_tables`](Self::fill_windowing_tables) allocate;
/// [`multiply_with_windowing_table`](Self::multiply_with_windowing_table) and
/// [`fill_windowing_tables_into`](Self::fill_windowing_tables_into) do not.
///
/// ```
/// use syrp_dsp_core::{WindowingFunction, WindowingMethod};
/// let w = WindowingFunction::<f32>::new(4, WindowingMethod::Rectangular, true, 0.0)?;
/// let mut samples = [2.0_f32; 4];
/// w.multiply_with_windowing_table(&mut samples);
/// assert_eq!(samples, [0.5; 4]); // normalised: the table sums to one
/// # Ok::<(), syrp_dsp_core::DspError>(())
/// ```
#[doc(alias = "dsp::WindowingFunction")]
#[derive(Debug, Clone, PartialEq)]
pub struct WindowingFunction<T> {
    table: Vec<T>,
}

impl<T: DspFloat> WindowingFunction<T> {
    /// A `size`-point table of `method`.
    ///
    /// # Errors
    /// [`DspError::CannotAllocate`] if the table cannot be allocated.
    ///
    /// ```
    /// use syrp_dsp_core::{WindowingFunction, WindowingMethod};
    /// let w = WindowingFunction::<f64>::new(5, WindowingMethod::Triangular, false, 0.0)?;
    /// assert_eq!(w.table(), &[0.0, 0.5, 1.0, 0.5, 0.0]);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    pub fn new(
        size: usize,
        method: WindowingMethod,
        normalise: bool,
        beta: T,
    ) -> Result<Self, DspError> {
        let mut w = Self { table: Vec::new() };
        w.fill_windowing_tables(size, method, normalise, beta)?;
        Ok(w)
    }

    /// Refills the table with `size` points of `method`, reallocating when the size grows.
    /// On error the table is unchanged.
    ///
    /// # Errors
    /// [`DspError::CannotAllocate`] if the table cannot be allocated.
    ///
    /// ```
    /// use syrp_dsp_core::{WindowingFunction, WindowingMethod};
    /// let mut w = WindowingFunction::<f32>::new(2, WindowingMethod::Hann, false, 0.0)?;
    /// w.fill_windowing_tables(3, WindowingMethod::Rectangular, false, 0.0)?;
    /// assert_eq!(w.table(), &[1.0; 3]);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    pub fn fill_windowing_tables(
        &mut self,
        size: usize,
        method: WindowingMethod,
        normalise: bool,
        beta: T,
    ) -> Result<(), DspError> {
        if size > self.table.len() {
            let mut table = vec_with_capacity("window table", size)?;
            table.resize(size, T::zero());
            self.table = table;
        } else {
            self.table.truncate(size);
        }
        Self::fill_windowing_tables_into(&mut self.table, method, normalise, beta);
        Ok(())
    }

    /// Fills `samples` with a `samples.len()`-point window of `method` (JUCE's static
    /// `fillWindowingTables`). Never allocates.
    ///
    /// ```
    /// use syrp_dsp_core::{WindowingFunction, WindowingMethod};
    /// let mut table = [0.0_f64; 2];
    /// WindowingFunction::fill_windowing_tables_into(&mut table, WindowingMethod::Hamming, false, 0.0);
    /// assert!((table[0] - 0.08).abs() < 1e-15);
    /// ```
    pub fn fill_windowing_tables_into(
        samples: &mut [T],
        method: WindowingMethod,
        normalise: bool,
        beta: T,
    ) {
        let n = samples.len();
        let beta = beta.to_f64();
        let mut sum = 0.0;
        for (i, s) in samples.iter_mut().enumerate() {
            let v = if n == 1 {
                1.0
            } else {
                method.value(i, n, beta)
            };
            sum += v;
            *s = T::from_f64(v);
        }
        if normalise && sum > 0.0 {
            let scale = 1.0 / sum;
            samples
                .iter_mut()
                .for_each(|s| *s = T::from_f64(s.to_f64() * scale));
        }
    }

    /// Multiplies `samples` by the table, point by point, over the length both have.
    /// Never allocates.
    ///
    /// ```
    /// use syrp_dsp_core::{WindowingFunction, WindowingMethod};
    /// let w = WindowingFunction::<f32>::new(3, WindowingMethod::Triangular, false, 0.0)?;
    /// let mut s = [4.0_f32; 3];
    /// w.multiply_with_windowing_table(&mut s);
    /// assert_eq!(s, [0.0, 4.0, 0.0]);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    pub fn multiply_with_windowing_table(&self, samples: &mut [T]) {
        for (s, w) in samples.iter_mut().zip(&self.table) {
            *s *= *w;
        }
    }

    /// The table.
    ///
    /// ```
    /// use syrp_dsp_core::{WindowingFunction, WindowingMethod};
    /// let w = WindowingFunction::<f32>::new(1, WindowingMethod::Blackman, false, 0.0)?;
    /// assert_eq!(w.table(), &[1.0]);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    #[must_use]
    pub fn table(&self) -> &[T] {
        &self.table
    }

    /// The number of points.
    ///
    /// ```
    /// use syrp_dsp_core::{WindowingFunction, WindowingMethod};
    /// assert_eq!(WindowingFunction::<f64>::new(8, WindowingMethod::Hann, true, 0.0)?.size(), 8);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    #[must_use]
    pub fn size(&self) -> usize {
        self.table.len()
    }
}

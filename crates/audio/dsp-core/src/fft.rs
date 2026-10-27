// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`Fft`]: power-of-two FFTs with JUCE's buffer layout, built on `rustfft` and `realfft`.

use std::fmt;
use std::sync::Arc;

use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use rustfft::FftPlanner;
use rustfft::num_complex::Complex;

use crate::DspError;
use crate::error::vec_with_capacity;

/// Forward and inverse FFTs of `2^order` points (JUCE's `dsp::FFT`).
///
/// Every plan and scratch buffer is made in [`new`](Self::new), so the `perform*` methods
/// never allocate, lock or do I/O; they take `&mut self` because they share that scratch.
///
/// Layout and scaling:
/// - Bin `k` is frequency `k * sample_rate / size`; bins above `size / 2` are the negative
///   frequencies, in the usual FFT order.
/// - Forward transforms are unscaled: `X[k] = Σ x[n] e^(-2πikn/N)`.
/// - Inverse transforms are scaled by `1/N`: `x[n] = (1/N) Σ X[k] e^(2πikn/N)`, so a forward
///   then inverse transform returns the input. The JUCE documentation does not state the
///   scaling of either direction; these are Syrp's choices (see the crate `AGENTS.md`).
/// - Real-only transforms use one `f32` array of `2 * size` values: real samples in the
///   first half on input, interleaved `re, im` pairs on output.
///
/// ```
/// use syrp_dsp_core::Fft;
/// let mut fft = Fft::new(3)?; // 8 points
/// let mut data = [0.0_f32; 16];
/// data[..8].copy_from_slice(&[1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
/// fft.perform_real_only_forward_transform(&mut data, false);
/// assert!(data.chunks(2).all(|c| c == [1.0, 0.0])); // an impulse has a flat spectrum
/// fft.perform_real_only_inverse_transform(&mut data);
/// assert_eq!(data[..8], [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
/// # Ok::<(), syrp_dsp_core::DspError>(())
/// ```
#[doc(alias = "dsp::FFT")]
pub struct Fft {
    order: u32,
    size: usize,
    forward: Arc<dyn rustfft::Fft<f32>>,
    inverse: Arc<dyn rustfft::Fft<f32>>,
    real_forward: Arc<dyn RealToComplex<f32>>,
    real_inverse: Arc<dyn ComplexToReal<f32>>,
    /// Shared scratch, as long as the longest any plan needs.
    scratch: Vec<Complex<f32>>,
    /// `size` real samples for the real transforms.
    real: Vec<f32>,
    /// `size / 2 + 1` bins for the real transforms.
    spectrum: Vec<Complex<f32>>,
}

impl Fft {
    /// The largest supported order: 2^20 = 1,048,576 points (Syrp's limit; a plan that size
    /// already takes tens of megabytes).
    ///
    /// ```
    /// assert!(syrp_dsp_core::Fft::new(syrp_dsp_core::Fft::MAX_ORDER + 1).is_err());
    /// ```
    pub const MAX_ORDER: u32 = 20;

    /// An FFT of `2^order` points. Allocates its plans and scratch; call it off the audio
    /// thread.
    ///
    /// # Errors
    /// [`DspError::OrderTooLarge`] (`SYRP-DSP-0002`) if `order > MAX_ORDER`, and
    /// [`DspError::CannotAllocate`] if the scratch cannot be allocated.
    ///
    /// ```
    /// use syrp_dsp_core::{DspError, Fft};
    /// assert_eq!(Fft::new(10)?.size(), 1024);
    /// assert!(matches!(Fft::new(31), Err(DspError::OrderTooLarge { .. })));
    /// # Ok::<(), DspError>(())
    /// ```
    pub fn new(order: u32) -> Result<Self, DspError> {
        if order > Self::MAX_ORDER {
            return Err(DspError::OrderTooLarge {
                order,
                max: Self::MAX_ORDER,
            });
        }
        let size = 1_usize << order;
        let mut planner = FftPlanner::new();
        let forward = planner.plan_fft_forward(size);
        let inverse = planner.plan_fft_inverse(size);
        let mut real_planner = RealFftPlanner::new();
        let real_forward = real_planner.plan_fft_forward(size);
        let real_inverse = real_planner.plan_fft_inverse(size);
        let scratch_len = [
            forward.get_inplace_scratch_len(),
            inverse.get_inplace_scratch_len(),
            real_forward.get_scratch_len(),
            real_inverse.get_scratch_len(),
        ]
        .into_iter()
        .max()
        .unwrap_or(0);
        let mut scratch = vec_with_capacity("FFT scratch", scratch_len)?;
        scratch.resize(scratch_len, Complex::default());
        let mut real = vec_with_capacity("FFT real buffer", size)?;
        real.resize(size, 0.0);
        let mut spectrum = vec_with_capacity("FFT spectrum", size / 2 + 1)?;
        spectrum.resize(size / 2 + 1, Complex::default());
        Ok(Self {
            order,
            size,
            forward,
            inverse,
            real_forward,
            real_inverse,
            scratch,
            real,
            spectrum,
        })
    }

    /// The number of points, `2^order`.
    ///
    /// ```
    /// assert_eq!(syrp_dsp_core::Fft::new(0)?.size(), 1);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    #[doc(alias = "getSize")]
    #[must_use]
    pub fn size(&self) -> usize {
        self.size
    }

    /// The order given to [`new`](Self::new).
    ///
    /// ```
    /// assert_eq!(syrp_dsp_core::Fft::new(5)?.order(), 5);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    #[must_use]
    pub fn order(&self) -> u32 {
        self.order
    }

    /// An out-of-place complex FFT of the first `size()` values of `input` into the first
    /// `size()` values of `output`: forward, or inverse (scaled by `1/N`) when `inverse`.
    ///
    /// # Panics
    /// If either slice holds fewer than `size()` values.
    ///
    /// ```
    /// use syrp_dsp_core::{Complex, Fft};
    /// let mut fft = Fft::new(2)?;
    /// let input = [Complex::new(1.0_f32, 0.0); 4];
    /// let mut out = [Complex::default(); 4];
    /// fft.perform(&input, &mut out, false);
    /// assert_eq!(out[0], Complex::new(4.0, 0.0)); // DC only
    /// let mut back = [Complex::default(); 4];
    /// fft.perform(&out, &mut back, true);
    /// assert_eq!(back, input);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    pub fn perform(&mut self, input: &[Complex<f32>], output: &mut [Complex<f32>], inverse: bool) {
        let n = self.size;
        assert!(
            input.len() >= n && output.len() >= n,
            "FFT of {n} points needs {n} input and output values, got {} and {}",
            input.len(),
            output.len()
        );
        let out = &mut output[..n];
        out.copy_from_slice(&input[..n]);
        let plan = if inverse {
            &self.inverse
        } else {
            &self.forward
        };
        let scratch_len = plan.get_inplace_scratch_len();
        plan.process_with_scratch(out, &mut self.scratch[..scratch_len]);
        if inverse {
            let scale = 1.0 / n as f32;
            out.iter_mut().for_each(|c| *c *= scale);
        }
    }

    /// An in-place forward transform of real data. `data` holds `2 * size()` values; the
    /// first `size()` are the input samples. On return `data` holds all `size()` complex bins
    /// as interleaved `re, im` pairs, negative frequencies included.
    ///
    /// `only_non_negative_frequencies` is a hint, as in JUCE: with it the caller promises to
    /// read only bins `0..=size()/2`. Syrp fills every bin either way (the negative ones are
    /// the conjugates of the positive ones).
    ///
    /// # Panics
    /// If `data` holds fewer than `2 * size()` values.
    ///
    /// ```
    /// let mut fft = syrp_dsp_core::Fft::new(2)?;
    /// let mut data = [1.0_f32, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0];
    /// fft.perform_real_only_forward_transform(&mut data, true);
    /// assert_eq!(data[..2], [4.0, 0.0]);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    #[doc(alias = "performRealOnlyForwardTransform")]
    pub fn perform_real_only_forward_transform(
        &mut self,
        data: &mut [f32],
        only_non_negative_frequencies: bool,
    ) {
        let _ = only_non_negative_frequencies;
        self.real_forward_into_spectrum(data);
        let n = self.size;
        for (k, pair) in data[..2 * n].chunks_exact_mut(2).enumerate() {
            let c = if k <= n / 2 {
                self.spectrum[k]
            } else {
                self.spectrum[n - k].conj()
            };
            pair[0] = c.re;
            pair[1] = c.im;
        }
    }

    /// The inverse of [`perform_real_only_forward_transform`](Self::perform_real_only_forward_transform),
    /// scaled by `1/N`. Reads bins `0..=size()/2` from the interleaved pairs in `data` (which
    /// must still hold `2 * size()` values) and writes the `size()` real samples to the first
    /// half; the second half is left unspecified. The imaginary parts of bin 0 and, for
    /// even sizes, bin `size()/2` are taken as zero, since a real signal cannot have them.
    ///
    /// # Panics
    /// If `data` holds fewer than `2 * size()` values.
    ///
    /// ```
    /// let mut fft = syrp_dsp_core::Fft::new(2)?;
    /// let mut data = [4.0_f32, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]; // DC bin of 4
    /// fft.perform_real_only_inverse_transform(&mut data);
    /// assert_eq!(data[..4], [1.0; 4]);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    #[doc(alias = "performRealOnlyInverseTransform")]
    pub fn perform_real_only_inverse_transform(&mut self, data: &mut [f32]) {
        let n = self.size;
        assert!(
            data.len() >= 2 * n,
            "real FFT of {n} points needs {} values, got {}",
            2 * n,
            data.len()
        );
        for (k, bin) in self.spectrum.iter_mut().enumerate() {
            *bin = Complex::new(data[2 * k], data[2 * k + 1]);
        }
        self.spectrum[0].im = 0.0;
        if n.is_multiple_of(2) {
            self.spectrum[n / 2].im = 0.0;
        }
        let scratch_len = self.real_inverse.get_scratch_len();
        let done = self.real_inverse.process_with_scratch(
            &mut self.spectrum,
            &mut self.real,
            &mut self.scratch[..scratch_len],
        );
        debug_assert!(done.is_ok(), "buffers are sized in new: {done:?}");
        let scale = 1.0 / n as f32;
        for (d, s) in data[..n].iter_mut().zip(&self.real) {
            *d = s * scale;
        }
    }

    /// Replaces the first `size()` samples in `data` (which holds `2 * size()` values) with
    /// the magnitude of each forward-transform bin: `data[k] = |X[k]|` for `k < size()`.
    /// With `ignore_negative_frequencies` only bins `0..=size()/2` are written and the rest
    /// of the first half is set to zero. The second half of `data` is set to zero (Syrp's
    /// choice; the JUCE documentation leaves it open).
    ///
    /// # Panics
    /// If `data` holds fewer than `2 * size()` values.
    ///
    /// ```
    /// let mut fft = syrp_dsp_core::Fft::new(2)?;
    /// let mut data = [0.0_f32, 1.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0]; // a sine at bin 1
    /// fft.perform_frequency_only_forward_transform(&mut data, false);
    /// assert_eq!(data[..4], [0.0, 2.0, 0.0, 2.0]);
    /// # Ok::<(), syrp_dsp_core::DspError>(())
    /// ```
    #[doc(alias = "performFrequencyOnlyForwardTransform")]
    pub fn perform_frequency_only_forward_transform(
        &mut self,
        data: &mut [f32],
        ignore_negative_frequencies: bool,
    ) {
        self.real_forward_into_spectrum(data);
        let n = self.size;
        for (k, d) in data[..n].iter_mut().enumerate() {
            *d = if k <= n / 2 {
                self.spectrum[k].norm()
            } else if ignore_negative_frequencies {
                0.0
            } else {
                self.spectrum[n - k].norm()
            };
        }
        data[n..2 * n].fill(0.0);
    }

    /// Transforms the first `size()` samples of `data` into `self.spectrum`.
    fn real_forward_into_spectrum(&mut self, data: &[f32]) {
        let n = self.size;
        assert!(
            data.len() >= 2 * n,
            "real FFT of {n} points needs {} values, got {}",
            2 * n,
            data.len()
        );
        self.real.copy_from_slice(&data[..n]);
        let scratch_len = self.real_forward.get_scratch_len();
        let done = self.real_forward.process_with_scratch(
            &mut self.real,
            &mut self.spectrum,
            &mut self.scratch[..scratch_len],
        );
        debug_assert!(done.is_ok(), "buffers are sized in new: {done:?}");
    }
}

impl fmt::Debug for Fft {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Fft")
            .field("order", &self.order)
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

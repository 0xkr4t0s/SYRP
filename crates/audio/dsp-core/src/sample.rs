// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`DspSample`]: the lane access the processor model needs on top of [`Sample`].

use syrp_audio_api::AudioSample;
use syrp_simd::{F32x4, F32x8, F64x2, F64x4, Sample};

mod sealed {
    pub trait Sealed {}
}

/// A [`Sample`] whose element type audio views can hold (`f32` or `f64`), with lane access.
///
/// It is implemented for exactly the `syrp-simd` sample types (`f32`, `f64`, [`F32x4`],
/// [`F32x8`], [`F64x2`], [`F64x4`]) and is sealed. [`ProcessorDuplicator`] uses it to pack
/// channels into lanes and back.
///
/// [`ProcessorDuplicator`]: crate::ProcessorDuplicator
///
/// ```
/// use syrp_dsp_core::DspSample;
/// use syrp_simd::F32x4;
/// let p = F32x4::from_fn(|i| i as f32 * 10.0);
/// assert_eq!(p.extract(3), 30.0);
/// assert_eq!(<f32 as DspSample>::from_fn(|_| 1.5), 1.5);
/// ```
pub trait DspSample: Sample<Element: AudioSample> + sealed::Sealed {
    /// A value whose lane `i` is `f(i)`, for `i` in `0..LANES` in order.
    ///
    /// ```
    /// use syrp_dsp_core::DspSample;
    /// use syrp_simd::F64x2;
    /// assert_eq!(F64x2::from_fn(|i| i as f64).to_array(), [0.0, 1.0]);
    /// ```
    fn from_fn(f: impl FnMut(usize) -> Self::Element) -> Self;

    /// Lane `i`.
    ///
    /// # Panics
    /// If `i >= LANES`.
    ///
    /// ```
    /// use syrp_dsp_core::DspSample;
    /// assert_eq!(2.5_f64.extract(0), 2.5);
    /// ```
    fn extract(self, i: usize) -> Self::Element;

    /// `samples` seen as a slice of `Self` when `Self` is the scalar element type (one lane),
    /// so one-lane processors run on a channel in place; `None` for lane packs.
    ///
    /// ```
    /// use syrp_dsp_core::DspSample;
    /// use syrp_simd::F32x4;
    /// let mut ch = [0.0_f32; 3];
    /// assert_eq!(<f32 as DspSample>::scalar_slice_mut(&mut ch).map(|s| s.len()), Some(3));
    /// assert!(F32x4::scalar_slice_mut(&mut ch).is_none());
    /// ```
    fn scalar_slice_mut(samples: &mut [Self::Element]) -> Option<&mut [Self]>;
}

// Macro justification: the two scalar impls and the four pack impls differ only in the type
// name; generating them keeps the six in lockstep. One line per method.
#[rustfmt::skip]
macro_rules! scalar_impl {
    ($($t:ty),+) => {$(
        impl sealed::Sealed for $t {}
        impl DspSample for $t {
            #[inline(always)] fn from_fn(mut f: impl FnMut(usize) -> $t) -> Self { f(0) }
            #[inline(always)] fn extract(self, i: usize) -> $t { assert!(i == 0, "lane {i} out of range (1 lane)"); self }
            #[inline(always)] fn scalar_slice_mut(samples: &mut [$t]) -> Option<&mut [$t]> { Some(samples) }
        }
    )+};
}

#[rustfmt::skip]
macro_rules! pack_impl {
    ($($t:ty),+) => {$(
        impl sealed::Sealed for $t {}
        impl DspSample for $t {
            #[inline(always)] fn from_fn(f: impl FnMut(usize) -> Self::Element) -> Self { Self::from_array(core::array::from_fn(f)) }
            #[inline(always)] fn extract(self, i: usize) -> Self::Element { self.to_array()[i] }
            #[inline(always)] fn scalar_slice_mut(_: &mut [Self::Element]) -> Option<&mut [Self]> { None }
        }
    )+};
}

scalar_impl!(f32, f64);
pack_impl!(F32x4, F32x8, F64x2, F64x4);

/// A scalar sample type, `f32` or `f64`, that tables and helpers are generic over. Values
/// are computed in `f64` and rounded to nearest on the way in.
///
/// ```
/// use syrp_dsp_core::DspFloat;
/// assert_eq!(f32::from_f64(0.1), 0.1_f32);
/// assert_eq!(0.5_f32.to_f64(), 0.5);
/// ```
pub trait DspFloat: DspSample<Element = Self> {
    /// `value` rounded to the nearest `Self`.
    ///
    /// ```
    /// use syrp_dsp_core::DspFloat;
    /// assert_eq!(f64::from_f64(1.5), 1.5);
    /// ```
    fn from_f64(value: f64) -> Self;

    /// `self` as `f64` (exact).
    ///
    /// ```
    /// use syrp_dsp_core::DspFloat;
    /// assert_eq!(2.0_f64.to_f64(), 2.0);
    /// ```
    fn to_f64(self) -> f64;
}

impl DspFloat for f32 {
    #[inline(always)]
    fn from_f64(value: f64) -> Self {
        value as f32
    }
    #[inline(always)]
    fn to_f64(self) -> f64 {
        f64::from(self)
    }
}

impl DspFloat for f64 {
    #[inline(always)]
    fn from_f64(value: f64) -> Self {
        value
    }
    #[inline(always)]
    fn to_f64(self) -> f64 {
        self
    }
}

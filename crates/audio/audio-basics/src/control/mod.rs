// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Control-rate DSP (master plan §5.8, ADR 0003 D5): decibel conversion and parameter
//! smoothing, written once against [`syrp_simd::Sample`] so that a pack type such as
//! `SmoothedValue<F32x8>` smooths eight parameters (or eight voices' parameters) in one pass.
//!
//! - [`decibels`]: gain and decibel conversion for `f32` and `f64`, in the role of JUCE's
//!   `Decibels`.
//! - [`SmoothedValue`]: a linear or multiplicative ramp toward a target, in the role of JUCE's
//!   `SmoothedValue`, with an optional control rate ([`SmoothedValue::with_control_rate`]).
//! - [`Adsr`]: a linear attack, decay, sustain, release envelope, in the role of JUCE's `ADSR`.
//!
//! Module notes (WP-2.1-03; the crate `AGENTS.md` belongs to WP-2.1-01):
//! - Lane-generic methods are `#[inline(always)]`, so they compile for the level of the
//!   `syrp_simd::dispatch!` body that calls them (the `syrp-simd` inlining rule).
//! - Per-lane state changes go through masks and [`Sample::select`]; the only branches are
//!   uniform ones (no lane is active, a control-rate segment is due).
//! - Thread affinity: any thread. [`SmoothedValue`] and [`Adsr`] are plain `Copy` data that
//!   never allocate, lock or block, and implement `syrp_rt::RtSafe` (proved with
//!   `syrp_rt::no_alloc` in `tests/control.rs`). The decibel conversions are RT-safe too;
//!   [`decibels::to_string`] builds a display `String` and is not for the audio thread.
//! - `benches/control.rs` counts Callgrind instructions for eight `Adsr<f32>` against one
//!   `Adsr<F32x8>`, and for `SmoothedValue` per sample against a control rate of 16
//!   (`cargo bench -p syrp-audio-basics --bench control`; needs Valgrind).
//! - Parity notes: a [`Linear`] ramp computes each value back from the target
//!   (`target - step * samples_left`) instead of adding the step, so long `f32` ramps do not
//!   accumulate rounding; values agree with repeated addition up to that rounding. A
//!   [`Multiplicative`] ramp multiplies by a per-sample ratio, so in `f32` it drifts from the
//!   exact curve by up to about `steps * 2^-24` relative before landing on the target.
//!   [`decibels::to_string`] shows the floor itself, and NaN, as silence; its cutoff is always
//!   the default −100 dB floor (the spec's signature has no floor parameter).
//!   [`SmoothedValue::reset`] truncates: the ramp is `floor(sample_rate * ramp_seconds)`
//!   samples, so 10.9 samples' worth of time ramps over 10.
//!   [`Adsr`] segments are linear and have not been compared with JUCE yet.
//!
//! ```
//! use syrp_audio_basics::control::{SmoothedValue, decibels};
//! let mut gain = SmoothedValue::<f32>::new(0.0);
//! gain.reset_steps(4);
//! gain.set_target(decibels::decibels_to_gain(0.0, -100.0));
//! let ramp: Vec<f32> = (0..4).map(|_| gain.next_value()).collect();
//! assert_eq!(ramp, [0.25, 0.5, 0.75, 1.0]);
//! ```

mod adsr;
pub mod decibels;
mod smoothed;

use syrp_rt::RtSafe;
use syrp_simd::{F32x4, F32x8, F64x2, F64x4, Sample};

pub use adsr::{Adsr, AdsrParams};
pub use smoothed::{Linear, Mapping, Multiplicative, SmoothedValue};

// SAFETY: `SmoothedValue` and `Adsr` hold only `Copy` lane values, masks, integers and a
// `PhantomData`, so they have no destructor and moving them never allocates. Every method
// except construction is arithmetic on those fields: no allocation, lock, blocking or I/O
// (the `b6_` tests run them under `syrp_rt::no_alloc`).
unsafe impl<S: ControlSample, M: Mapping> RtSafe for SmoothedValue<S, M> {}
// SAFETY: as above.
unsafe impl<S: ControlSample> RtSafe for Adsr<S> {}

/// A [`Sample`] type the control-rate types accept: `f32`, `f64` and every `syrp-simd` lane
/// pack (`F32x4`, `F32x8`, `F64x2`, `F64x4`), which is every type that implements `Sample`.
///
/// It adds the per-lane operations `Sample` does not have (a power function for the
/// multiplicative mapping, a lane count as a float). The trait is sealed.
///
/// ```
/// use syrp_audio_basics::control::{ControlSample, SmoothedValue};
/// fn half_way<S: ControlSample>(from: S, to: S) -> S {
///     let mut v = SmoothedValue::<S>::new(from);
///     v.reset_steps(2);
///     v.set_target(to);
///     v.next_value()
/// }
/// assert_eq!(half_way(0.0_f64, 1.0), 0.5);
/// ```
pub trait ControlSample: Sample + lanes::Lanes {}

mod lanes {
    use syrp_simd::Sample;

    /// Per-lane helpers behind [`ControlSample`](super::ControlSample).
    pub trait Lanes: Sample {
        /// `n` in every lane.
        fn from_count(n: u32) -> Self;
        /// `value` in every lane, rounded to nearest when narrowing.
        fn from_f64(value: f64) -> Self;
        /// Lane-wise rounding to the nearest whole number (half away from zero).
        fn round(self) -> Self;
        /// The smallest lane, as `f64`.
        fn min_lane(self) -> f64;
        /// Lane-wise `self.powf(exponent)`.
        fn powf(self, exponent: Self) -> Self;
    }
}

impl ControlSample for f32 {}
impl ControlSample for f64 {}
impl ControlSample for F32x4 {}
impl ControlSample for F32x8 {}
impl ControlSample for F64x2 {}
impl ControlSample for F64x4 {}

impl lanes::Lanes for f32 {
    #[inline(always)]
    fn from_count(n: u32) -> Self {
        n as f32
    }
    #[inline(always)]
    fn from_f64(value: f64) -> Self {
        value as f32
    }
    #[inline(always)]
    fn round(self) -> Self {
        f32::round(self)
    }
    #[inline(always)]
    fn min_lane(self) -> f64 {
        f64::from(self)
    }
    #[inline(always)]
    fn powf(self, exponent: Self) -> Self {
        f32::powf(self, exponent)
    }
}

impl lanes::Lanes for f64 {
    #[inline(always)]
    fn from_count(n: u32) -> Self {
        f64::from(n)
    }
    #[inline(always)]
    fn from_f64(value: f64) -> Self {
        value
    }
    #[inline(always)]
    fn round(self) -> Self {
        f64::round(self)
    }
    #[inline(always)]
    fn min_lane(self) -> f64 {
        self
    }
    #[inline(always)]
    fn powf(self, exponent: Self) -> Self {
        f64::powf(self, exponent)
    }
}

// Macro justification: the four pack implementations differ only in the pack and element
// type; each is five short methods that go through the scalar implementation lane by lane.
macro_rules! pack_lanes {
    ($($pack:ident: $elem:ty),*) => {$(
        impl lanes::Lanes for $pack {
            #[inline(always)]
            fn from_count(n: u32) -> Self {
                Self::splat(<$elem as lanes::Lanes>::from_count(n))
            }
            #[inline(always)]
            fn from_f64(value: f64) -> Self {
                Self::splat(<$elem as lanes::Lanes>::from_f64(value))
            }
            #[inline(always)]
            fn round(self) -> Self {
                Self::from_array(self.to_array().map(<$elem>::round))
            }
            #[inline(always)]
            fn min_lane(self) -> f64 {
                self.to_array().into_iter().fold(f64::INFINITY, |m, x| m.min(f64::from(x)))
            }
            #[inline(always)]
            fn powf(self, exponent: Self) -> Self {
                Self::from_array(core::array::from_fn(|i| self.lane(i).powf(exponent.lane(i))))
            }
        }
    )*};
}
pack_lanes!(F32x4: f32, F32x8: f32, F64x2: f64, F64x4: f64);

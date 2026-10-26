// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Control-rate DSP (master plan §5.8, ADR 0003 D5): decibel conversion and parameter
//! smoothing, written once against [`syrp_simd::Sample`] so that a pack type such as
//! `SmoothedValue<F32x8>` smooths eight parameters (or eight voices' parameters) in one pass.
//!
//! - [`decibels`]: gain and decibel conversion for `f32` and `f64`, in the role of JUCE's
//!   `Decibels`.
//! - [`SmoothedValue`]: a linear or multiplicative ramp toward a target, in the role of JUCE's
//!   `SmoothedValue`, with an optional control rate ([`SmoothedValue::with_control_rate`]).
//!
//! Module notes (WP-2.1-03; the crate `AGENTS.md` belongs to WP-2.1-01):
//! - Lane-generic methods are `#[inline(always)]`, so they compile for the level of the
//!   `syrp_simd::dispatch!` body that calls them (the `syrp-simd` inlining rule).
//! - Per-lane state changes go through masks and [`Sample::select`]; the only branches are
//!   uniform ones (no lane is smoothing, the control-rate segment counter).
//! - Nothing here allocates after construction, except [`decibels::to_string`], which builds
//!   a display `String` and is not for the audio thread.
//! - Parity notes: a [`Linear`] ramp computes each value back from the target
//!   (`target - step * samples_left`) instead of adding the step, so long `f32` ramps do not
//!   accumulate rounding; values agree with repeated addition up to that rounding. A
//!   [`Multiplicative`] ramp multiplies by a per-sample ratio, so in `f32` it drifts from the
//!   exact curve by up to about `steps * 2^-24` relative before landing on the target.
//!   [`decibels::to_string`] shows the floor itself, and NaN, as silence; its cutoff is always
//!   the default −100 dB floor (the spec's signature has no floor parameter).
//!   [`SmoothedValue::reset`] truncates: the ramp is `floor(sample_rate * ramp_seconds)`
//!   samples, so 10.9 samples' worth of time ramps over 10.
//!
//! Known gaps: `Adsr` (spec B4), the B5 matrix for it, and the allocation tests, `RtSafe` and
//! benches (B6) land in the next increment of WP-2.1-03. Tests are in `tests/control.rs`.
//!
//! ```
//! use syrp_audio_basics::control::{SmoothedValue, decibels};
//! let mut gain = SmoothedValue::<f32>::new(0.0);
//! gain.reset_steps(4);
//! gain.set_target(decibels::decibels_to_gain(0.0, -100.0));
//! let ramp: Vec<f32> = (0..4).map(|_| gain.next_value()).collect();
//! assert_eq!(ramp, [0.25, 0.5, 0.75, 1.0]);
//! ```

pub mod decibels;
mod smoothed;

use syrp_simd::{F32x4, F32x8, F64x2, F64x4, Sample};

pub use smoothed::{Linear, Mapping, Multiplicative, SmoothedValue};

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
    fn powf(self, exponent: Self) -> Self {
        f64::powf(self, exponent)
    }
}

// Macro justification: the four pack implementations differ only in the pack and element
// type; each is two one-line methods that go through the scalar implementation lane by lane.
macro_rules! pack_lanes {
    ($($pack:ident: $elem:ty),*) => {$(
        impl lanes::Lanes for $pack {
            #[inline(always)]
            fn from_count(n: u32) -> Self {
                Self::splat(<$elem as lanes::Lanes>::from_count(n))
            }
            #[inline(always)]
            fn powf(self, exponent: Self) -> Self {
                Self::from_array(core::array::from_fn(|i| self.lane(i).powf(exponent.lane(i))))
            }
        }
    )*};
}
pack_lanes!(F32x4: f32, F32x8: f32, F64x2: f64, F64x4: f64);

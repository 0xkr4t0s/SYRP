// SPDX-License-Identifier: MIT OR Apache-2.0
//! The L0 SIMD layer of Syrp (ADR 0003 D5).
//!
//! DSP code is written once against [`Sample`] and runs as `f32`, `f64` or a lane pack
//! ([`F32x4`], [`F32x8`], [`F64x2`], [`F64x4`]), so `Voice<F32x8>` renders eight voices in one
//! pass. [`Level::detect`] picks the instruction set once and [`dispatch!`] compiles a body for
//! it, and [`AlignedBuf`] gives 64-byte aligned storage. Dispatch is built on [`fearless_simd`],
//! re-exported for code that wants explicit vectors. The denormal guard and slice kernels with
//! scalar references follow in the next increment of WP-0.2-14.
//!
//! ```
//! use syrp_simd::{dispatch, F32x4, Level, Sample};
//! /// A one-pole smoother, written once for every sample type.
//! fn smooth<S: Sample>(state: &mut S, target: S, coeff: S) -> S {
//!     *state = coeff.mul_add(*state - target, target);
//!     *state
//! }
//! let mut s = F32x4::zero();
//! let target = F32x4::from_array([1.0, 2.0, 3.0, 4.0]);
//! let y = dispatch!(Level::detect(), _simd => smooth(&mut s, target, F32x4::splat(0.5)),
//!     scalar => smooth(&mut s, target, F32x4::splat(0.5)));
//! assert_eq!(y.to_array(), [0.5, 1.0, 1.5, 2.0]);
//! ```

mod aligned;
mod level;
mod sample;

pub use aligned::{ALIGN, AlignedBuf, Zeroable};
pub use fearless_simd;
pub use level::{ENV_VAR, Level};
pub use sample::{
    F32x4, F32x8, F64x2, F64x4, Mask, Mask32x4, Mask32x8, Mask64x2, Mask64x4, Sample,
};

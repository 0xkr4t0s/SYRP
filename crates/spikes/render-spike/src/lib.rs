// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Renderer spike (WP-0.2-09, master plan §11, risk R2): which CPU rasteriser becomes Syrp's
//! reference renderer.
//!
//! - [`scenes`] builds 20 [`Scene`]s as [`syrp_gfx_api::DisplayList`]s (paths, strokes,
//!   gradients, text-like glyph outlines, images, clips, transparency layers), each meant to be
//!   rendered at every scale in [`SCALES`];
//! - [`compare`] is the golden comparator: a per-channel [`Tolerance`], a maximum ratio of
//!   differing pixels, and a diff PNG from the [`GoldenReport`].
//!
//! This is a throwaway spike: the findings live in `docs/design/0004-renderer-spike.md`.
//!
//! ```
//! use syrp_render_spike::{Tolerance, compare, scenes};
//! let scene = &scenes()[0];
//! let blank = scene.target(1.25)?;
//! let report = compare(&blank, &blank.clone(), Tolerance::EXACT)?;
//! assert!(report.passed());
//! # Ok::<(), syrp_gfx_api::RenderError>(())
//! ```

mod golden;
mod scene;

pub use golden::{GoldenReport, Tolerance, compare};
pub use scene::{SCALES, Scene, scenes};

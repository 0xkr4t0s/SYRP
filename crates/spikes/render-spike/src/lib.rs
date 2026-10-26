// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Renderer spike (WP-0.2-09, master plan §11, risk R2): which CPU rasteriser becomes Syrp's
//! reference renderer.
//!
//! - [`scenes`] builds 20 [`Scene`]s as [`syrp_gfx_api::DisplayList`]s (paths, strokes,
//!   gradients, text-like glyph outlines, images, clips, transparency layers), each meant to be
//!   rendered at every scale in [`SCALES`];
//! - [`VelloCpuRenderer`] and [`TinySkiaRenderer`] replay the same list through `vello_cpu` and
//!   `tiny-skia`, both implementing [`syrp_gfx_api::Renderer`]; [`Backend`] names them;
//! - [`compare`] is the golden comparator: a per-channel [`Tolerance`], a maximum ratio of
//!   differing pixels, and a diff PNG from the [`GoldenReport`].
//!
//! This is a throwaway spike: the findings live in `docs/design/0004-renderer-spike.md`.
//!
//! ```
//! use syrp_gfx_api::Renderer;
//! use syrp_render_spike::{Backend, Tolerance, compare, scenes};
//! let scene = &scenes()[0];
//! let mut a = scene.target(1.0)?;
//! let mut b = scene.target(1.0)?;
//! Backend::VelloCpu.renderer().render(scene.list(), &mut a)?;
//! Backend::TinySkia.renderer().render(scene.list(), &mut b)?;
//! let report = compare(&a, &b, Tolerance::new(48, 0.05))?;
//! assert!(report.passed(), "{report:?}");
//! # Ok::<(), syrp_gfx_api::RenderError>(())
//! ```

mod assets;
mod golden;
mod replay;
mod scene;
mod skia;
mod vello;

pub use golden::{GoldenReport, Tolerance, compare};
pub use scene::{SCALES, Scene, scenes};
pub use skia::TinySkiaRenderer;
pub use vello::{VelloCpuRenderer, VelloMode};

use syrp_gfx_api::Renderer;

/// The renderers under comparison.
/// ```
/// use syrp_render_spike::Backend;
/// assert_eq!(Backend::ALL.map(Backend::name), ["vello_cpu", "tiny-skia"]);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Backend {
    /// `vello_cpu` in [`VelloMode::Deterministic`].
    VelloCpu,
    /// `tiny-skia`.
    TinySkia,
}

impl Backend {
    /// Every backend, in report order.
    /// ```
    /// assert_eq!(syrp_render_spike::Backend::ALL.len(), 2);
    /// ```
    pub const ALL: [Backend; 2] = [Backend::VelloCpu, Backend::TinySkia];

    /// The crate name, as used in reports.
    /// ```
    /// assert_eq!(syrp_render_spike::Backend::TinySkia.name(), "tiny-skia");
    /// ```
    pub fn name(self) -> &'static str {
        match self {
            Backend::VelloCpu => "vello_cpu",
            Backend::TinySkia => "tiny-skia",
        }
    }

    /// A fresh renderer for this backend, holding the spike's built-in images and outline font.
    /// ```
    /// let _r = syrp_render_spike::Backend::VelloCpu.renderer();
    /// ```
    pub fn renderer(self) -> Box<dyn Renderer> {
        match self {
            Backend::VelloCpu => Box::new(VelloCpuRenderer::new(VelloMode::Deterministic)),
            Backend::TinySkia => Box::new(TinySkiaRenderer::new()),
        }
    }
}

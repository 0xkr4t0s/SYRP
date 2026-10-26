// SPDX-License-Identifier: MIT OR Apache-2.0
//! The Syrp drawing contract (master plan §5.7, ADR 0003).
//!
//! - [`DisplayList`]: recorded drawing ops (an op-tag stream, per-kind payload arrays and shared
//!   arenas) read back as borrowed [`Op`] views, with [`Glyph`] runs, [`BlendMode`] layers and
//!   [`ResamplingQuality`];
//! - [`Path`] (a verb array plus a point array) with [`PathView`], and [`Stroke`] parameters;
//! - [`Brush`]: a solid [`Colour`] or a linear, radial or two-point-conical gradient;
//! - opaque [`ImageHandle`] and [`FontHandle`] ids that renderers resolve.
//! - [`Renderer`] draws a list into a [`RenderTarget`] (physical pixels plus an explicit scale
//!   factor), failing with a [`RenderError`];
//! - [`DisplayList::content_hash`], a fixed seedless 64-bit hash of the ops, and (feature
//!   `serde`) op-by-op JSON, both from the same [`Op`] views.
//!
//! Following ADR 0003 D3, every type keeps its fields private: owned types expose
//! constructors and accessors, and borrowed views ([`PathView`], [`StrokeRef`], [`BrushRef`])
//! are what a display list hands back, so storage can change without a contract change.
//!
//! ```
//! use syrp_geom::Point;
//! use syrp_gfx_api::{Brush, Colour, GradientStop, Path, Stroke};
//! let mut tri = Path::new();
//! tri.move_to(0.0, 0.0);
//! tri.line_to(10.0, 0.0);
//! tri.line_to(5.0, 8.0);
//! tri.close();
//! let fill = Brush::radial(Point::new(5.0, 4.0), 6.0, [GradientStop::new(0.0, Colour::WHITE)]);
//! let outline = Stroke::new(1.5);
//! assert_eq!(tri.bounds().unwrap().height(), 8.0);
//! # let _ = (fill, outline);
//! ```

mod brush;
mod encode;
mod list;
mod path;
mod render;

pub use brush::{Brush, BrushKind, BrushRef, Colour, GradientStop};
pub use list::{BlendMode, DisplayList, Glyph, Op, ResamplingQuality};
pub use path::{Cap, FillRule, Join, Path, PathEl, PathVerb, PathView, Stroke, StrokeRef};
pub use render::{RenderError, RenderTarget, Renderer};

/// An opaque image id. Its owner (an image cache) assigns the raw value; renderers resolve it.
/// ```
/// let h = syrp_gfx_api::ImageHandle::from_raw(7);
/// assert_eq!(h.raw(), 7);
/// ```
#[doc(alias = "Image")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImageHandle(u64);

impl ImageHandle {
    /// Wraps a raw id.
    /// ```
    /// let _ = syrp_gfx_api::ImageHandle::from_raw(1);
    /// ```
    pub const fn from_raw(raw: u64) -> Self {
        ImageHandle(raw)
    }

    /// The raw id.
    /// ```
    /// assert_eq!(syrp_gfx_api::ImageHandle::from_raw(1).raw(), 1);
    /// ```
    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// An opaque font id (a typeface at a set of variation settings), resolved by renderers.
/// ```
/// let h = syrp_gfx_api::FontHandle::from_raw(3);
/// assert_eq!(h.raw(), 3);
/// ```
#[doc(alias = "Typeface")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontHandle(u64);

impl FontHandle {
    /// Wraps a raw id.
    /// ```
    /// let _ = syrp_gfx_api::FontHandle::from_raw(1);
    /// ```
    pub const fn from_raw(raw: u64) -> Self {
        FontHandle(raw)
    }

    /// The raw id.
    /// ```
    /// assert_eq!(syrp_gfx_api::FontHandle::from_raw(1).raw(), 1);
    /// ```
    pub const fn raw(self) -> u64 {
        self.0
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0
//! Render targets, the renderer trait and render errors.

use std::fmt;

use syrp_error::{Diagnostic, ErrorCode};

use crate::list::DisplayList;
use crate::{FontHandle, ImageHandle};

/// What a renderer draws into: a size in physical pixels and an explicit scale factor
/// (physical pixels per logical unit) that renderers apply before the list's own transforms.
///
/// A target made with [`with_pixels`](RenderTarget::with_pixels) owns a CPU pixel buffer:
/// 8-bit RGBA, premultiplied alpha, rows of `width * 4` bytes, top row first, starting fully
/// transparent. A target made with [`new`](RenderTarget::new) has no buffer and stands for a
/// surface the renderer owns (a window's swap chain, say).
/// ```
/// use syrp_gfx_api::RenderTarget;
/// let t = RenderTarget::from_logical(200.0, 100.0, 1.5, true).unwrap();
/// assert_eq!((t.width(), t.height(), t.scale()), (300, 150, 1.5));
/// assert_eq!(t.pixels().unwrap().len(), 300 * 150 * 4);
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct RenderTarget {
    width: u32,
    height: u32,
    scale: f64,
    pixels: Option<Vec<u8>>,
}

/// Physical pixels for a logical length: `logical * scale`, rounded up so the target covers
/// the whole logical area, except that a product within floating-point noise (relative
/// `1e-9`) of a whole number snaps to it, so `10.0 * 1.1` gives 11, not 12.
fn physical(logical: f64, scale: f64) -> Result<u32, RenderError> {
    let p = logical * scale;
    if !(p.is_finite() && p >= 0.0) {
        return Err(RenderError::InvalidTarget);
    }
    let near = p.round();
    let p = if (p - near).abs() <= 1e-9 * near.max(1.0) {
        near
    } else {
        p.ceil()
    };
    if p > f64::from(u32::MAX) {
        return Err(RenderError::InvalidTarget);
    }
    Ok(p as u32)
}

fn check_scale(scale: f64) -> Result<f64, RenderError> {
    if scale.is_finite() && scale > 0.0 {
        Ok(scale)
    } else {
        Err(RenderError::InvalidTarget)
    }
}

fn buffer(width: u32, height: u32) -> Result<Vec<u8>, RenderError> {
    let len = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4));
    match len {
        Some(n) if n <= isize::MAX as usize => Ok(vec![0; n]),
        _ => Err(RenderError::InvalidTarget),
    }
}

impl RenderTarget {
    /// A surface target of `width` x `height` physical pixels with no CPU buffer.
    ///
    /// # Errors
    /// [`RenderError::InvalidTarget`] unless `scale` is finite and positive.
    /// ```
    /// let t = syrp_gfx_api::RenderTarget::new(3, 5, 2.0).unwrap();
    /// assert_eq!(t.logical_size(), (1.5, 2.5));
    /// assert!(t.pixels().is_none());
    /// ```
    pub fn new(width: u32, height: u32, scale: f64) -> Result<Self, RenderError> {
        let scale = check_scale(scale)?;
        Ok(RenderTarget {
            width,
            height,
            scale,
            pixels: None,
        })
    }

    /// A target with a zeroed CPU pixel buffer.
    ///
    /// # Errors
    /// [`RenderError::InvalidTarget`] for a bad `scale` or a buffer too large to address.
    /// ```
    /// let t = syrp_gfx_api::RenderTarget::with_pixels(2, 2, 1.0).unwrap();
    /// assert_eq!(t.pixels(), Some(&[0; 16][..]));
    /// ```
    pub fn with_pixels(width: u32, height: u32, scale: f64) -> Result<Self, RenderError> {
        let mut t = Self::new(width, height, scale)?;
        t.pixels = Some(buffer(width, height)?);
        Ok(t)
    }

    /// A target covering a `width` x `height` logical area at `scale`, with a CPU buffer if
    /// `pixels`. Each physical side is `logical * scale` rounded up, except that products
    /// within floating-point noise of a whole number snap to it.
    ///
    /// # Errors
    /// [`RenderError::InvalidTarget`] for a bad `scale`, a negative or non-finite size, or a
    /// side above `u32::MAX` pixels.
    /// ```
    /// use syrp_gfx_api::RenderTarget;
    /// let t = RenderTarget::from_logical(101.0, 10.0, 1.25, false).unwrap();
    /// assert_eq!((t.width(), t.height()), (127, 13)); // 126.25 and 12.5 round up
    /// ```
    pub fn from_logical(
        width: f64,
        height: f64,
        scale: f64,
        pixels: bool,
    ) -> Result<Self, RenderError> {
        let scale = check_scale(scale)?;
        let (w, h) = (physical(width, scale)?, physical(height, scale)?);
        if pixels {
            Self::with_pixels(w, h, scale)
        } else {
            Self::new(w, h, scale)
        }
    }

    /// Width in physical pixels.
    /// ```
    /// assert_eq!(syrp_gfx_api::RenderTarget::new(3, 5, 1.0).unwrap().width(), 3);
    /// ```
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in physical pixels.
    /// ```
    /// assert_eq!(syrp_gfx_api::RenderTarget::new(3, 5, 1.0).unwrap().height(), 5);
    /// ```
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Physical pixels per logical unit.
    /// ```
    /// assert_eq!(syrp_gfx_api::RenderTarget::new(3, 5, 1.25).unwrap().scale(), 1.25);
    /// ```
    pub fn scale(&self) -> f64 {
        self.scale
    }

    /// The size in logical units: the physical size divided by the scale (not rounded).
    /// ```
    /// assert_eq!(syrp_gfx_api::RenderTarget::new(5, 5, 2.0).unwrap().logical_size(), (2.5, 2.5));
    /// ```
    pub fn logical_size(&self) -> (f64, f64) {
        (
            f64::from(self.width) / self.scale,
            f64::from(self.height) / self.scale,
        )
    }

    /// The CPU pixel buffer, if this target has one.
    /// ```
    /// assert!(syrp_gfx_api::RenderTarget::new(1, 1, 1.0).unwrap().pixels().is_none());
    /// ```
    pub fn pixels(&self) -> Option<&[u8]> {
        self.pixels.as_deref()
    }

    /// The CPU pixel buffer, mutably (renderers write here).
    /// ```
    /// let mut t = syrp_gfx_api::RenderTarget::with_pixels(1, 1, 1.0).unwrap();
    /// t.pixels_mut().unwrap()[3] = 255;
    /// ```
    pub fn pixels_mut(&mut self) -> Option<&mut [u8]> {
        self.pixels.as_deref_mut()
    }

    /// Changes the physical size and scale (a window resize or a move to another display),
    /// reallocating and clearing the CPU buffer if there is one. On error nothing changes.
    ///
    /// # Errors
    /// As [`with_pixels`](RenderTarget::with_pixels).
    /// ```
    /// let mut t = syrp_gfx_api::RenderTarget::with_pixels(1, 1, 1.0).unwrap();
    /// t.resize(4, 2, 2.0).unwrap();
    /// assert_eq!((t.logical_size(), t.pixels().unwrap().len()), ((2.0, 1.0), 32));
    /// ```
    pub fn resize(&mut self, width: u32, height: u32, scale: f64) -> Result<(), RenderError> {
        let next = match self.pixels {
            Some(_) => Self::with_pixels(width, height, scale)?,
            None => Self::new(width, height, scale)?,
        };
        *self = next;
        Ok(())
    }
}

/// Draws a [`DisplayList`] into a [`RenderTarget`] (implemented by `syrp-render-cpu` and
/// `syrp-render-gpu`). Implementations map logical units to pixels with the target's scale,
/// resolve image and font handles themselves, and treat scopes left open as closed at the end.
/// ```
/// use syrp_gfx_api::{DisplayList, Op, RenderError, RenderTarget, Renderer};
/// struct CountOps(usize);
/// impl Renderer for CountOps {
///     fn render(&mut self, list: &DisplayList, _: &mut RenderTarget) -> Result<(), RenderError> {
///         self.0 += list.ops().filter(|op| *op != Op::Pop).count();
///         Ok(())
///     }
/// }
/// let mut target = RenderTarget::new(10, 10, 1.0)?;
/// let mut r = CountOps(0);
/// r.render(&DisplayList::new(), &mut target)?;
/// assert_eq!(r.0, 0);
/// # Ok::<(), RenderError>(())
/// ```
pub trait Renderer {
    /// Draws `list` over the current contents of `target`.
    ///
    /// # Errors
    /// A [`RenderError`] if the target is unusable, a handle does not resolve, or the backend
    /// fails.
    /// ```
    /// # use syrp_gfx_api::{DisplayList, RenderError, RenderTarget, Renderer};
    /// fn draw(r: &mut dyn Renderer, t: &mut RenderTarget) -> Result<(), RenderError> {
    ///     r.render(&DisplayList::new(), t)
    /// }
    /// ```
    fn render(&mut self, list: &DisplayList, target: &mut RenderTarget) -> Result<(), RenderError>;
}

/// Errors from building a [`RenderTarget`] or rendering, catalogued in `errors/gfx.toml`.
/// ```
/// use syrp_error::Diagnostic;
/// let e = syrp_gfx_api::RenderTarget::new(1, 1, 0.0).unwrap_err();
/// assert_eq!(e.code().as_str(), "SYRP-GFX-0001");
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RenderError {
    /// Bad scale or size, or a buffer too large (`SYRP-GFX-0001`).
    InvalidTarget,
    /// An image handle the renderer cannot resolve (`SYRP-GFX-0002`).
    UnknownImage(ImageHandle),
    /// A font handle the renderer cannot resolve (`SYRP-GFX-0003`).
    UnknownFont(FontHandle),
    /// The backend failed, for example a lost GPU device (`SYRP-GFX-0004`).
    Backend(String),
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenderError::InvalidTarget => f.write_str("invalid render target size or scale"),
            RenderError::UnknownImage(h) => write!(f, "unknown image handle {}", h.raw()),
            RenderError::UnknownFont(h) => write!(f, "unknown font handle {}", h.raw()),
            RenderError::Backend(msg) => write!(f, "render backend failed: {msg}"),
        }
    }
}

impl std::error::Error for RenderError {}

impl Diagnostic for RenderError {
    fn code(&self) -> ErrorCode {
        ErrorCode::new(match self {
            RenderError::InvalidTarget => "SYRP-GFX-0001",
            RenderError::UnknownImage(_) => "SYRP-GFX-0002",
            RenderError::UnknownFont(_) => "SYRP-GFX-0003",
            RenderError::Backend(_) => "SYRP-GFX-0004",
        })
    }
}

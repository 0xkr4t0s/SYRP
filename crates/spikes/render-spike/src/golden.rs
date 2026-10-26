// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The golden image comparator (spec B4).

use syrp_gfx_api::{RenderError, RenderTarget};

/// How far a render may drift from its golden: a pixel differs when any channel (of the
/// premultiplied RGBA8 bytes) is more than `per_channel` away, and the comparison passes when
/// differing pixels are at most `max_ratio` of all pixels.
/// ```
/// use syrp_render_spike::Tolerance;
/// let t = Tolerance::new(2, 0.001);
/// assert_eq!((t.per_channel(), t.max_ratio()), (2, 0.001));
/// assert_eq!(Tolerance::EXACT.per_channel(), 0);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tolerance {
    per_channel: u8,
    max_ratio: f64,
}

impl Tolerance {
    /// Byte-identical output.
    /// ```
    /// assert_eq!(syrp_render_spike::Tolerance::EXACT.max_ratio(), 0.0);
    /// ```
    pub const EXACT: Tolerance = Tolerance {
        per_channel: 0,
        max_ratio: 0.0,
    };

    /// A tolerance; `max_ratio` is clamped to `[0, 1]` (NaN becomes 0).
    /// ```
    /// assert_eq!(syrp_render_spike::Tolerance::new(1, 7.0).max_ratio(), 1.0);
    /// ```
    pub fn new(per_channel: u8, max_ratio: f64) -> Self {
        let max_ratio = if max_ratio.is_nan() {
            0.0
        } else {
            max_ratio.clamp(0.0, 1.0)
        };
        Self {
            per_channel,
            max_ratio,
        }
    }

    /// Largest per-channel difference that still counts as equal.
    /// ```
    /// assert_eq!(syrp_render_spike::Tolerance::new(3, 0.0).per_channel(), 3);
    /// ```
    pub fn per_channel(self) -> u8 {
        self.per_channel
    }

    /// Largest accepted fraction of differing pixels.
    /// ```
    /// assert_eq!(syrp_render_spike::Tolerance::new(0, 0.5).max_ratio(), 0.5);
    /// ```
    pub fn max_ratio(self) -> f64 {
        self.max_ratio
    }
}

/// The outcome of [`compare`]: counts, the verdict, and a diff image.
/// ```
/// use syrp_gfx_api::RenderTarget;
/// use syrp_render_spike::{Tolerance, compare};
/// let a = RenderTarget::with_pixels(2, 2, 1.0)?;
/// let r = compare(&a, &a, Tolerance::EXACT)?;
/// assert_eq!((r.differing_pixels(), r.total_pixels(), r.passed()), (0, 4, true));
/// # Ok::<(), syrp_gfx_api::RenderError>(())
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct GoldenReport {
    width: u32,
    height: u32,
    differing: u64,
    max_delta: u8,
    tolerance: Tolerance,
    diff: Vec<u8>,
}

impl GoldenReport {
    /// Pixels with some channel beyond the tolerance.
    /// ```
    /// # let a = syrp_gfx_api::RenderTarget::with_pixels(1, 1, 1.0)?;
    /// let r = syrp_render_spike::compare(&a, &a, syrp_render_spike::Tolerance::EXACT)?;
    /// assert_eq!(r.differing_pixels(), 0);
    /// # Ok::<(), syrp_gfx_api::RenderError>(())
    /// ```
    pub fn differing_pixels(&self) -> u64 {
        self.differing
    }

    /// Pixels compared (`width * height`).
    /// ```
    /// # let a = syrp_gfx_api::RenderTarget::with_pixels(3, 2, 1.0)?;
    /// let r = syrp_render_spike::compare(&a, &a, syrp_render_spike::Tolerance::EXACT)?;
    /// assert_eq!(r.total_pixels(), 6);
    /// # Ok::<(), syrp_gfx_api::RenderError>(())
    /// ```
    pub fn total_pixels(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }

    /// `differing_pixels / total_pixels` (0 for an empty image).
    /// ```
    /// # let a = syrp_gfx_api::RenderTarget::with_pixels(1, 1, 1.0)?;
    /// let r = syrp_render_spike::compare(&a, &a, syrp_render_spike::Tolerance::EXACT)?;
    /// assert_eq!(r.differing_ratio(), 0.0);
    /// # Ok::<(), syrp_gfx_api::RenderError>(())
    /// ```
    pub fn differing_ratio(&self) -> f64 {
        match self.total_pixels() {
            0 => 0.0,
            n => self.differing as f64 / n as f64,
        }
    }

    /// The largest difference seen in any channel of any pixel.
    /// ```
    /// # let a = syrp_gfx_api::RenderTarget::with_pixels(1, 1, 1.0)?;
    /// let r = syrp_render_spike::compare(&a, &a, syrp_render_spike::Tolerance::EXACT)?;
    /// assert_eq!(r.max_channel_delta(), 0);
    /// # Ok::<(), syrp_gfx_api::RenderError>(())
    /// ```
    pub fn max_channel_delta(&self) -> u8 {
        self.max_delta
    }

    /// Whether the differing ratio is within the tolerance's `max_ratio`.
    /// ```
    /// # let a = syrp_gfx_api::RenderTarget::with_pixels(1, 1, 1.0)?;
    /// let r = syrp_render_spike::compare(&a, &a, syrp_render_spike::Tolerance::EXACT)?;
    /// assert!(r.passed());
    /// # Ok::<(), syrp_gfx_api::RenderError>(())
    /// ```
    pub fn passed(&self) -> bool {
        self.differing_ratio() <= self.tolerance.max_ratio
    }

    /// The diff image as PNG bytes (8-bit RGBA, opaque): pixels within tolerance show the
    /// golden's luminance as a pale grey, differing pixels are red, brighter for larger
    /// differences.
    /// ```
    /// # let a = syrp_gfx_api::RenderTarget::with_pixels(2, 2, 1.0)?;
    /// let r = syrp_render_spike::compare(&a, &a, syrp_render_spike::Tolerance::EXACT)?;
    /// assert_eq!(&r.diff_png()?[1..4], b"PNG");
    /// # Ok::<(), syrp_gfx_api::RenderError>(())
    /// ```
    pub fn diff_png(&self) -> Result<Vec<u8>, RenderError> {
        let backend = |e: png::EncodingError| RenderError::Backend(format!("diff PNG: {e}"));
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, self.width, self.height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()
            .and_then(|mut w| w.write_image_data(&self.diff))
            .map_err(backend)?;
        Ok(out)
    }
}

/// Compares `actual` with `golden` pixel by pixel.
///
/// # Errors
/// [`RenderError::InvalidTarget`] when either target has no pixel buffer or their sizes
/// differ; [`RenderError::Backend`] when the diff image cannot be allocated.
/// ```
/// use syrp_gfx_api::RenderTarget;
/// use syrp_render_spike::{Tolerance, compare};
/// let golden = RenderTarget::with_pixels(2, 1, 1.0)?;
/// let mut actual = golden.clone();
/// actual.pixels_mut().unwrap()[0] = 9;
/// let r = compare(&actual, &golden, Tolerance::new(8, 0.0))?;
/// assert_eq!((r.differing_pixels(), r.max_channel_delta(), r.passed()), (1, 9, false));
/// assert!(compare(&actual, &golden, Tolerance::new(8, 0.5))?.passed());
/// # Ok::<(), syrp_gfx_api::RenderError>(())
/// ```
pub fn compare(
    actual: &RenderTarget,
    golden: &RenderTarget,
    tolerance: Tolerance,
) -> Result<GoldenReport, RenderError> {
    let (Some(a), Some(g)) = (actual.pixels(), golden.pixels()) else {
        return Err(RenderError::InvalidTarget);
    };
    if (actual.width(), actual.height()) != (golden.width(), golden.height()) {
        return Err(RenderError::InvalidTarget);
    }
    let mut diff = Vec::new();
    diff.try_reserve_exact(g.len())
        .map_err(|e| RenderError::Backend(format!("diff image: {e}")))?;
    let (mut differing, mut max_delta) = (0_u64, 0_u8);
    for (pa, pg) in a.chunks_exact(4).zip(g.chunks_exact(4)) {
        let delta = pa
            .iter()
            .zip(pg)
            .map(|(x, y)| x.abs_diff(*y))
            .max()
            .unwrap_or(0);
        max_delta = max_delta.max(delta);
        if delta > tolerance.per_channel {
            differing += 1;
            diff.extend([128 + delta / 2, 0, 0, 255]);
        } else {
            let luma = (u32::from(pg[0]) * 3 + u32::from(pg[1]) * 6 + u32::from(pg[2])) / 30;
            let v = 170 + luma as u8 / 3;
            diff.extend([v, v, v, 255]);
        }
    }
    Ok(GoldenReport {
        width: golden.width(),
        height: golden.height(),
        differing,
        max_delta,
        tolerance,
        diff,
    })
}

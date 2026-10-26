// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The spike's built-in resources: two images and a synthetic outline font, so scenes need no
//! files and both backends resolve handles identically.

use syrp_gfx_api::{FillRule, ImageHandle, Path};
use syrp_testkit::NoiseRng;

use crate::scene::{PHOTO, SPRITE, circle, polygon};

/// A premultiplied RGBA8 image, rows top first.
#[derive(Clone, Debug)]
pub(crate) struct RgbaImage {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) data: Vec<u8>,
}

/// Images by handle; built once per renderer.
#[derive(Clone, Debug)]
pub(crate) struct Assets {
    images: Vec<(ImageHandle, RgbaImage)>,
}

impl Assets {
    pub(crate) fn new() -> Self {
        Self {
            images: vec![(PHOTO, photo()), (SPRITE, sprite())],
        }
    }

    pub(crate) fn image(&self, handle: ImageHandle) -> Option<&RgbaImage> {
        self.images
            .iter()
            .find(|(h, _)| *h == handle)
            .map(|(_, i)| i)
    }
}

fn photo() -> RgbaImage {
    let (w, h) = (32_u32, 32_u32);
    let mut rng = NoiseRng::new(0x5379_7270);
    let mut data = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let n = rng.next_bipolar(24.0);
            let r = (x * 255 / (w - 1)) as f32 + n;
            let g = (y * 255 / (h - 1)) as f32 - n;
            let b = (((x + y) % 8) * 32) as f32;
            data.extend([r, g, b].map(|c| c.clamp(0.0, 255.0) as u8));
            data.push(255);
        }
    }
    RgbaImage {
        width: w,
        height: h,
        data,
    }
}

fn sprite() -> RgbaImage {
    let n = 24_u32;
    let mut data = Vec::with_capacity((n * n * 4) as usize);
    let c = (n as f32 - 1.0) / 2.0;
    for y in 0..n {
        for x in 0..n {
            let d = ((x as f32 - c).powi(2) + (y as f32 - c).powi(2)).sqrt() / c;
            let a = ((1.0 - d) * 2.0).clamp(0.0, 1.0);
            let premul = |v: f32| (v * a).round() as u8;
            data.extend([premul(250.0), premul(120.0), premul(30.0), premul(255.0)]);
        }
    }
    RgbaImage {
        width: n,
        height: n,
        data,
    }
}

/// The outline of glyph `id` in the synthetic font, in user units: the em square is scaled by
/// `size` and its origin (left end of the baseline) placed at `(ox, oy)`, y down. Five shapes
/// repeat by `id % 5`: `o` (a ring), `l` (a stem), `v` (a chevron), `a` (a ring with a stem,
/// overlapping under non-zero) and `x` (two crossing bars).
pub(crate) fn glyph_outline(id: u32, ox: f32, oy: f32, size: f32, out: &mut Path) {
    out.clear();
    let pt = |x: f32, y: f32| (ox + x * size, oy - y * size);
    let poly = |out: &mut Path, pts: &[(f32, f32)]| {
        let pts: Vec<_> = pts.iter().map(|&(x, y)| pt(x, y)).collect();
        polygon(out, &pts);
    };
    let ring = |out: &mut Path| {
        let (cx, cy) = pt(0.25, 0.25);
        circle(out, cx, cy, 0.25 * size, false);
        circle(out, cx, cy, 0.16 * size, true);
    };
    match id % 5 {
        0 => ring(out),
        1 => poly(out, &[(0.18, 0.0), (0.18, 0.72), (0.3, 0.72), (0.3, 0.0)]),
        2 => poly(
            out,
            &[
                (0.0, 0.5),
                (0.1, 0.5),
                (0.25, 0.1),
                (0.4, 0.5),
                (0.5, 0.5),
                (0.3, 0.0),
                (0.2, 0.0),
            ],
        ),
        3 => {
            ring(out);
            poly(out, &[(0.4, 0.0), (0.4, 0.5), (0.5, 0.5), (0.5, 0.0)]);
        }
        _ => {
            poly(out, &[(0.0, 0.0), (0.4, 0.5), (0.5, 0.5), (0.1, 0.0)]);
            poly(out, &[(0.4, 0.0), (0.0, 0.5), (0.1, 0.5), (0.5, 0.0)]);
        }
    }
    out.set_fill_rule(FillRule::NonZero);
}

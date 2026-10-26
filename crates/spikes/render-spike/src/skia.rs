// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The `tiny-skia` backend. tiny-skia has no layer or clip stack of its own: layers are
//! offscreen pixmaps composited on pop, and clips are coverage masks intersected on push.

use syrp_geom::{AffineTransform, Rect};
use syrp_gfx_api::{
    BlendMode, BrushKind, BrushRef, Cap, DisplayList, FillRule, ImageHandle, Join, PathEl,
    PathView, RenderError, RenderTarget, Renderer, ResamplingQuality, StrokeRef,
};
use tiny_skia as ts;

use crate::assets::Assets;
use crate::replay::{Shape, Sink, paints_nothing, replay};

/// Renders a [`DisplayList`] with `tiny-skia` (anti-aliased scanline rasteriser).
/// ```
/// use syrp_gfx_api::{Colour, DisplayList, RenderTarget, Renderer};
/// let mut list = DisplayList::new();
/// list.fill_rect(syrp_geom::Rect::new(0, 0, 4, 4), Colour::WHITE);
/// let mut target = RenderTarget::with_pixels(4, 4, 1.0)?;
/// syrp_render_spike::TinySkiaRenderer::new().render(&list, &mut target)?;
/// assert_eq!(&target.pixels().unwrap()[..4], &[255; 4]);
/// # Ok::<(), syrp_gfx_api::RenderError>(())
/// ```
#[derive(Debug)]
pub struct TinySkiaRenderer {
    assets: Assets,
    /// The built-in images, converted once so draws borrow them instead of copying.
    images: Vec<(ImageHandle, ts::Pixmap)>,
}

impl TinySkiaRenderer {
    /// A renderer holding the spike's built-in images and outline font.
    /// ```
    /// let _r = syrp_render_spike::TinySkiaRenderer::new();
    /// ```
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let assets = Assets::new();
        let images = assets
            .images()
            .filter_map(|(handle, img)| {
                let size = ts::IntSize::from_wh(img.width, img.height)?;
                Some((handle, ts::Pixmap::from_vec(img.data.clone(), size)?))
            })
            .collect();
        Self { assets, images }
    }
}

impl Renderer for TinySkiaRenderer {
    fn render(&mut self, list: &DisplayList, target: &mut RenderTarget) -> Result<(), RenderError> {
        let (w, h, scale) = (target.width(), target.height(), target.scale());
        let pixels = target.pixels_mut().ok_or(RenderError::InvalidTarget)?;
        let mut sink = SkiaSink {
            base: ts::PixmapMut::from_bytes(pixels, w, h).ok_or(RenderError::InvalidTarget)?,
            layers: Vec::new(),
            clips: Vec::new(),
            images: &self.images,
        };
        replay(list, scale, &self.assets, &mut sink)
    }
}

struct SkiaSink<'a> {
    base: ts::PixmapMut<'a>,
    layers: Vec<(ts::Pixmap, f32, BlendMode)>,
    clips: Vec<ts::Mask>,
    images: &'a [(ImageHandle, ts::Pixmap)],
}

fn transform(t: &AffineTransform) -> ts::Transform {
    let f = |v: f64| v as f32;
    ts::Transform::from_row(f(t.m00), f(t.m10), f(t.m01), f(t.m11), f(t.m02), f(t.m12))
}

fn colour(c: syrp_gfx_api::Colour) -> ts::Color {
    let [r, g, b, a] = c.to_rgba();
    ts::Color::from_rgba8(r, g, b, a)
}

fn rule(r: FillRule) -> ts::FillRule {
    match r {
        FillRule::EvenOdd => ts::FillRule::EvenOdd,
        _ => ts::FillRule::Winding,
    }
}

fn tpoint(p: syrp_geom::Point<f32>) -> ts::Point {
    ts::Point::from_xy(p.x, p.y)
}

fn path(view: PathView<'_>) -> Option<ts::Path> {
    let mut b = ts::PathBuilder::new();
    for el in view.elements() {
        match el {
            PathEl::MoveTo(p) => b.move_to(p.x, p.y),
            PathEl::LineTo(p) => b.line_to(p.x, p.y),
            PathEl::QuadTo(c, p) => b.quad_to(c.x, c.y, p.x, p.y),
            PathEl::CubicTo(c1, c2, p) => b.cubic_to(c1.x, c1.y, c2.x, c2.y, p.x, p.y),
            _ => b.close(),
        }
    }
    b.finish()
}

/// tiny-skia needs an even number of dash lengths; an odd list is repeated once, so the
/// lengths keep alternating on, off, on... (the SVG rule).
fn dash_list(dashes: &[f32]) -> Vec<f32> {
    let times = if dashes.len() % 2 == 1 { 2 } else { 1 };
    dashes.repeat(times)
}

fn shape_path(shape: Shape<'_>) -> Option<(ts::Path, ts::FillRule)> {
    match shape {
        Shape::Rect(r) => {
            let r = ts::Rect::from_xywh(r.x(), r.y(), r.width(), r.height())?;
            Some((ts::PathBuilder::from_rect(r), ts::FillRule::Winding))
        }
        Shape::Path(p) => Some((path(p)?, rule(p.fill_rule()))),
    }
}

fn shader(brush: BrushRef<'_>) -> Option<ts::Shader<'static>> {
    let stops: Vec<_> = brush
        .stops()
        .iter()
        .map(|s| ts::GradientStop::new(s.offset(), colour(s.colour())))
        .collect();
    let (pad, id) = (ts::SpreadMode::Pad, ts::Transform::identity());
    match brush.kind() {
        BrushKind::Solid(c) => Some(ts::Shader::SolidColor(colour(c))),
        BrushKind::Linear { start, end, .. } => {
            ts::LinearGradient::new(tpoint(start), tpoint(end), stops, pad, id)
        }
        BrushKind::Radial { centre, radius, .. } => {
            let c = tpoint(centre);
            ts::RadialGradient::new(c, 0.0, c, radius, stops, pad, id)
        }
        BrushKind::TwoPointConical {
            start,
            start_radius,
            end,
            end_radius,
            ..
        } => {
            let (s, e) = (tpoint(start), tpoint(end));
            ts::RadialGradient::new(s, start_radius, e, end_radius, stops, pad, id)
        }
        _ => None,
    }
}

fn blend(b: BlendMode) -> ts::BlendMode {
    use ts::BlendMode as T;
    match b {
        BlendMode::Multiply => T::Multiply,
        BlendMode::Screen => T::Screen,
        BlendMode::Overlay => T::Overlay,
        BlendMode::Darken => T::Darken,
        BlendMode::Lighten => T::Lighten,
        BlendMode::ColourDodge => T::ColorDodge,
        BlendMode::ColourBurn => T::ColorBurn,
        BlendMode::HardLight => T::HardLight,
        BlendMode::SoftLight => T::SoftLight,
        BlendMode::Difference => T::Difference,
        BlendMode::Exclusion => T::Exclusion,
        BlendMode::Hue => T::Hue,
        BlendMode::Saturation => T::Saturation,
        BlendMode::Colour => T::Color,
        BlendMode::Luminosity => T::Luminosity,
        _ => T::SourceOver,
    }
}

impl SkiaSink<'_> {
    /// Runs `f` on the innermost layer (or the target) with the current clip mask.
    fn draw(&mut self, f: impl FnOnce(&mut ts::PixmapMut<'_>, Option<&ts::Mask>)) {
        let mask = self.clips.last();
        match self.layers.last_mut() {
            Some((layer, ..)) => f(&mut layer.as_mut(), mask),
            None => f(&mut self.base, mask),
        }
    }
}

impl Sink for SkiaSink<'_> {
    fn fill(&mut self, shape: Shape<'_>, brush: BrushRef<'_>, xf: &AffineTransform) {
        let (Some(shader), Some((p, rule))) = (shader(brush), shape_path(shape)) else {
            return;
        };
        if paints_nothing(brush) {
            return;
        }
        let paint = ts::Paint {
            shader,
            anti_alias: true,
            ..ts::Paint::default()
        };
        self.draw(|dst, mask| dst.fill_path(&p, &paint, rule, transform(xf), mask));
    }

    fn stroke(
        &mut self,
        path_view: PathView<'_>,
        s: StrokeRef<'_>,
        b: BrushRef<'_>,
        xf: &AffineTransform,
    ) {
        if paints_nothing(b) {
            return;
        }
        let (Some(shader), Some(p)) = (shader(b), path(path_view)) else {
            return;
        };
        let stroke = ts::Stroke {
            width: s.width(),
            miter_limit: s.miter_limit(),
            line_cap: match s.cap() {
                Cap::Round => ts::LineCap::Round,
                Cap::Square => ts::LineCap::Square,
                _ => ts::LineCap::Butt,
            },
            line_join: match s.join() {
                Join::Round => ts::LineJoin::Round,
                Join::Bevel => ts::LineJoin::Bevel,
                _ => ts::LineJoin::Miter,
            },
            dash: ts::StrokeDash::new(dash_list(s.dashes()), s.dash_offset()),
        };
        let paint = ts::Paint {
            shader,
            anti_alias: true,
            ..ts::Paint::default()
        };
        self.draw(|dst, mask| dst.stroke_path(&p, &paint, &stroke, transform(xf), mask));
    }

    fn image(
        &mut self,
        handle: ImageHandle,
        image_xf: &AffineTransform,
        area: Rect<f32>,
        quality: ResamplingQuality,
        xf: &AffineTransform,
    ) {
        let images = self.images;
        let Some((_, pixmap)) = images.iter().find(|(h, _)| *h == handle) else {
            return;
        };
        let Some(rect) = ts::Rect::from_xywh(area.x(), area.y(), area.width(), area.height())
        else {
            return;
        };
        let quality = match quality {
            ResamplingQuality::Low => ts::FilterQuality::Nearest,
            ResamplingQuality::High => ts::FilterQuality::Bicubic,
            _ => ts::FilterQuality::Bilinear,
        };
        let shader = ts::Pattern::new(
            pixmap.as_ref(),
            ts::SpreadMode::Pad,
            quality,
            1.0,
            transform(image_xf),
        );
        let paint = ts::Paint {
            shader,
            anti_alias: true,
            ..ts::Paint::default()
        };
        self.draw(|dst, mask| dst.fill_rect(rect, &paint, transform(xf), mask));
    }

    fn push_clip(&mut self, shape: Shape<'_>, xf: &AffineTransform) {
        let (w, h) = (self.base.width(), self.base.height());
        let mut mask = match self.clips.last() {
            Some(m) => m.clone(),
            None => {
                let mut m = ts::Mask::new(w, h).expect("target size was validated");
                m.data_mut().fill(255);
                m
            }
        };
        match shape_path(shape) {
            Some((p, rule)) => mask.intersect_path(&p, rule, true, transform(xf)),
            None => mask.clear(),
        }
        self.clips.push(mask);
    }

    fn pop_clip(&mut self) {
        self.clips.pop();
    }

    fn push_layer(&mut self, opacity: f32, mode: BlendMode) {
        let (w, h) = (self.base.width(), self.base.height());
        let layer = ts::Pixmap::new(w, h).expect("target size was validated");
        self.layers.push((layer, opacity.clamp(0.0, 1.0), mode));
    }

    fn pop_layer(&mut self) {
        let Some((layer, opacity, mode)) = self.layers.pop() else {
            return;
        };
        let paint = ts::PixmapPaint {
            opacity,
            blend_mode: blend(mode),
            quality: ts::FilterQuality::Nearest,
        };
        let id = ts::Transform::identity();
        match self.layers.last_mut() {
            Some((below, ..)) => below.draw_pixmap(0, 0, layer.as_ref(), &paint, id, None),
            None => self
                .base
                .draw_pixmap(0, 0, layer.as_ref(), &paint, id, None),
        }
    }
}

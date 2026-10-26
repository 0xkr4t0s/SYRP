// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The `vello_cpu` backend.

use std::sync::Arc;

use syrp_geom::{AffineTransform, Rect};
use syrp_gfx_api::{
    BlendMode, BrushKind, BrushRef, Cap, DisplayList, FillRule, Join, PathEl, PathView,
    RenderError, RenderTarget, Renderer, ResamplingQuality, StrokeRef,
};
use vello_cpu::kurbo::{self, Affine, BezPath, Point};
use vello_cpu::peniko::{self, ImageQuality, ImageSampler, Mix};
use vello_cpu::{
    Image, ImageSource, Level, PaintType, PixelMetadata, Pixmap, PixmapMut, RenderContext,
    RenderSettings, Resources, color::AlphaColor, color::Srgb,
};

use crate::assets::{Assets, RgbaImage};
use crate::replay::{Shape, Sink, paints_nothing, replay};

/// How `vello_cpu` picks its SIMD code path.
/// ```
/// use syrp_render_spike::VelloMode;
/// assert_ne!(VelloMode::Deterministic, VelloMode::NativeSimd);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum VelloMode {
    /// The architecture's baseline SIMD level (SSE2 on x86_64, NEON on aarch64), single
    /// threaded, u8 pipeline: the same code path on every CPU of one architecture.
    Deterministic,
    /// The best SIMD level the running CPU supports (AVX2 where present), single threaded.
    NativeSimd,
}

/// Renders a [`DisplayList`] with `vello_cpu` (sparse strips, single threaded).
/// ```
/// use syrp_gfx_api::{Colour, DisplayList, RenderTarget, Renderer};
/// use syrp_render_spike::{VelloCpuRenderer, VelloMode};
/// let mut list = DisplayList::new();
/// list.fill_rect(syrp_geom::Rect::new(0, 0, 4, 4), Colour::WHITE);
/// let mut target = RenderTarget::with_pixels(4, 4, 1.0)?;
/// VelloCpuRenderer::new(VelloMode::Deterministic).render(&list, &mut target)?;
/// assert_eq!(&target.pixels().unwrap()[..4], &[255; 4]);
/// # Ok::<(), syrp_gfx_api::RenderError>(())
/// ```
#[derive(Debug)]
pub struct VelloCpuRenderer {
    mode: VelloMode,
    assets: Assets,
    resources: Resources,
}

impl VelloCpuRenderer {
    /// A renderer in `mode`, holding the spike's built-in images and outline font.
    /// ```
    /// let _r = syrp_render_spike::VelloCpuRenderer::new(syrp_render_spike::VelloMode::NativeSimd);
    /// ```
    pub fn new(mode: VelloMode) -> Self {
        Self {
            mode,
            assets: Assets::new(),
            resources: Resources::new(),
        }
    }
}

impl Renderer for VelloCpuRenderer {
    fn render(&mut self, list: &DisplayList, target: &mut RenderTarget) -> Result<(), RenderError> {
        let w = u16::try_from(target.width()).map_err(|_| RenderError::InvalidTarget)?;
        let h = u16::try_from(target.height()).map_err(|_| RenderError::InvalidTarget)?;
        let level = match self.mode {
            VelloMode::Deterministic => Level::baseline(),
            VelloMode::NativeSimd => Level::new(),
        };
        let settings = RenderSettings {
            level,
            num_threads: 0,
        };
        let mut sink = VelloSink {
            ctx: RenderContext::new_with(w, h, settings),
            path: BezPath::new(),
        };
        replay(list, target.scale(), &self.assets, &mut sink)?;
        let pixels = target.pixels_mut().ok_or(RenderError::InvalidTarget)?;
        let out = PixmapMut::new(w, h, pixels).ok_or(RenderError::InvalidTarget)?;
        sink.ctx.flush();
        sink.ctx.render(out, &mut self.resources);
        Ok(())
    }
}

struct VelloSink {
    ctx: RenderContext,
    path: BezPath,
}

fn affine(t: &AffineTransform) -> Affine {
    Affine::new([t.m00, t.m10, t.m01, t.m11, t.m02, t.m12])
}

fn point(p: syrp_geom::Point<f32>) -> Point {
    Point::new(f64::from(p.x), f64::from(p.y))
}

fn krect(r: Rect<f32>) -> kurbo::Rect {
    let f = f64::from;
    kurbo::Rect::new(f(r.x()), f(r.y()), f(r.right()), f(r.bottom()))
}

fn colour(c: syrp_gfx_api::Colour) -> AlphaColor<Srgb> {
    let [r, g, b, a] = c.to_rgba();
    AlphaColor::from_rgba8(r, g, b, a)
}

fn fill_rule(rule: FillRule) -> peniko::Fill {
    match rule {
        FillRule::EvenOdd => peniko::Fill::EvenOdd,
        _ => peniko::Fill::NonZero,
    }
}

fn paint(brush: BrushRef<'_>) -> PaintType {
    let stops: Vec<(f32, AlphaColor<Srgb>)> = brush
        .stops()
        .iter()
        .map(|s| (s.offset(), colour(s.colour())))
        .collect();
    if let [only] = stops.as_slice() {
        return PaintType::Solid(only.1);
    }
    let gradient = match brush.kind() {
        BrushKind::Solid(c) => return PaintType::Solid(colour(c)),
        BrushKind::Linear { start, end, .. } => {
            peniko::Gradient::new_linear(point(start), point(end))
        }
        BrushKind::Radial { centre, radius, .. } => {
            peniko::Gradient::new_radial(point(centre), radius)
        }
        BrushKind::TwoPointConical {
            start,
            start_radius,
            end,
            end_radius,
            ..
        } => peniko::Gradient::new_two_point_radial(
            point(start),
            start_radius,
            point(end),
            end_radius,
        ),
        _ => return PaintType::Solid(AlphaColor::TRANSPARENT),
    };
    PaintType::Gradient(gradient.with_stops(stops.as_slice()))
}

fn mix(blend: BlendMode) -> Mix {
    match blend {
        BlendMode::Multiply => Mix::Multiply,
        BlendMode::Screen => Mix::Screen,
        BlendMode::Overlay => Mix::Overlay,
        BlendMode::Darken => Mix::Darken,
        BlendMode::Lighten => Mix::Lighten,
        BlendMode::ColourDodge => Mix::ColorDodge,
        BlendMode::ColourBurn => Mix::ColorBurn,
        BlendMode::HardLight => Mix::HardLight,
        BlendMode::SoftLight => Mix::SoftLight,
        BlendMode::Difference => Mix::Difference,
        BlendMode::Exclusion => Mix::Exclusion,
        BlendMode::Hue => Mix::Hue,
        BlendMode::Saturation => Mix::Saturation,
        BlendMode::Colour => Mix::Color,
        BlendMode::Luminosity => Mix::Luminosity,
        _ => Mix::Normal,
    }
}

impl VelloSink {
    /// Loads `path` into the scratch path and sets its fill rule on the context.
    fn load(&mut self, path: PathView<'_>) {
        self.path.truncate(0);
        for el in path.elements() {
            self.path.push(match el {
                PathEl::MoveTo(p) => kurbo::PathEl::MoveTo(point(p)),
                PathEl::LineTo(p) => kurbo::PathEl::LineTo(point(p)),
                PathEl::QuadTo(c, p) => kurbo::PathEl::QuadTo(point(c), point(p)),
                PathEl::CubicTo(a, b, p) => kurbo::PathEl::CurveTo(point(a), point(b), point(p)),
                _ => kurbo::PathEl::ClosePath,
            });
        }
        self.ctx.set_fill_rule(fill_rule(path.fill_rule()));
    }
}

impl Sink for VelloSink {
    fn fill(&mut self, shape: Shape<'_>, brush: BrushRef<'_>, xf: &AffineTransform) {
        if paints_nothing(brush) {
            return;
        }
        self.ctx.set_transform(affine(xf));
        self.ctx.reset_paint_transform();
        self.ctx.set_paint(paint(brush));
        match shape {
            Shape::Rect(r) => self.ctx.fill_rect(&krect(r)),
            Shape::Path(p) => {
                self.load(p);
                self.ctx.fill_path(&self.path);
            }
        }
    }

    fn stroke(
        &mut self,
        path: PathView<'_>,
        s: StrokeRef<'_>,
        b: BrushRef<'_>,
        xf: &AffineTransform,
    ) {
        if paints_nothing(b) {
            return;
        }
        let join = match s.join() {
            Join::Round => kurbo::Join::Round,
            Join::Bevel => kurbo::Join::Bevel,
            _ => kurbo::Join::Miter,
        };
        let cap = match s.cap() {
            Cap::Round => kurbo::Cap::Round,
            Cap::Square => kurbo::Cap::Square,
            _ => kurbo::Cap::Butt,
        };
        let stroke = kurbo::Stroke::new(f64::from(s.width()))
            .with_join(join)
            .with_caps(cap)
            .with_miter_limit(f64::from(s.miter_limit()))
            .with_dashes(
                f64::from(s.dash_offset()),
                s.dashes().iter().map(|&d| f64::from(d)),
            );
        self.ctx.set_transform(affine(xf));
        self.ctx.reset_paint_transform();
        self.ctx.set_stroke(stroke);
        self.ctx.set_paint(paint(b));
        self.load(path);
        self.ctx.stroke_path(&self.path);
    }

    fn image(
        &mut self,
        image: &RgbaImage,
        image_xf: &AffineTransform,
        area: Rect<f32>,
        quality: ResamplingQuality,
        xf: &AffineTransform,
    ) {
        let (Ok(w), Ok(h)) = (u16::try_from(image.width), u16::try_from(image.height)) else {
            return;
        };
        let meta = PixelMetadata::new(peniko::ImageAlphaType::AlphaPremultiplied, true);
        let pixmap = Pixmap::from_parts(image.data.clone(), w, h, meta);
        let quality = match quality {
            ResamplingQuality::Low => ImageQuality::Low,
            ResamplingQuality::High => ImageQuality::High,
            _ => ImageQuality::Medium,
        };
        let brush = Image {
            image: ImageSource::Pixmap(Arc::new(pixmap)),
            sampler: ImageSampler::new().with_quality(quality),
        };
        self.ctx.set_transform(affine(xf));
        self.ctx.set_paint_transform(affine(image_xf));
        self.ctx.set_paint(brush);
        self.ctx.fill_rect(&krect(area));
        self.ctx.reset_paint_transform();
    }

    fn push_clip(&mut self, shape: Shape<'_>, xf: &AffineTransform) {
        self.ctx.set_transform(affine(xf));
        match shape {
            Shape::Rect(r) => self.ctx.push_clip_rect(&krect(r)),
            Shape::Path(p) => {
                self.load(p);
                self.ctx.push_clip_path(&self.path);
            }
        }
    }

    fn pop_clip(&mut self) {
        self.ctx.pop_clip();
    }

    fn push_layer(&mut self, opacity: f32, blend: BlendMode) {
        let blend = peniko::BlendMode::from(mix(blend));
        self.ctx
            .push_layer(None, Some(blend), Some(opacity), None, None);
    }

    fn pop_layer(&mut self) {
        self.ctx.pop_layer();
    }
}

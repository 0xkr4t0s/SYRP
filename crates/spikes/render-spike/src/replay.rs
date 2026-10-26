// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Walks a `DisplayList` once and lowers it to a handful of backend calls, so both renderers
//! see exactly the same geometry: glyphs become outlines and images become a placed rectangle
//! with an image-to-user transform before any backend code runs.

use syrp_geom::{AffineTransform, Rect};
use syrp_gfx_api::{
    BlendMode, BrushRef, DisplayList, ImageHandle, Op, Path, PathView, RenderError,
    ResamplingQuality, StrokeRef,
};

use crate::assets::{Assets, glyph_outline};
use crate::scene::OUTLINES;

/// A filled or clipping shape.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Shape<'a> {
    Rect(Rect<f32>),
    Path(PathView<'a>),
}

/// What a backend implements. `xf` maps user units to device pixels (scale included).
pub(crate) trait Sink {
    fn fill(&mut self, shape: Shape<'_>, brush: BrushRef<'_>, xf: &AffineTransform);
    fn stroke(
        &mut self,
        path: PathView<'_>,
        stroke: StrokeRef<'_>,
        brush: BrushRef<'_>,
        xf: &AffineTransform,
    );
    /// Paints image `handle` mapped to user space by `image_xf`, limited to `area` (user units).
    fn image(
        &mut self,
        handle: ImageHandle,
        image_xf: &AffineTransform,
        area: Rect<f32>,
        quality: ResamplingQuality,
        xf: &AffineTransform,
    );
    fn push_clip(&mut self, shape: Shape<'_>, xf: &AffineTransform);
    fn pop_clip(&mut self);
    fn push_layer(&mut self, opacity: f32, blend: BlendMode);
    fn pop_layer(&mut self);
}

enum Scope {
    Transform(AffineTransform),
    Clip,
    Layer,
}

/// Replays `list` into `sink` at `scale`; scopes left open are closed at the end.
pub(crate) fn replay(
    list: &DisplayList,
    scale: f64,
    assets: &Assets,
    sink: &mut impl Sink,
) -> Result<(), RenderError> {
    let mut xf = AffineTransform::scale(scale, scale);
    let mut scopes = Vec::new();
    let mut glyph = Path::new();
    for op in list.ops() {
        match op {
            Op::FillRect { rect, brush, .. } => sink.fill(Shape::Rect(rect), brush, &xf),
            Op::FillPath { path, brush, .. } => sink.fill(Shape::Path(path), brush, &xf),
            Op::StrokePath {
                path,
                stroke,
                brush,
                ..
            } => sink.stroke(path, stroke, brush, &xf),
            Op::DrawImage {
                image,
                dest,
                placement,
                quality,
                ..
            } => {
                let img = assets
                    .image(image)
                    .ok_or(RenderError::UnknownImage(image))?;
                let src = Rect::new(0.0, 0.0, img.width as f32, img.height as f32);
                let image_xf = placement.transform_to_fit(src, dest);
                let area = placement.apply_to(src, dest).intersection(&dest);
                sink.image(image, &image_xf, area, quality, &xf);
            }
            Op::DrawGlyphs {
                font,
                size,
                glyphs,
                brush,
                ..
            } => {
                if font != OUTLINES {
                    return Err(RenderError::UnknownFont(font));
                }
                for g in glyphs {
                    let o = g.origin();
                    glyph_outline(g.id(), o.x, o.y, size, &mut glyph);
                    sink.fill(Shape::Path(glyph.as_view()), brush, &xf);
                }
            }
            Op::PushClipRect { rect, .. } => {
                sink.push_clip(Shape::Rect(rect), &xf);
                scopes.push(Scope::Clip);
            }
            Op::PushClipPath { path, .. } => {
                sink.push_clip(Shape::Path(path), &xf);
                scopes.push(Scope::Clip);
            }
            Op::PushTransform { transform, .. } => {
                scopes.push(Scope::Transform(xf));
                xf = transform.followed_by(&xf);
            }
            Op::PushLayer { opacity, blend, .. } => {
                sink.push_layer(opacity, blend);
                scopes.push(Scope::Layer);
            }
            Op::Pop => close(scopes.pop(), &mut xf, sink),
            _ => {
                return Err(RenderError::Backend(
                    "op outside the spike's scope (clip masks)".into(),
                ));
            }
        }
    }
    while let Some(s) = scopes.pop() {
        close(Some(s), &mut xf, sink);
    }
    Ok(())
}

fn close(scope: Option<Scope>, xf: &mut AffineTransform, sink: &mut impl Sink) {
    match scope {
        Some(Scope::Transform(prev)) => *xf = prev,
        Some(Scope::Clip) => sink.pop_clip(),
        Some(Scope::Layer) => sink.pop_layer(),
        None => {}
    }
}

/// Whether `brush` is a gradient with no stops, which paints nothing.
pub(crate) fn paints_nothing(brush: BrushRef<'_>) -> bool {
    !matches!(brush.kind(), syrp_gfx_api::BrushKind::Solid(_)) && brush.stops().is_empty()
}

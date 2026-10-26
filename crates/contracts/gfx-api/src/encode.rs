// SPDX-License-Identifier: MIT OR Apache-2.0
//! The canonical encoding of a display list: the content hash and (feature `serde`) JSON.
//!
//! Both walk the public [`Op`] views, never the storage, so capacity, bytes left behind by
//! `clear()` and arena layout cannot affect either.

use syrp_geom::{AffineTransform, Point, Rect};

use crate::brush::{BrushKind, BrushRef};
use crate::list::{DisplayList, Op};
use crate::path::PathView;

/// 64-bit FNV-1a: fixed, seedless and byte-order independent (all values are fed little-endian).
struct Fnv(u64);

impl Fnv {
    fn bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    fn u8(&mut self, v: u8) {
        self.bytes(&[v]);
    }
    fn u32(&mut self, v: u32) {
        self.bytes(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.bytes(&v.to_le_bytes());
    }
    fn len(&mut self, n: usize) {
        self.u64(n as u64);
    }
    /// Floats by bit pattern: `-0.0` differs from `0.0`, and a NaN hashes by its payload.
    fn f32(&mut self, v: f32) {
        self.u32(v.to_bits());
    }
    fn point(&mut self, p: Point<f32>) {
        self.f32(p.x);
        self.f32(p.y);
    }
    fn rect(&mut self, r: Rect<f32>) {
        [r.x(), r.y(), r.width(), r.height()]
            .into_iter()
            .for_each(|v| self.f32(v));
    }
    fn transform(&mut self, t: AffineTransform) {
        [t.m00, t.m01, t.m02, t.m10, t.m11, t.m12]
            .into_iter()
            .for_each(|v| self.u64(v.to_bits()));
    }
    fn path(&mut self, p: PathView<'_>) {
        self.u8(p.fill_rule() as u8);
        self.len(p.verbs().len());
        p.verbs().iter().for_each(|&v| self.u8(v as u8));
        self.len(p.points().len());
        p.points().iter().for_each(|&q| self.point(q));
    }
    fn brush(&mut self, b: BrushRef<'_>) {
        match b.kind() {
            BrushKind::Solid(c) => {
                self.u8(0);
                self.u32(c.argb());
            }
            BrushKind::Linear { start, end } => {
                self.u8(1);
                [start, end].into_iter().for_each(|p| self.point(p));
            }
            BrushKind::Radial { centre, radius } => {
                self.u8(2);
                self.point(centre);
                self.f32(radius);
            }
            BrushKind::TwoPointConical {
                start,
                start_radius,
                end,
                end_radius,
            } => {
                self.u8(3);
                self.point(start);
                self.f32(start_radius);
                self.point(end);
                self.f32(end_radius);
            }
        }
        self.len(b.stops().len());
        for s in b.stops() {
            self.f32(s.offset());
            self.u32(s.colour().argb());
        }
    }
    fn op(&mut self, op: Op<'_>) {
        match op {
            Op::FillRect { rect, brush } => {
                self.u8(0);
                self.rect(rect);
                self.brush(brush);
            }
            Op::FillPath { path, brush } => {
                self.u8(1);
                self.path(path);
                self.brush(brush);
            }
            Op::StrokePath {
                path,
                stroke,
                brush,
            } => {
                self.u8(2);
                self.path(path);
                [stroke.width(), stroke.miter_limit(), stroke.dash_offset()]
                    .into_iter()
                    .for_each(|v| self.f32(v));
                self.u8(stroke.join() as u8);
                self.u8(stroke.cap() as u8);
                self.len(stroke.dashes().len());
                stroke.dashes().iter().for_each(|&d| self.f32(d));
                self.brush(brush);
            }
            Op::DrawImage {
                image,
                dest,
                placement,
                quality,
            } => {
                self.u8(3);
                self.u64(image.raw());
                self.rect(dest);
                self.u32(placement.bits());
                self.u8(quality as u8);
            }
            Op::DrawGlyphs {
                font,
                size,
                glyphs,
                brush,
            } => {
                self.u8(4);
                self.u64(font.raw());
                self.f32(size);
                self.len(glyphs.len());
                for g in glyphs {
                    self.u32(g.id());
                    self.point(g.origin());
                }
                self.brush(brush);
            }
            Op::PushClipRect { rect } => {
                self.u8(5);
                self.rect(rect);
            }
            Op::PushClipPath { path } => {
                self.u8(6);
                self.path(path);
            }
            Op::PushClipMask { image, transform } => {
                self.u8(7);
                self.u64(image.raw());
                self.transform(transform);
            }
            Op::PushTransform { transform } => {
                self.u8(8);
                self.transform(transform);
            }
            Op::PushLayer { opacity, blend } => {
                self.u8(9);
                self.f32(opacity);
                self.u8(blend as u8);
            }
            Op::Pop => self.u8(10),
        }
    }
}

impl DisplayList {
    /// A 64-bit hash of the ops' canonical encoding, for skipping unchanged cached layers and
    /// keying goldens.
    ///
    /// It walks the [`Op`] views (op kind, then every payload field and the arena contents it
    /// references, slices length-prefixed), so spare capacity, leftovers from `clear()` and
    /// arena layout never count. Floats hash by bit pattern (`-0.0` differs from `0.0`; a NaN
    /// hashes by its payload). The algorithm (64-bit FNV-1a over little-endian bytes) is fixed
    /// and seedless: the same ops give the same value on every run, build and platform.
    /// ```
    /// use syrp_gfx_api::{Colour, DisplayList};
    /// let mut a = DisplayList::new();
    /// a.fill_rect(syrp_geom::Rect::new(0, 0, 4, 4), Colour::BLACK);
    /// let b = a.clone();
    /// assert_eq!(a.content_hash(), b.content_hash());
    /// assert_ne!(a.content_hash(), DisplayList::new().content_hash());
    /// ```
    #[must_use]
    pub fn content_hash(&self) -> u64 {
        let mut h = Fnv(0xcbf2_9ce4_8422_2325);
        self.ops().for_each(|op| h.op(op));
        h.0
    }
}

#[cfg(feature = "serde")]
mod json {
    use serde::ser::{Serialize, SerializeSeq, Serializer};

    use super::*;
    use crate::{BlendMode, Cap, FillRule, Join, PathVerb, ResamplingQuality};

    fn xy(p: Point<f32>) -> [f32; 2] {
        [p.x, p.y]
    }
    fn xywh(r: Rect<f32>) -> [f32; 4] {
        [r.x(), r.y(), r.width(), r.height()]
    }
    fn matrix(t: AffineTransform) -> [f64; 6] {
        [t.m00, t.m01, t.m02, t.m10, t.m11, t.m12]
    }

    #[derive(serde::Serialize)]
    struct PathJson<'a> {
        fill_rule: FillRule,
        verbs: &'a [PathVerb],
        points: Vec<[f32; 2]>,
    }

    impl<'a> From<PathView<'a>> for PathJson<'a> {
        fn from(p: PathView<'a>) -> Self {
            let points = p.points().iter().map(|&q| xy(q)).collect();
            PathJson {
                fill_rule: p.fill_rule(),
                verbs: p.verbs(),
                points,
            }
        }
    }

    #[derive(serde::Serialize)]
    #[serde(tag = "kind", rename_all = "snake_case")]
    enum BrushJson {
        Solid {
            argb: u32,
        },
        Linear {
            start: [f32; 2],
            end: [f32; 2],
            stops: Vec<(f32, u32)>,
        },
        Radial {
            centre: [f32; 2],
            radius: f32,
            stops: Vec<(f32, u32)>,
        },
        TwoPointConical {
            start: [f32; 2],
            start_radius: f32,
            end: [f32; 2],
            end_radius: f32,
            stops: Vec<(f32, u32)>,
        },
    }

    impl From<BrushRef<'_>> for BrushJson {
        fn from(b: BrushRef<'_>) -> Self {
            let stops = b
                .stops()
                .iter()
                .map(|s| (s.offset(), s.colour().argb()))
                .collect();
            match b.kind() {
                BrushKind::Solid(c) => BrushJson::Solid { argb: c.argb() },
                BrushKind::Linear { start, end } => BrushJson::Linear {
                    start: xy(start),
                    end: xy(end),
                    stops,
                },
                BrushKind::Radial { centre, radius } => BrushJson::Radial {
                    centre: xy(centre),
                    radius,
                    stops,
                },
                BrushKind::TwoPointConical {
                    start,
                    start_radius,
                    end,
                    end_radius,
                } => {
                    let (start, end) = (xy(start), xy(end));
                    BrushJson::TwoPointConical {
                        start,
                        start_radius,
                        end,
                        end_radius,
                        stops,
                    }
                }
            }
        }
    }

    #[derive(serde::Serialize)]
    #[serde(tag = "op", rename_all = "snake_case")]
    enum OpJson<'a> {
        FillRect {
            rect: [f32; 4],
            brush: BrushJson,
        },
        FillPath {
            path: PathJson<'a>,
            brush: BrushJson,
        },
        #[rustfmt::skip]
        StrokePath { path: PathJson<'a>, width: f32, join: Join, cap: Cap, miter_limit: f32, dashes: &'a [f32], dash_offset: f32, brush: BrushJson },
        DrawImage {
            image: u64,
            dest: [f32; 4],
            placement: u32,
            quality: ResamplingQuality,
        },
        DrawGlyphs {
            font: u64,
            size: f32,
            glyphs: Vec<(u32, [f32; 2])>,
            brush: BrushJson,
        },
        PushClipRect {
            rect: [f32; 4],
        },
        PushClipPath {
            path: PathJson<'a>,
        },
        PushClipMask {
            image: u64,
            transform: [f64; 6],
        },
        PushTransform {
            transform: [f64; 6],
        },
        PushLayer {
            opacity: f32,
            blend: BlendMode,
        },
        Pop,
    }

    impl<'a> From<Op<'a>> for OpJson<'a> {
        fn from(op: Op<'a>) -> Self {
            match op {
                Op::FillRect { rect, brush } => OpJson::FillRect {
                    rect: xywh(rect),
                    brush: brush.into(),
                },
                Op::FillPath { path, brush } => OpJson::FillPath {
                    path: path.into(),
                    brush: brush.into(),
                },
                Op::StrokePath {
                    path,
                    stroke: s,
                    brush,
                } => OpJson::StrokePath {
                    path: path.into(),
                    width: s.width(),
                    join: s.join(),
                    cap: s.cap(),
                    miter_limit: s.miter_limit(),
                    dashes: s.dashes(),
                    dash_offset: s.dash_offset(),
                    brush: brush.into(),
                },
                Op::DrawImage {
                    image,
                    dest,
                    placement,
                    quality,
                } => {
                    let (image, dest, placement) = (image.raw(), xywh(dest), placement.bits());
                    OpJson::DrawImage {
                        image,
                        dest,
                        placement,
                        quality,
                    }
                }
                Op::DrawGlyphs {
                    font,
                    size,
                    glyphs,
                    brush,
                } => {
                    let glyphs = glyphs.iter().map(|g| (g.id(), xy(g.origin()))).collect();
                    OpJson::DrawGlyphs {
                        font: font.raw(),
                        size,
                        glyphs,
                        brush: brush.into(),
                    }
                }
                Op::PushClipRect { rect } => OpJson::PushClipRect { rect: xywh(rect) },
                Op::PushClipPath { path } => OpJson::PushClipPath { path: path.into() },
                Op::PushClipMask { image, transform } => OpJson::PushClipMask {
                    image: image.raw(),
                    transform: matrix(transform),
                },
                Op::PushTransform { transform } => OpJson::PushTransform {
                    transform: matrix(transform),
                },
                Op::PushLayer { opacity, blend } => OpJson::PushLayer { opacity, blend },
                Op::Pop => OpJson::Pop,
            }
        }
    }

    /// Serialises as an array of ops, each an object tagged by `"op"` (for example
    /// `{"op":"fill_rect","rect":[x,y,w,h],"brush":{"kind":"solid","argb":4278190080}}`).
    /// Colours are packed ARGB numbers, transforms `[m00, m01, m02, m10, m11, m12]`, and image
    /// placements their `RectanglePlacement::bits`. NaN floats become `null` in JSON.
    impl Serialize for DisplayList {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            let mut seq = s.serialize_seq(Some(self.len()))?;
            for op in self.ops() {
                seq.serialize_element(&OpJson::from(op))?;
            }
            seq.end()
        }
    }
}

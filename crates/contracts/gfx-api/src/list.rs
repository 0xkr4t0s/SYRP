// SPDX-License-Identifier: MIT OR Apache-2.0
//! The display list: an op-tag stream, per-kind payload arrays and shared arenas.

use syrp_geom::{AffineTransform, Coord, Point, Rect, RectanglePlacement};

use crate::brush::{BrushKind, BrushRef, GradientStop};
use crate::path::{FillRule, PathVerb, PathView, StrokeParams, StrokeRef, non_neg};
use crate::{FontHandle, ImageHandle};

/// One positioned glyph: a glyph id in its font and the baseline origin in user units.
/// ```
/// use syrp_geom::Point;
/// let g = syrp_gfx_api::Glyph::new(42, Point::new(10.0, 20.0));
/// assert_eq!((g.id(), g.origin()), (42, Point::new(10.0, 20.0)));
/// ```
#[doc(alias = "PositionedGlyph")]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    id: u32,
    origin: Point<f32>,
}

impl Glyph {
    /// A glyph with id `id` (in its font) drawn with its baseline origin at `origin`.
    /// ```
    /// let _ = syrp_gfx_api::Glyph::new(1, Default::default());
    /// ```
    pub const fn new(id: u32, origin: Point<f32>) -> Self {
        Glyph { id, origin }
    }

    /// The glyph id in its font.
    /// ```
    /// assert_eq!(syrp_gfx_api::Glyph::new(7, Default::default()).id(), 7);
    /// ```
    pub const fn id(self) -> u32 {
        self.id
    }

    /// The baseline origin.
    /// ```
    /// assert_eq!(syrp_gfx_api::Glyph::new(7, Default::default()).origin().x, 0.0);
    /// ```
    pub const fn origin(self) -> Point<f32> {
        self.origin
    }
}

/// How a transparency layer is composited onto what is below it when it is popped.
/// ```
/// assert_eq!(syrp_gfx_api::BlendMode::default(), syrp_gfx_api::BlendMode::Normal);
/// ```
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "snake_case")
)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BlendMode {
    /// Source over (the default).
    #[default]
    Normal,
    /// Multiply.
    Multiply,
    /// Screen.
    Screen,
    /// Overlay.
    Overlay,
    /// Darken.
    Darken,
    /// Lighten.
    Lighten,
    /// Colour dodge.
    ColourDodge,
    /// Colour burn.
    ColourBurn,
    /// Hard light.
    HardLight,
    /// Soft light.
    SoftLight,
    /// Difference.
    Difference,
    /// Exclusion.
    Exclusion,
    /// Hue of the layer, saturation and luminosity of the backdrop.
    Hue,
    /// Saturation of the layer.
    Saturation,
    /// Hue and saturation of the layer.
    Colour,
    /// Luminosity of the layer.
    Luminosity,
}

/// Image resampling quality (JUCE's Graphics::ResamplingQuality).
/// ```
/// assert_eq!(syrp_gfx_api::ResamplingQuality::default(), syrp_gfx_api::ResamplingQuality::Medium);
/// ```
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "snake_case")
)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ResamplingQuality {
    /// Nearest neighbour.
    Low,
    /// Bilinear (the default).
    #[default]
    Medium,
    /// Higher-order filtering.
    High,
}

/// One op read back from a [`DisplayList`]. Every borrowed part points into the list.
///
/// Scopes opened by the `Push*` ops are closed, innermost first, by [`Op::Pop`]; a renderer
/// treats scopes still open at the end of the list as closed there.
/// ```
/// use syrp_gfx_api::{Colour, DisplayList, Op};
/// let mut list = DisplayList::new();
/// list.fill_rect(syrp_geom::Rect::new(0, 0, 4, 4), Colour::BLACK);
/// assert!(matches!(list.ops().next(), Some(Op::FillRect { .. })));
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Op<'a> {
    /// Fill a rectangle.
    #[non_exhaustive]
    FillRect {
        /// The rectangle, in user units.
        rect: Rect<f32>,
        /// What to paint it with.
        brush: BrushRef<'a>,
    },
    /// Fill a path with its fill rule.
    #[non_exhaustive]
    FillPath {
        /// The path.
        path: PathView<'a>,
        /// What to paint it with.
        brush: BrushRef<'a>,
    },
    /// Stroke a path.
    #[non_exhaustive]
    StrokePath {
        /// The path.
        path: PathView<'a>,
        /// Stroke parameters.
        stroke: StrokeRef<'a>,
        /// What to paint the stroke with.
        brush: BrushRef<'a>,
    },
    /// Draw an image placed inside `dest`.
    #[non_exhaustive]
    DrawImage {
        /// The image.
        image: ImageHandle,
        /// The area to place it in.
        dest: Rect<f32>,
        /// How it is fitted into `dest`.
        placement: RectanglePlacement,
        /// Resampling quality.
        quality: ResamplingQuality,
    },
    /// Draw a run of glyphs from one font.
    #[non_exhaustive]
    DrawGlyphs {
        /// The font.
        font: FontHandle,
        /// Font size (em height) in user units.
        size: f32,
        /// The glyphs.
        glyphs: &'a [Glyph],
        /// What to paint them with.
        brush: BrushRef<'a>,
    },
    /// Open a scope clipped to a rectangle.
    #[non_exhaustive]
    PushClipRect {
        /// The clip rectangle.
        rect: Rect<f32>,
    },
    /// Open a scope clipped to a path (with its fill rule).
    #[non_exhaustive]
    PushClipPath {
        /// The clip path.
        path: PathView<'a>,
    },
    /// Open a scope masked by an image's alpha channel.
    #[non_exhaustive]
    PushClipMask {
        /// The mask image.
        image: ImageHandle,
        /// Maps image pixels to user space.
        transform: AffineTransform,
    },
    /// Open a scope whose transform is `transform` followed by the enclosing one.
    #[non_exhaustive]
    PushTransform {
        /// The added transform.
        transform: AffineTransform,
    },
    /// Open a transparency layer, composited with `opacity` and `blend` when popped.
    #[non_exhaustive]
    PushLayer {
        /// Opacity in `[0, 1]`.
        opacity: f32,
        /// Blend mode.
        blend: BlendMode,
    },
    /// Close the innermost open scope.
    Pop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tag {
    FillRect,
    FillPath,
    StrokePath,
    DrawImage,
    DrawGlyphs,
    ClipRect,
    ClipPath,
    ClipMask,
    Transform,
    Layer,
    Pop,
}

/// A range into one of the shared arenas.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Span {
    start: u32,
    len: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PathRec {
    verbs: Span,
    points: Span,
    rule: FillRule,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct BrushRec {
    kind: BrushKind,
    stops: Span,
}

/// Appends `items` to `arena` and returns their range.
///
/// # Panics
/// If the arena would exceed `u32::MAX` elements.
fn append<T: Copy>(arena: &mut Vec<T>, items: &[T]) -> Span {
    let span = |n: usize| u32::try_from(n).expect("display-list arena exceeds u32::MAX items");
    let start = span(arena.len());
    arena.extend_from_slice(items);
    Span {
        start,
        len: span(items.len()),
    }
}

fn slice<T>(arena: &[T], s: Span) -> &[T] {
    &arena[s.start as usize..][..s.len as usize]
}

/// A recorded sequence of drawing ops (master plan §5.7; ADR 0003 D2).
///
/// Stored as an op-tag stream, one payload array per op kind, and shared arenas for path
/// verbs, points, glyphs, gradient stops and dash lengths, all addressed by index. Paths,
/// brushes and glyphs are copied into the arenas, so the caller's values can be dropped or
/// reused. [`clear`](DisplayList::clear) keeps every buffer's capacity: re-recording a list of
/// the same shape allocates nothing. Ops come back as borrowed [`Op`] views.
///
/// Rectangles in any [`Coord`] type are stored as `f32` (rounded to nearest; integers above
/// 2^24 lose precision).
/// ```
/// use syrp_geom::{AffineTransform, Rect};
/// use syrp_gfx_api::{BlendMode, Colour, DisplayList, Op, Path};
/// let mut tri = Path::new();
/// tri.line_to(10.0, 0.0);
/// tri.line_to(5.0, 8.0);
/// tri.close();
/// let mut list = DisplayList::new();
/// list.push_transform(AffineTransform::translation(2.0, 2.0));
/// list.push_layer(0.5, BlendMode::Multiply);
/// list.fill_path(&tri, Colour::BLACK);
/// list.pop();
/// list.pop();
/// assert_eq!(list.len(), 5);
/// assert!(matches!(list.get(2), Some(Op::FillPath { .. })));
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DisplayList {
    /// The op stream: each op's kind and its index in that kind's payload array.
    ops: Vec<(Tag, u32)>,
    fill_rects: Vec<(Rect<f32>, BrushRec)>,
    fills: Vec<(PathRec, BrushRec)>,
    strokes: Vec<(PathRec, StrokeParams, Span, BrushRec)>,
    images: Vec<(
        ImageHandle,
        Rect<f32>,
        RectanglePlacement,
        ResamplingQuality,
    )>,
    glyph_runs: Vec<(FontHandle, f32, Span, BrushRec)>,
    clip_rects: Vec<Rect<f32>>,
    clip_paths: Vec<PathRec>,
    clip_masks: Vec<(ImageHandle, AffineTransform)>,
    transforms: Vec<AffineTransform>,
    layers: Vec<(f32, BlendMode)>,
    verbs: Vec<PathVerb>,
    points: Vec<Point<f32>>,
    glyphs: Vec<Glyph>,
    stops: Vec<GradientStop>,
    dashes: Vec<f32>,
    open: u32,
}

impl DisplayList {
    /// An empty list.
    /// ```
    /// assert!(syrp_gfx_api::DisplayList::new().is_empty());
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Removes every op, keeping all capacity for re-recording.
    /// ```
    /// let mut list = syrp_gfx_api::DisplayList::new();
    /// list.pop_all();
    /// list.clear();
    /// assert!(list.is_empty());
    /// ```
    pub fn clear(&mut self) {
        self.ops.clear();
        self.fill_rects.clear();
        self.fills.clear();
        self.strokes.clear();
        self.images.clear();
        self.glyph_runs.clear();
        self.clip_rects.clear();
        self.clip_paths.clear();
        self.clip_masks.clear();
        self.transforms.clear();
        self.layers.clear();
        self.verbs.clear();
        self.points.clear();
        self.glyphs.clear();
        self.stops.clear();
        self.dashes.clear();
        self.open = 0;
    }

    /// The number of ops.
    /// ```
    /// assert_eq!(syrp_gfx_api::DisplayList::new().len(), 0);
    /// ```
    pub fn len(&self) -> usize {
        self.ops.len()
    }

    /// True if no ops are recorded.
    /// ```
    /// assert!(syrp_gfx_api::DisplayList::new().is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// The number of scopes opened by `push_*` and not yet popped.
    /// ```
    /// let mut list = syrp_gfx_api::DisplayList::new();
    /// list.push_clip_rect(syrp_geom::Rect::new(0, 0, 8, 8));
    /// assert_eq!(list.open_scopes(), 1);
    /// ```
    pub fn open_scopes(&self) -> u32 {
        self.open
    }

    /// Fills `rect` with `brush`.
    /// ```
    /// let mut list = syrp_gfx_api::DisplayList::new();
    /// list.fill_rect(syrp_geom::Rect::new(0.5, 0.5, 2.0, 2.0), syrp_gfx_api::Colour::WHITE);
    /// ```
    #[doc(alias = "fillRect")]
    pub fn fill_rect<'b, T: Coord>(&mut self, rect: Rect<T>, brush: impl Into<BrushRef<'b>>) {
        let rec = (rect.cast(), self.add_brush(brush.into()));
        Self::record(&mut self.ops, Tag::FillRect, &mut self.fill_rects, rec);
    }

    /// Fills `path` (using its fill rule) with `brush`.
    /// ```
    /// let mut list = syrp_gfx_api::DisplayList::new();
    /// list.fill_path(&syrp_gfx_api::Path::new(), syrp_gfx_api::Colour::WHITE);
    /// ```
    #[doc(alias = "fillPath")]
    pub fn fill_path<'p, 'b>(
        &mut self,
        path: impl Into<PathView<'p>>,
        brush: impl Into<BrushRef<'b>>,
    ) {
        let rec = (self.add_path(path.into()), self.add_brush(brush.into()));
        Self::record(&mut self.ops, Tag::FillPath, &mut self.fills, rec);
    }

    /// Strokes `path` with `stroke`, painted with `brush`.
    /// ```
    /// use syrp_gfx_api::{Colour, DisplayList, Path, Stroke};
    /// let mut list = DisplayList::new();
    /// list.stroke_path(&Path::new(), &Stroke::new(1.0), Colour::BLACK);
    /// ```
    #[doc(alias = "strokePath")]
    pub fn stroke_path<'p, 's, 'b>(
        &mut self,
        path: impl Into<PathView<'p>>,
        stroke: impl Into<StrokeRef<'s>>,
        brush: impl Into<BrushRef<'b>>,
    ) {
        let stroke = stroke.into();
        let path = self.add_path(path.into());
        let dashes = append(&mut self.dashes, stroke.dashes());
        let rec = (path, stroke.params(), dashes, self.add_brush(brush.into()));
        Self::record(&mut self.ops, Tag::StrokePath, &mut self.strokes, rec);
    }

    /// Draws `image` fitted into `dest` by `placement`.
    /// ```
    /// use syrp_geom::{Rect, RectanglePlacement};
    /// use syrp_gfx_api::{DisplayList, ImageHandle, ResamplingQuality};
    /// let mut list = DisplayList::new();
    /// let dest = Rect::new(0, 0, 64, 64);
    /// list.draw_image(ImageHandle::from_raw(1), dest, RectanglePlacement::CENTRED, ResamplingQuality::High);
    /// ```
    #[doc(alias = "drawImage")]
    pub fn draw_image<T: Coord>(
        &mut self,
        image: ImageHandle,
        dest: Rect<T>,
        placement: RectanglePlacement,
        quality: ResamplingQuality,
    ) {
        let rec = (image, dest.cast(), placement, quality);
        Self::record(&mut self.ops, Tag::DrawImage, &mut self.images, rec);
    }

    /// Draws `glyphs` from `font` at `size` (negative or NaN sizes become 0).
    /// ```
    /// use syrp_gfx_api::{Colour, DisplayList, FontHandle, Glyph};
    /// let mut list = DisplayList::new();
    /// let run = [Glyph::new(3, Default::default())];
    /// list.draw_glyphs(FontHandle::from_raw(1), 12.0, &run, Colour::BLACK);
    /// ```
    #[doc(alias = "drawGlyphs")]
    pub fn draw_glyphs<'b>(
        &mut self,
        font: FontHandle,
        size: f32,
        glyphs: &[Glyph],
        brush: impl Into<BrushRef<'b>>,
    ) {
        let glyphs = append(&mut self.glyphs, glyphs);
        let rec = (font, non_neg(size), glyphs, self.add_brush(brush.into()));
        Self::record(&mut self.ops, Tag::DrawGlyphs, &mut self.glyph_runs, rec);
    }

    /// Opens a scope clipped to `rect`.
    /// ```
    /// let mut list = syrp_gfx_api::DisplayList::new();
    /// list.push_clip_rect(syrp_geom::Rect::new(0, 0, 8, 8));
    /// ```
    #[doc(alias = "reduceClipRegion")]
    pub fn push_clip_rect<T: Coord>(&mut self, rect: Rect<T>) {
        Self::record(
            &mut self.ops,
            Tag::ClipRect,
            &mut self.clip_rects,
            rect.cast(),
        );
        self.open += 1;
    }

    /// Opens a scope clipped to `path` (using its fill rule).
    /// ```
    /// let mut list = syrp_gfx_api::DisplayList::new();
    /// list.push_clip_path(&syrp_gfx_api::Path::new());
    /// ```
    pub fn push_clip_path<'p>(&mut self, path: impl Into<PathView<'p>>) {
        let rec = self.add_path(path.into());
        Self::record(&mut self.ops, Tag::ClipPath, &mut self.clip_paths, rec);
        self.open += 1;
    }

    /// Opens a scope masked by the alpha of `image`, placed by `transform`.
    /// ```
    /// use syrp_geom::AffineTransform;
    /// let mut list = syrp_gfx_api::DisplayList::new();
    /// list.push_clip_mask(syrp_gfx_api::ImageHandle::from_raw(2), AffineTransform::IDENTITY);
    /// ```
    pub fn push_clip_mask(&mut self, image: ImageHandle, transform: AffineTransform) {
        let rec = (image, transform);
        Self::record(&mut self.ops, Tag::ClipMask, &mut self.clip_masks, rec);
        self.open += 1;
    }

    /// Opens a scope that applies `transform` before the enclosing transform.
    /// ```
    /// let mut list = syrp_gfx_api::DisplayList::new();
    /// list.push_transform(syrp_geom::AffineTransform::scale(2.0, 2.0));
    /// ```
    #[doc(alias = "addTransform")]
    pub fn push_transform(&mut self, transform: AffineTransform) {
        Self::record(
            &mut self.ops,
            Tag::Transform,
            &mut self.transforms,
            transform,
        );
        self.open += 1;
    }

    /// Opens a transparency layer; `opacity` is clamped to `[0, 1]` (NaN becomes 0).
    /// ```
    /// use syrp_gfx_api::{BlendMode, DisplayList};
    /// let mut list = DisplayList::new();
    /// list.push_layer(0.5, BlendMode::Screen);
    /// ```
    #[doc(alias = "beginTransparencyLayer")]
    pub fn push_layer(&mut self, opacity: f32, blend: BlendMode) {
        let rec = (non_neg(opacity).min(1.0), blend);
        Self::record(&mut self.ops, Tag::Layer, &mut self.layers, rec);
        self.open += 1;
    }

    /// Closes the innermost open scope. Returns false, recording nothing, if none is open.
    /// ```
    /// let mut list = syrp_gfx_api::DisplayList::new();
    /// assert!(!list.pop());
    /// ```
    #[doc(alias = "restoreState")]
    #[doc(alias = "endTransparencyLayer")]
    pub fn pop(&mut self) -> bool {
        if self.open == 0 {
            return false;
        }
        self.open -= 1;
        self.ops.push((Tag::Pop, 0));
        true
    }

    /// Closes every open scope.
    /// ```
    /// let mut list = syrp_gfx_api::DisplayList::new();
    /// list.push_layer(1.0, Default::default());
    /// list.pop_all();
    /// assert_eq!(list.open_scopes(), 0);
    /// ```
    pub fn pop_all(&mut self) {
        while self.pop() {}
    }

    /// The op at `index`, or `None` past the end.
    /// ```
    /// assert!(syrp_gfx_api::DisplayList::new().get(0).is_none());
    /// ```
    pub fn get(&self, index: usize) -> Option<Op<'_>> {
        self.ops
            .get(index)
            .map(|&(tag, i)| self.view(tag, i as usize))
    }

    /// The ops in recording order, as borrowed views.
    /// ```
    /// assert_eq!(syrp_gfx_api::DisplayList::new().ops().count(), 0);
    /// ```
    pub fn ops(&self) -> impl ExactSizeIterator<Item = Op<'_>> + use<'_> {
        self.ops.iter().map(|&(tag, i)| self.view(tag, i as usize))
    }

    fn record<T>(ops: &mut Vec<(Tag, u32)>, tag: Tag, payloads: &mut Vec<T>, rec: T) {
        let i = u32::try_from(payloads.len()).expect("display-list payloads exceed u32::MAX");
        payloads.push(rec);
        ops.push((tag, i));
    }

    fn add_path(&mut self, path: PathView<'_>) -> PathRec {
        let verbs = append(&mut self.verbs, path.verbs());
        let points = append(&mut self.points, path.points());
        PathRec {
            verbs,
            points,
            rule: path.fill_rule(),
        }
    }

    fn add_brush(&mut self, brush: BrushRef<'_>) -> BrushRec {
        BrushRec {
            kind: brush.kind(),
            stops: append(&mut self.stops, brush.stops()),
        }
    }

    fn path(&self, r: PathRec) -> PathView<'_> {
        PathView::new(
            slice(&self.verbs, r.verbs),
            slice(&self.points, r.points),
            r.rule,
        )
    }

    fn brush(&self, r: BrushRec) -> BrushRef<'_> {
        BrushRef::new(r.kind, slice(&self.stops, r.stops))
    }

    fn view(&self, tag: Tag, i: usize) -> Op<'_> {
        match tag {
            Tag::FillRect => {
                let (rect, brush) = self.fill_rects[i];
                Op::FillRect {
                    rect,
                    brush: self.brush(brush),
                }
            }
            Tag::FillPath => {
                let (path, brush) = self.fills[i];
                Op::FillPath {
                    path: self.path(path),
                    brush: self.brush(brush),
                }
            }
            Tag::StrokePath => {
                let (path, params, dashes, brush) = self.strokes[i];
                let stroke = StrokeRef::new(params, slice(&self.dashes, dashes));
                Op::StrokePath {
                    path: self.path(path),
                    stroke,
                    brush: self.brush(brush),
                }
            }
            Tag::DrawImage => {
                let (image, dest, placement, quality) = self.images[i];
                Op::DrawImage {
                    image,
                    dest,
                    placement,
                    quality,
                }
            }
            Tag::DrawGlyphs => {
                let (font, size, glyphs, brush) = self.glyph_runs[i];
                let (glyphs, brush) = (slice(&self.glyphs, glyphs), self.brush(brush));
                Op::DrawGlyphs {
                    font,
                    size,
                    glyphs,
                    brush,
                }
            }
            Tag::ClipRect => Op::PushClipRect {
                rect: self.clip_rects[i],
            },
            Tag::ClipPath => Op::PushClipPath {
                path: self.path(self.clip_paths[i]),
            },
            Tag::ClipMask => {
                let (image, transform) = self.clip_masks[i];
                Op::PushClipMask { image, transform }
            }
            Tag::Transform => Op::PushTransform {
                transform: self.transforms[i],
            },
            Tag::Layer => {
                let (opacity, blend) = self.layers[i];
                Op::PushLayer { opacity, blend }
            }
            Tag::Pop => Op::Pop,
        }
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0
//! Paths (a verb array plus a point array) and stroke parameters.

use syrp_geom::{Point, Rect};

/// One path command. Each verb consumes [`point_count`](PathVerb::point_count) points.
/// ```
/// assert_eq!(syrp_gfx_api::PathVerb::CubicTo.point_count(), 3);
/// ```
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "snake_case")
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PathVerb {
    /// Starts a subpath at one point.
    MoveTo,
    /// A straight line to one point.
    LineTo,
    /// A quadratic Bézier: control point, end point.
    QuadTo,
    /// A cubic Bézier: two control points, end point.
    CubicTo,
    /// Closes the subpath with a line back to its start. Uses no points.
    Close,
}

impl PathVerb {
    /// The number of points this verb takes from the point array.
    /// ```
    /// assert_eq!(syrp_gfx_api::PathVerb::Close.point_count(), 0);
    /// ```
    pub const fn point_count(self) -> usize {
        match self {
            PathVerb::MoveTo | PathVerb::LineTo => 1,
            PathVerb::QuadTo => 2,
            PathVerb::CubicTo => 3,
            PathVerb::Close => 0,
        }
    }
}

/// How the inside of a path is decided.
/// ```
/// assert_eq!(syrp_gfx_api::FillRule::default(), syrp_gfx_api::FillRule::NonZero);
/// ```
#[doc(alias = "setUsingNonZeroWinding")]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "snake_case")
)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FillRule {
    /// Inside where the winding number is not zero (the default).
    #[default]
    NonZero,
    /// Inside where the winding number is odd.
    EvenOdd,
}

/// A path element with its points, as yielded by [`PathView::elements`].
/// ```
/// use syrp_gfx_api::{Path, PathEl};
/// let mut p = Path::new();
/// p.line_to(1.0, 0.0);
/// assert!(matches!(p.as_view().elements().next(), Some(PathEl::MoveTo(_))));
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathEl {
    /// Start a subpath.
    MoveTo(Point<f32>),
    /// Straight line.
    LineTo(Point<f32>),
    /// Quadratic Bézier (control, end).
    QuadTo(Point<f32>, Point<f32>),
    /// Cubic Bézier (control 1, control 2, end).
    CubicTo(Point<f32>, Point<f32>, Point<f32>),
    /// Close the subpath.
    Close,
}

/// An owned path: a verb array and a point array (JUCE's Path).
///
/// A drawing verb with no open subpath (on an empty path, or after [`close`](Path::close))
/// first records a `MoveTo` to the current point (the origin, or the closed subpath's start),
/// so every subpath begins with `MoveTo`. Read it back through [`as_view`](Path::as_view).
/// ```
/// use syrp_gfx_api::Path;
/// let mut p = Path::new();
/// p.move_to(0.0, 0.0);
/// p.line_to(10.0, 0.0);
/// p.quad_to(10.0, 10.0, 0.0, 10.0);
/// p.close();
/// assert_eq!(p.as_view().verbs().len(), 4);
/// assert_eq!(p.bounds().unwrap().width(), 10.0);
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    verbs: Vec<PathVerb>,
    points: Vec<Point<f32>>,
    fill_rule: FillRule,
    in_subpath: bool,
    start: Point<f32>,
}

impl Path {
    /// An empty path with the non-zero fill rule.
    /// ```
    /// assert!(syrp_gfx_api::Path::new().is_empty());
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts a new subpath at (x, y).
    /// ```
    /// let mut p = syrp_gfx_api::Path::new();
    /// p.move_to(1.0, 2.0);
    /// ```
    #[doc(alias = "startNewSubPath")]
    pub fn move_to(&mut self, x: f32, y: f32) {
        self.start = Point::new(x, y);
        self.push(PathVerb::MoveTo, &[self.start]);
        self.in_subpath = true;
    }

    /// Adds a straight line to (x, y).
    /// ```
    /// let mut p = syrp_gfx_api::Path::new();
    /// p.line_to(1.0, 2.0);
    /// ```
    #[doc(alias = "lineTo")]
    pub fn line_to(&mut self, x: f32, y: f32) {
        self.draw(PathVerb::LineTo, &[Point::new(x, y)]);
    }

    /// Adds a quadratic Bézier with control (cx, cy) ending at (x, y).
    /// ```
    /// let mut p = syrp_gfx_api::Path::new();
    /// p.quad_to(1.0, 2.0, 3.0, 4.0);
    /// ```
    #[doc(alias = "quadraticTo")]
    pub fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.draw(PathVerb::QuadTo, &[Point::new(cx, cy), Point::new(x, y)]);
    }

    /// Adds a cubic Bézier with controls (c1x, c1y), (c2x, c2y) ending at (x, y).
    /// ```
    /// let mut p = syrp_gfx_api::Path::new();
    /// p.cubic_to(1.0, 2.0, 3.0, 4.0, 5.0, 6.0);
    /// ```
    #[doc(alias = "cubicTo")]
    pub fn cubic_to(&mut self, c1x: f32, c1y: f32, c2x: f32, c2y: f32, x: f32, y: f32) {
        let pts = [Point::new(c1x, c1y), Point::new(c2x, c2y), Point::new(x, y)];
        self.draw(PathVerb::CubicTo, &pts);
    }

    /// Closes the open subpath; does nothing if none is open.
    /// ```
    /// let mut p = syrp_gfx_api::Path::new();
    /// p.close();
    /// assert!(p.is_empty());
    /// ```
    #[doc(alias = "closeSubPath")]
    pub fn close(&mut self) {
        if self.in_subpath {
            self.push(PathVerb::Close, &[]);
            self.in_subpath = false;
        }
    }

    /// Sets the fill rule.
    /// ```
    /// use syrp_gfx_api::{FillRule, Path};
    /// let mut p = Path::new();
    /// p.set_fill_rule(FillRule::EvenOdd);
    /// assert_eq!(p.as_view().fill_rule(), FillRule::EvenOdd);
    /// ```
    pub fn set_fill_rule(&mut self, rule: FillRule) {
        self.fill_rule = rule;
    }

    /// Removes every element, keeping capacity and the fill rule.
    /// ```
    /// let mut p = syrp_gfx_api::Path::new();
    /// p.line_to(1.0, 1.0);
    /// p.clear();
    /// assert!(p.is_empty());
    /// ```
    pub fn clear(&mut self) {
        self.verbs.clear();
        self.points.clear();
        self.in_subpath = false;
        self.start = Point::origin();
    }

    /// True if the path has no verbs.
    /// ```
    /// assert!(syrp_gfx_api::Path::new().is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.verbs.is_empty()
    }

    /// The bounds of every point, control points included (see [`PathView::bounds`]).
    /// ```
    /// assert_eq!(syrp_gfx_api::Path::new().bounds(), None);
    /// ```
    #[doc(alias = "getBounds")]
    pub fn bounds(&self) -> Option<Rect<f32>> {
        self.as_view().bounds()
    }

    /// A borrowed view of this path.
    /// ```
    /// let p = syrp_gfx_api::Path::new();
    /// assert_eq!(p.as_view().elements().count(), 0);
    /// ```
    pub fn as_view(&self) -> PathView<'_> {
        PathView::new(&self.verbs, &self.points, self.fill_rule)
    }

    fn draw(&mut self, verb: PathVerb, pts: &[Point<f32>]) {
        if !self.in_subpath {
            self.move_to(self.start.x, self.start.y);
        }
        self.push(verb, pts);
    }

    fn push(&mut self, verb: PathVerb, pts: &[Point<f32>]) {
        self.verbs.push(verb);
        self.points.extend_from_slice(pts);
    }
}

/// A borrowed path: from a [`Path`], or read back from a `DisplayList`'s arenas.
/// ```
/// let mut p = syrp_gfx_api::Path::new();
/// p.line_to(2.0, 3.0);
/// let v = p.as_view();
/// assert_eq!(v.points().len(), 2);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathView<'a> {
    verbs: &'a [PathVerb],
    points: &'a [Point<f32>],
    fill_rule: FillRule,
}

impl<'a> PathView<'a> {
    pub(crate) fn new(verbs: &'a [PathVerb], points: &'a [Point<f32>], rule: FillRule) -> Self {
        PathView {
            verbs,
            points,
            fill_rule: rule,
        }
    }

    /// The verb array.
    /// ```
    /// assert!(syrp_gfx_api::Path::new().as_view().verbs().is_empty());
    /// ```
    pub fn verbs(self) -> &'a [PathVerb] {
        self.verbs
    }

    /// The point array.
    /// ```
    /// assert!(syrp_gfx_api::Path::new().as_view().points().is_empty());
    /// ```
    pub fn points(self) -> &'a [Point<f32>] {
        self.points
    }

    /// The fill rule.
    /// ```
    /// let p = syrp_gfx_api::Path::new();
    /// assert_eq!(p.as_view().fill_rule(), syrp_gfx_api::FillRule::NonZero);
    /// ```
    pub fn fill_rule(self) -> FillRule {
        self.fill_rule
    }

    /// The elements in order, each with its points.
    /// ```
    /// assert_eq!(syrp_gfx_api::Path::new().as_view().elements().count(), 0);
    /// ```
    pub fn elements(self) -> impl Iterator<Item = PathEl> + use<'a> {
        let mut pts = self.points.iter().copied();
        let mut next = move || pts.next().unwrap_or_default();
        self.verbs.iter().map(move |verb| match verb {
            PathVerb::MoveTo => PathEl::MoveTo(next()),
            PathVerb::LineTo => PathEl::LineTo(next()),
            PathVerb::QuadTo => PathEl::QuadTo(next(), next()),
            PathVerb::CubicTo => PathEl::CubicTo(next(), next(), next()),
            PathVerb::Close => PathEl::Close,
        })
    }

    /// The smallest rectangle holding every point, curve control points included, so it is
    /// a conservative bound for curves. `None` for an empty path. NaN coordinates are skipped.
    /// ```
    /// let mut p = syrp_gfx_api::Path::new();
    /// p.quad_to(5.0, 10.0, 10.0, 0.0);
    /// assert_eq!(p.as_view().bounds().unwrap().height(), 10.0);
    /// ```
    pub fn bounds(self) -> Option<Rect<f32>> {
        let mut it = self
            .points
            .iter()
            .filter(|p| !p.x.is_nan() && !p.y.is_nan());
        let first = it.next()?;
        let (mut l, mut t, mut r, mut b) = (first.x, first.y, first.x, first.y);
        for p in it {
            (l, t, r, b) = (l.min(p.x), t.min(p.y), r.max(p.x), b.max(p.y));
        }
        Some(Rect::from_ltrb(l, t, r, b))
    }
}

/// How stroke segments meet (JUCE's PathStrokeType::JointStyle).
/// ```
/// assert_eq!(syrp_gfx_api::Join::default(), syrp_gfx_api::Join::Miter);
/// ```
#[doc(alias = "JointStyle")]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "snake_case")
)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Join {
    /// Sharp corners, bevelled beyond the miter limit (the default).
    #[default]
    Miter,
    /// Rounded corners.
    Round,
    /// Flattened corners.
    Bevel,
}

/// How open subpaths end (JUCE's PathStrokeType::EndCapStyle).
/// ```
/// assert_eq!(syrp_gfx_api::Cap::default(), syrp_gfx_api::Cap::Butt);
/// ```
#[doc(alias = "EndCapStyle")]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "snake_case")
)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Cap {
    /// Ends flat at the end point (the default).
    #[default]
    Butt,
    /// Ends square, extended by half the width.
    Square,
    /// Ends with a half circle.
    Round,
}

/// Clamps to `[0, inf)`, mapping NaN and `-0.0` to `0.0`.
pub(crate) fn non_neg(v: f32) -> f32 {
    if v > 0.0 { v } else { 0.0 }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct StrokeParams {
    pub(crate) width: f32,
    pub(crate) miter_limit: f32,
    pub(crate) join: Join,
    pub(crate) cap: Cap,
    pub(crate) dash_offset: f32,
}

/// Stroke parameters: width, join, cap, miter limit and dashes (JUCE's PathStrokeType).
///
/// Negative or NaN widths, miter limits and dash lengths become 0. An empty dash list, or one
/// whose lengths sum to 0, strokes solid; dash lengths alternate on, off, on...
/// ```
/// use syrp_gfx_api::{Cap, Join, Stroke};
/// let s = Stroke::new(2.0).with_join(Join::Round).with_cap(Cap::Round).with_dashes([4.0, 2.0], 0.0);
/// assert_eq!(s.as_view().dashes(), &[4.0, 2.0]);
/// ```
#[doc(alias = "PathStrokeType")]
#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    params: StrokeParams,
    dashes: Vec<f32>,
}

impl Stroke {
    /// A solid stroke of `width` with miter joins (limit 4), butt caps and no dashes.
    /// ```
    /// assert_eq!(syrp_gfx_api::Stroke::new(-1.0).as_view().width(), 0.0);
    /// ```
    pub fn new(width: f32) -> Self {
        let params = StrokeParams {
            width: non_neg(width),
            miter_limit: 4.0,
            join: Join::Miter,
            cap: Cap::Butt,
            dash_offset: 0.0,
        };
        Stroke {
            params,
            dashes: Vec::new(),
        }
    }

    /// Sets the join style.
    /// ```
    /// let _ = syrp_gfx_api::Stroke::new(1.0).with_join(syrp_gfx_api::Join::Bevel);
    /// ```
    #[must_use]
    pub fn with_join(mut self, join: Join) -> Self {
        self.params.join = join;
        self
    }

    /// Sets the cap style.
    /// ```
    /// let _ = syrp_gfx_api::Stroke::new(1.0).with_cap(syrp_gfx_api::Cap::Square);
    /// ```
    #[must_use]
    pub fn with_cap(mut self, cap: Cap) -> Self {
        self.params.cap = cap;
        self
    }

    /// Sets the miter limit (ratio of miter length to width).
    /// ```
    /// let s = syrp_gfx_api::Stroke::new(1.0).with_miter_limit(10.0);
    /// assert_eq!(s.as_view().miter_limit(), 10.0);
    /// ```
    #[must_use]
    pub fn with_miter_limit(mut self, limit: f32) -> Self {
        self.params.miter_limit = non_neg(limit);
        self
    }

    /// Sets the dash lengths and the distance into the pattern at which dashing starts.
    /// ```
    /// let s = syrp_gfx_api::Stroke::new(1.0).with_dashes([3.0, f32::NAN], 1.0);
    /// assert_eq!(s.as_view().dashes(), &[3.0, 0.0]);
    /// ```
    #[doc(alias = "createDashedStroke")]
    #[must_use]
    pub fn with_dashes(mut self, lengths: impl IntoIterator<Item = f32>, offset: f32) -> Self {
        self.dashes.clear();
        self.dashes.extend(lengths.into_iter().map(non_neg));
        self.params.dash_offset = if offset.is_finite() { offset } else { 0.0 };
        self
    }

    /// A borrowed view of this stroke.
    /// ```
    /// assert_eq!(syrp_gfx_api::Stroke::new(3.0).as_view().width(), 3.0);
    /// ```
    pub fn as_view(&self) -> StrokeRef<'_> {
        StrokeRef::new(self.params, &self.dashes)
    }
}

/// A borrowed [`Stroke`], as read back from a `DisplayList`.
/// ```
/// let s = syrp_gfx_api::Stroke::new(1.5);
/// assert_eq!(s.as_view().cap(), syrp_gfx_api::Cap::Butt);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrokeRef<'a> {
    params: StrokeParams,
    dashes: &'a [f32],
}

impl<'a> StrokeRef<'a> {
    pub(crate) fn new(params: StrokeParams, dashes: &'a [f32]) -> Self {
        StrokeRef { params, dashes }
    }

    pub(crate) fn params(self) -> StrokeParams {
        self.params
    }

    /// Stroke width in user units.
    /// ```
    /// assert_eq!(syrp_gfx_api::Stroke::new(2.0).as_view().width(), 2.0);
    /// ```
    pub fn width(self) -> f32 {
        self.params.width
    }

    /// The join style.
    /// ```
    /// assert_eq!(syrp_gfx_api::Stroke::new(2.0).as_view().join(), syrp_gfx_api::Join::Miter);
    /// ```
    pub fn join(self) -> Join {
        self.params.join
    }

    /// The cap style.
    /// ```
    /// assert_eq!(syrp_gfx_api::Stroke::new(2.0).as_view().cap(), syrp_gfx_api::Cap::Butt);
    /// ```
    pub fn cap(self) -> Cap {
        self.params.cap
    }

    /// The miter limit.
    /// ```
    /// assert_eq!(syrp_gfx_api::Stroke::new(2.0).as_view().miter_limit(), 4.0);
    /// ```
    pub fn miter_limit(self) -> f32 {
        self.params.miter_limit
    }

    /// The dash lengths (empty for a solid stroke).
    /// ```
    /// assert!(syrp_gfx_api::Stroke::new(2.0).as_view().dashes().is_empty());
    /// ```
    pub fn dashes(self) -> &'a [f32] {
        self.dashes
    }

    /// The distance into the dash pattern at which the stroke starts.
    /// ```
    /// assert_eq!(syrp_gfx_api::Stroke::new(2.0).as_view().dash_offset(), 0.0);
    /// ```
    pub fn dash_offset(self) -> f32 {
        self.params.dash_offset
    }
}

impl<'a> From<&'a Path> for PathView<'a> {
    fn from(path: &'a Path) -> Self {
        path.as_view()
    }
}

impl<'a> From<&'a Stroke> for StrokeRef<'a> {
    fn from(stroke: &'a Stroke) -> Self {
        stroke.as_view()
    }
}

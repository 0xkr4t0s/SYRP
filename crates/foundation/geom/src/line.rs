//! [`Line`] (a segment between two points) and [`Parallelogram`].

use crate::{AffineTransform, Coord, Point, Rect};

/// Cross product of two `f64` vectors.
fn cross(a: (f64, f64), b: (f64, f64)) -> f64 {
    a.0 * b.1 - a.1 * b.0
}

/// A straight line segment from `start` to `end`.
///
/// Lengths, angles and positions along the line are computed in `f64`. Methods that return a
/// `Point<T>` round to nearest (halves away from zero) when `T` is `i32`.
/// ```
/// use syrp_geom::{Line, Point};
/// let l = Line::new(Point::new(0.0, 0.0), Point::new(3.0, 4.0));
/// assert_eq!(l.length(), 5.0);
/// assert_eq!(l.point_along(2.5), Point::new(1.5, 2.0));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Line<T> {
    /// The start point.
    pub start: Point<T>,
    /// The end point.
    pub end: Point<T>,
}

impl<T: Coord> Line<T> {
    /// Creates a line from `start` to `end`.
    /// ```
    /// use syrp_geom::{Line, Point};
    /// assert_eq!(Line::new(Point::new(1, 2), Point::new(3, 4)).end, Point::new(3, 4));
    /// ```
    pub const fn new(start: Point<T>, end: Point<T>) -> Self {
        Self { start, end }
    }

    /// The same line running the other way.
    /// ```
    /// use syrp_geom::{Line, Point};
    /// let l = Line::new(Point::new(1, 2), Point::new(3, 4)).reversed();
    /// assert_eq!(l.start, Point::new(3, 4));
    /// ```
    pub fn reversed(self) -> Self {
        Self::new(self.end, self.start)
    }

    /// The vector from start to end, in `f64`.
    fn delta(&self) -> (f64, f64) {
        (
            self.end.x.to_f64() - self.start.x.to_f64(),
            self.end.y.to_f64() - self.start.y.to_f64(),
        )
    }

    /// Distance from start to end.
    /// ```
    /// use syrp_geom::{Line, Point};
    /// assert_eq!(Line::new(Point::new(1, 1), Point::new(4, 5)).length(), 5.0);
    /// ```
    #[doc(alias = "getLength")]
    pub fn length(&self) -> f64 {
        let (dx, dy) = self.delta();
        dx.hypot(dy)
    }

    /// The direction from start to end in radians, measured clockwise (on screen, y down) from
    /// straight up: up is 0, right is pi/2, down is pi and left is -pi/2. A zero-length line
    /// returns 0.
    /// ```
    /// use syrp_geom::{Line, Point};
    /// let right = Line::new(Point::new(0.0, 0.0), Point::new(1.0, 0.0));
    /// assert_eq!(right.angle(), std::f64::consts::FRAC_PI_2);
    /// ```
    #[doc(alias = "getAngle")]
    pub fn angle(&self) -> f64 {
        let (dx, dy) = self.delta();
        // `0.0 - dy` rather than `-dy` so a zero-length line gives atan2(0, +0) = 0.
        dx.atan2(0.0 - dy)
    }

    /// The point `proportion` of the way along (0 is start, 1 is end; values outside extrapolate).
    /// ```
    /// use syrp_geom::{Line, Point};
    /// let l = Line::new(Point::new(0, 0), Point::new(10, 5));
    /// assert_eq!(l.point_along_proportionally(0.5), Point::new(5, 3)); // 2.5 rounds to 3
    /// ```
    #[doc(alias = "getPointAlongLineProportionally")]
    pub fn point_along_proportionally(&self, proportion: f64) -> Point<T> {
        let (dx, dy) = self.delta();
        Point::new(
            T::from_f64(self.start.x.to_f64() + dx * proportion),
            T::from_f64(self.start.y.to_f64() + dy * proportion),
        )
    }

    /// The point `distance` along the line from its start (negative goes backwards). A
    /// zero-length line returns its start.
    /// ```
    /// use syrp_geom::{Line, Point};
    /// let l = Line::new(Point::new(0.0, 0.0), Point::new(0.0, 10.0));
    /// assert_eq!(l.point_along(4.0), Point::new(0.0, 4.0));
    /// ```
    #[doc(alias = "getPointAlongLine")]
    pub fn point_along(&self, distance: f64) -> Point<T> {
        let len = self.length();
        if len == 0.0 {
            return self.start;
        }
        self.point_along_proportionally(distance / len)
    }

    /// The point where the two segments cross, or `None` if they do not meet. Endpoints count
    /// as part of each segment; the segment positions get a 1e-12 tolerance so floating-point
    /// error cannot lose an exact endpoint touch. This is segment-only: lines that would only
    /// meet if extended return `None`. Parallel segments (including overlapping collinear ones) return
    /// `None`. Integer results are rounded to nearest.
    /// ```
    /// use syrp_geom::{Line, Point};
    /// let a = Line::new(Point::new(0.0, 0.0), Point::new(4.0, 4.0));
    /// let b = Line::new(Point::new(0.0, 4.0), Point::new(4.0, 0.0));
    /// assert_eq!(a.intersection(&b), Some(Point::new(2.0, 2.0)));
    /// ```
    #[doc(alias = "intersects")]
    pub fn intersection(&self, other: &Self) -> Option<Point<T>> {
        let (r, s) = (self.delta(), other.delta());
        let denom = cross(r, s);
        if denom == 0.0 || !denom.is_finite() {
            return None;
        }
        let qp = (
            other.start.x.to_f64() - self.start.x.to_f64(),
            other.start.y.to_f64() - self.start.y.to_f64(),
        );
        let t = cross(qp, s) / denom;
        let u = cross(qp, r) / denom;
        // A small tolerance so that a segment starting exactly on the other still meets it
        // when floating-point error puts `t` or `u` a hair outside 0..=1.
        const EPS: f64 = 1e-12;
        let inside = |v: f64| (-EPS..=1.0 + EPS).contains(&v);
        (inside(t) && inside(u)).then(|| self.point_along_proportionally(t.clamp(0.0, 1.0)))
    }
}

/// A parallelogram given by three corners; the fourth is implied.
///
/// This is the shape a rectangle becomes under an [`AffineTransform`].
/// ```
/// use syrp_geom::{Parallelogram, Point, Rect};
/// let p = Parallelogram::from(Rect::new(0, 0, 4, 2));
/// assert_eq!(p.bottom_right(), Point::new(4, 2));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Parallelogram<T> {
    /// The corner that was the rectangle's top-left.
    pub top_left: Point<T>,
    /// The corner that was the rectangle's top-right.
    pub top_right: Point<T>,
    /// The corner that was the rectangle's bottom-left.
    pub bottom_left: Point<T>,
}

impl<T: Coord> Parallelogram<T> {
    /// Creates a parallelogram from three corners.
    /// ```
    /// use syrp_geom::{Parallelogram, Point};
    /// let p = Parallelogram::new(Point::new(0, 0), Point::new(2, 0), Point::new(1, 1));
    /// assert_eq!(p.bottom_right(), Point::new(3, 1));
    /// ```
    pub const fn new(top_left: Point<T>, top_right: Point<T>, bottom_left: Point<T>) -> Self {
        Self {
            top_left,
            top_right,
            bottom_left,
        }
    }

    /// The implied fourth corner, `top_right + bottom_left - top_left`.
    /// ```
    /// use syrp_geom::{Parallelogram, Point};
    /// let p = Parallelogram::new(Point::new(1.0, 1.0), Point::new(3.0, 1.0), Point::new(1.0, 2.0));
    /// assert_eq!(p.bottom_right(), Point::new(3.0, 2.0));
    /// ```
    #[doc(alias = "getBottomRight")]
    pub fn bottom_right(&self) -> Point<T> {
        self.top_right + self.bottom_left - self.top_left
    }

    /// Length of the top edge.
    /// ```
    /// assert_eq!(syrp_geom::Parallelogram::from(syrp_geom::Rect::new(0, 0, 4, 2)).width(), 4.0);
    /// ```
    #[doc(alias = "getWidth")]
    pub fn width(&self) -> f64 {
        self.top_left.distance_from(self.top_right)
    }

    /// Length of the left edge.
    /// ```
    /// assert_eq!(syrp_geom::Parallelogram::from(syrp_geom::Rect::new(0, 0, 4, 2)).height(), 2.0);
    /// ```
    #[doc(alias = "getHeight")]
    pub fn height(&self) -> f64 {
        self.top_left.distance_from(self.bottom_left)
    }

    /// The smallest axis-aligned rectangle containing all four corners.
    /// ```
    /// use syrp_geom::{Parallelogram, Point, Rect};
    /// let p = Parallelogram::new(Point::new(0, 0), Point::new(2, 0), Point::new(1, 1));
    /// assert_eq!(p.bounding_box(), Rect::new(0, 0, 3, 1));
    /// ```
    #[doc(alias = "getBoundingBox")]
    pub fn bounding_box(&self) -> Rect<T> {
        let c = [self.top_right, self.bottom_left, self.bottom_right()];
        let (mut l, mut t, mut r, mut b) = (
            self.top_left.x,
            self.top_left.y,
            self.top_left.x,
            self.top_left.y,
        );
        for p in c {
            (l, t) = (l.min_of(p.x), t.min_of(p.y));
            (r, b) = (r.max_of(p.x), b.max_of(p.y));
        }
        Rect::from_ltrb(l, t, r, b)
    }

    /// Applies `t` to each of the three corners (integer corners round to nearest).
    /// ```
    /// use syrp_geom::{AffineTransform, Parallelogram, Point, Rect};
    /// let p = Parallelogram::from(Rect::new(0, 0, 2, 1)).transformed_by(&AffineTransform::scale(2.0, 2.0));
    /// assert_eq!(p.bottom_right(), Point::new(4, 2));
    /// ```
    #[doc(alias = "transformedBy")]
    pub fn transformed_by(&self, t: &AffineTransform) -> Self {
        Self::new(
            t.transform_point(self.top_left),
            t.transform_point(self.top_right),
            t.transform_point(self.bottom_left),
        )
    }
}

impl<T: Coord> From<Rect<T>> for Parallelogram<T> {
    /// The rectangle's top-left, top-right and bottom-left corners.
    fn from(r: Rect<T>) -> Self {
        Self::new(
            r.position(),
            Point::new(r.right(), r.y()),
            Point::new(r.x(), r.bottom()),
        )
    }
}

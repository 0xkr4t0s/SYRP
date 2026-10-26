//! Conversions to and from [`kurbo`] types (feature `kurbo`). All are exact, in `f64`.
//!
//! ```
//! use syrp_geom::{AffineTransform, Point, Rect};
//! let k: kurbo::Rect = Rect::new(1.0, 2.0, 3.0, 4.0).into();
//! assert_eq!(k, kurbo::Rect::new(1.0, 2.0, 4.0, 6.0));
//! let p: Point<f64> = kurbo::Point::new(1.0, 2.0).into();
//! let a: kurbo::Affine = AffineTransform::translation(5.0, 0.0).into();
//! assert_eq!(a * kurbo::Point::new(1.0, 2.0), kurbo::Point::new(6.0, 2.0));
//! assert_eq!(p, Point::new(1.0, 2.0));
//! ```

use crate::{AffineTransform, Line, Point, Rect};

impl From<Point<f64>> for kurbo::Point {
    fn from(p: Point<f64>) -> Self {
        kurbo::Point::new(p.x, p.y)
    }
}

impl From<kurbo::Point> for Point<f64> {
    fn from(p: kurbo::Point) -> Self {
        Point::new(p.x, p.y)
    }
}

impl From<Point<f64>> for kurbo::Vec2 {
    fn from(p: Point<f64>) -> Self {
        kurbo::Vec2::new(p.x, p.y)
    }
}

impl From<kurbo::Vec2> for Point<f64> {
    fn from(v: kurbo::Vec2) -> Self {
        Point::new(v.x, v.y)
    }
}

impl From<Rect<f64>> for kurbo::Rect {
    fn from(r: Rect<f64>) -> Self {
        kurbo::Rect::new(r.x(), r.y(), r.right(), r.bottom())
    }
}

/// A kurbo rectangle with `x1 < x0` or `y1 < y0` becomes empty in that axis, following
/// [`Rect::from_ltrb`]. Call `kurbo::Rect::abs` first to keep its area instead.
impl From<kurbo::Rect> for Rect<f64> {
    fn from(r: kurbo::Rect) -> Self {
        Rect::from_ltrb(r.x0, r.y0, r.x1, r.y1)
    }
}

impl From<Line<f64>> for kurbo::Line {
    fn from(l: Line<f64>) -> Self {
        kurbo::Line::new(kurbo::Point::from(l.start), kurbo::Point::from(l.end))
    }
}

impl From<kurbo::Line> for Line<f64> {
    fn from(l: kurbo::Line) -> Self {
        Line::new(l.p0.into(), l.p1.into())
    }
}

impl From<AffineTransform> for kurbo::Affine {
    fn from(t: AffineTransform) -> Self {
        // kurbo's coefficients are column-major: [a, b, c, d, e, f] maps (x, y) to
        // (a x + c y + e, b x + d y + f).
        kurbo::Affine::new([t.m00, t.m10, t.m01, t.m11, t.m02, t.m12])
    }
}

impl From<kurbo::Affine> for AffineTransform {
    fn from(a: kurbo::Affine) -> Self {
        let [c0, c1, c2, c3, c4, c5] = a.as_coeffs();
        AffineTransform::new(c0, c2, c4, c1, c3, c5)
    }
}

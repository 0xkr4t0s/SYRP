//! [`AffineTransform`]: a 2D affine transformation matrix.

use crate::{Coord, Point, Rect};

/// A 2D affine transform, stored as the top two rows of a 3x3 matrix in `f64`.
///
/// A point `(x, y)` maps to
/// `(m00 * x + m01 * y + m02, m10 * x + m11 * y + m12)`.
///
/// [`translated`](Self::translated) applies its translation *after* the existing transform,
/// like [`followed_by`](Self::followed_by).
/// ```
/// use syrp_geom::{AffineTransform, Point};
/// let t = AffineTransform::scale(2.0, 2.0).translated(1.0, 0.0);
/// assert_eq!(t.transform_point(Point::new(3.0, 4.0)), Point::new(7.0, 8.0));
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AffineTransform {
    /// Row 0, column 0: x contribution to x.
    pub m00: f64,
    /// Row 0, column 1: y contribution to x.
    pub m01: f64,
    /// Row 0, column 2: x offset.
    pub m02: f64,
    /// Row 1, column 0: x contribution to y.
    pub m10: f64,
    /// Row 1, column 1: y contribution to y.
    pub m11: f64,
    /// Row 1, column 2: y offset.
    pub m12: f64,
}

impl Default for AffineTransform {
    /// The identity transform.
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl AffineTransform {
    /// The transform that leaves every point where it is.
    /// ```
    /// use syrp_geom::AffineTransform;
    /// assert!(AffineTransform::IDENTITY.is_identity());
    /// ```
    #[doc(alias = "identity")]
    pub const IDENTITY: Self = Self::new(1.0, 0.0, 0.0, 0.0, 1.0, 0.0);

    /// Creates a transform from its six matrix entries, row by row.
    /// ```
    /// let t = syrp_geom::AffineTransform::new(1.0, 0.0, 5.0, 0.0, 1.0, 6.0);
    /// assert_eq!(t, syrp_geom::AffineTransform::translation(5.0, 6.0));
    /// ```
    pub const fn new(m00: f64, m01: f64, m02: f64, m10: f64, m11: f64, m12: f64) -> Self {
        Self {
            m00,
            m01,
            m02,
            m10,
            m11,
            m12,
        }
    }

    /// A translation by `(dx, dy)`.
    /// ```
    /// use syrp_geom::{AffineTransform, Point};
    /// let p = AffineTransform::translation(1.0, 2.0).transform_point(Point::new(0.0, 0.0));
    /// assert_eq!(p, Point::new(1.0, 2.0));
    /// ```
    pub const fn translation(dx: f64, dy: f64) -> Self {
        Self::new(1.0, 0.0, dx, 0.0, 1.0, dy)
    }

    /// A rotation by `radians` about the origin. With y pointing down, a positive angle turns
    /// the x axis towards the y axis (clockwise on screen).
    /// ```
    /// use syrp_geom::{AffineTransform, Point};
    /// let p = AffineTransform::rotation(std::f64::consts::FRAC_PI_2).transform_point(Point::new(1.0_f64, 0.0));
    /// assert!(p.x.abs() < 1e-12 && (p.y - 1.0).abs() < 1e-12);
    /// ```
    pub fn rotation(radians: f64) -> Self {
        let (s, c) = radians.sin_cos();
        Self::new(c, -s, 0.0, s, c, 0.0)
    }

    /// A rotation by `radians` about the pivot `(px, py)`.
    /// ```
    /// use syrp_geom::{AffineTransform, Point};
    /// let t = AffineTransform::rotation_about(1.0, 5.0, 5.0);
    /// assert_eq!(t.transform_point(Point::new(5.0, 5.0)), Point::new(5.0, 5.0));
    /// ```
    pub fn rotation_about(radians: f64, px: f64, py: f64) -> Self {
        Self::translation(-px, -py)
            .followed_by(&Self::rotation(radians))
            .translated(px, py)
    }

    /// A scale by `sx` horizontally and `sy` vertically about the origin.
    /// ```
    /// use syrp_geom::{AffineTransform, Point};
    /// assert_eq!(AffineTransform::scale(2.0, 3.0).transform_point(Point::new(1.0, 1.0)), Point::new(2.0, 3.0));
    /// ```
    pub const fn scale(sx: f64, sy: f64) -> Self {
        Self::new(sx, 0.0, 0.0, 0.0, sy, 0.0)
    }

    /// A shear about the origin: `x' = x + shear_x * y` and `y' = shear_y * x + y`.
    /// ```
    /// use syrp_geom::{AffineTransform, Point};
    /// assert_eq!(AffineTransform::shear(2.0, 0.0).transform_point(Point::new(1.0, 1.0)), Point::new(3.0, 1.0));
    /// ```
    pub const fn shear(shear_x: f64, shear_y: f64) -> Self {
        Self::new(1.0, shear_x, 0.0, shear_y, 1.0, 0.0)
    }

    /// The transform that applies `self` first and then `other`.
    /// ```
    /// use syrp_geom::{AffineTransform, Point};
    /// let t = AffineTransform::translation(1.0, 0.0).followed_by(&AffineTransform::scale(10.0, 10.0));
    /// assert_eq!(t.transform_point(Point::new(0.0, 0.0)), Point::new(10.0, 0.0));
    /// ```
    #[doc(alias = "followedBy")]
    pub fn followed_by(&self, other: &Self) -> Self {
        let (a, b) = (self, other);
        Self::new(
            b.m00 * a.m00 + b.m01 * a.m10,
            b.m00 * a.m01 + b.m01 * a.m11,
            b.m00 * a.m02 + b.m01 * a.m12 + b.m02,
            b.m10 * a.m00 + b.m11 * a.m10,
            b.m10 * a.m01 + b.m11 * a.m11,
            b.m10 * a.m02 + b.m11 * a.m12 + b.m12,
        )
    }

    /// This transform followed by a translation.
    /// ```
    /// use syrp_geom::AffineTransform;
    /// assert_eq!(AffineTransform::IDENTITY.translated(3.0, 4.0), AffineTransform::translation(3.0, 4.0));
    /// ```
    pub fn translated(&self, dx: f64, dy: f64) -> Self {
        Self {
            m02: self.m02 + dx,
            m12: self.m12 + dy,
            ..*self
        }
    }

    /// Determinant of the 2x2 linear part.
    /// ```
    /// assert_eq!(syrp_geom::AffineTransform::scale(2.0, 3.0).determinant(), 6.0);
    /// ```
    #[doc(alias = "getDeterminant")]
    pub fn determinant(&self) -> f64 {
        self.m00 * self.m11 - self.m01 * self.m10
    }

    /// True if the transform cannot be inverted: its determinant is zero or not finite.
    /// ```
    /// assert!(syrp_geom::AffineTransform::scale(0.0, 1.0).is_singular());
    /// ```
    #[doc(alias = "isSingularity")]
    pub fn is_singular(&self) -> bool {
        let d = self.determinant();
        d == 0.0 || !d.is_finite()
    }

    /// The inverse transform, or `None` if this one [is singular](Self::is_singular) or the
    /// inverse would not be finite (a determinant so small that dividing by it overflows).
    /// ```
    /// use syrp_geom::{AffineTransform, Point};
    /// let t = AffineTransform::scale(2.0, 4.0).translated(1.0, 1.0);
    /// let p = t.inverted().unwrap().transform_point(Point::new(3.0, 5.0));
    /// assert_eq!(p, Point::new(1.0, 1.0));
    /// assert!(AffineTransform::scale(0.0, 0.0).inverted().is_none());
    /// ```
    pub fn inverted(&self) -> Option<Self> {
        if self.is_singular() {
            return None;
        }
        let d = self.determinant();
        let (i00, i01) = (self.m11 / d, -self.m01 / d);
        let (i10, i11) = (-self.m10 / d, self.m00 / d);
        let inv = Self::new(
            i00,
            i01,
            -(i00 * self.m02 + i01 * self.m12),
            i10,
            i11,
            -(i10 * self.m02 + i11 * self.m12),
        );
        [inv.m00, inv.m01, inv.m02, inv.m10, inv.m11, inv.m12]
            .iter()
            .all(|v| v.is_finite())
            .then_some(inv)
    }

    /// True if this is exactly the identity.
    /// ```
    /// assert!(!syrp_geom::AffineTransform::translation(1.0, 0.0).is_identity());
    /// ```
    #[doc(alias = "isIdentity")]
    pub fn is_identity(&self) -> bool {
        *self == Self::IDENTITY
    }

    /// Transforms `(x, y)` in `f64`.
    fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        (
            self.m00 * x + self.m01 * y + self.m02,
            self.m10 * x + self.m11 * y + self.m12,
        )
    }

    /// Transforms a point. The maths is done in `f64`; integer results round to nearest, with
    /// halves rounded away from zero (2.5 becomes 3, -2.5 becomes -3).
    /// ```
    /// use syrp_geom::{AffineTransform, Point};
    /// assert_eq!(AffineTransform::scale(1.5, 1.5).transform_point(Point::new(1, 1)), Point::new(2, 2));
    /// ```
    #[doc(alias = "transformPoint")]
    pub fn transform_point<T: Coord>(&self, p: Point<T>) -> Point<T> {
        let (x, y) = self.apply(p.x.to_f64(), p.y.to_f64());
        Point::new(T::from_f64(x), T::from_f64(y))
    }

    /// The axis-aligned bounding box of the transformed rectangle's four corners.
    /// Integer results are widened outwards (floor/ceil) so they still enclose the shape.
    /// ```
    /// use syrp_geom::{AffineTransform, Rect};
    /// let r = AffineTransform::rotation(std::f64::consts::FRAC_PI_2).transform_rect(Rect::new(0, 0, 10, 5));
    /// assert_eq!(r, Rect::new(-5, 0, 5, 10));
    /// ```
    #[doc(alias = "transformedBy")]
    pub fn transform_rect<T: Coord>(&self, r: Rect<T>) -> Rect<T> {
        let (l, t) = (r.x().to_f64(), r.y().to_f64());
        let (rt, b) = (r.right().to_f64(), r.bottom().to_f64());
        let corners = [
            self.apply(l, t),
            self.apply(rt, t),
            self.apply(l, b),
            self.apply(rt, b),
        ];
        let (mut x0, mut y0) = corners[0];
        let (mut x1, mut y1) = corners[0];
        for &(x, y) in &corners[1..] {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        // For integer rects, snap values within rounding noise of an integer before taking
        // floor/ceil, so that an exact 90-degree rotation does not grow the rect by a pixel.
        // Float rects keep the exact bounds.
        let snap = |v: f64| {
            if T::INTEGER && (v - v.round()).abs() < 1e-9 {
                v.round()
            } else {
                v
            }
        };
        Rect::from_ltrb(
            T::floor_from_f64(snap(x0)),
            T::floor_from_f64(snap(y0)),
            T::ceil_from_f64(snap(x1)),
            T::ceil_from_f64(snap(y1)),
        )
    }
}

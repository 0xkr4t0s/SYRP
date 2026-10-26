//! [`Rect`]: an axis-aligned rectangle stored as position plus size.

use crate::{Coord, Point};

/// Clamps a size to be non-negative; NaN also becomes zero.
fn non_negative<T: Coord>(v: T) -> T {
    if v > T::ZERO { v } else { T::ZERO }
}

/// An axis-aligned rectangle: a top-left position plus a width and height.
///
/// Invariant: width and height are never negative. Every constructor and builder clamps a
/// negative (or NaN) size to zero, so a rectangle given a negative size is simply empty.
/// The left and top edges are inside the rectangle, the right and bottom edges are not.
/// ```
/// use syrp_geom::{Point, Rect};
/// let r = Rect::new(10, 10, 100, 50);
/// assert_eq!(r.right(), 110);
/// assert!(r.contains_point(Point::new(10, 10)));
/// assert!(!r.contains_point(Point::new(110, 10)));
/// assert!(Rect::new(0, 0, -5, 10).is_empty());
/// ```
#[doc(alias = "Rectangle")]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect<T> {
    x: T,
    y: T,
    w: T,
    h: T,
}

impl<T: Coord> Rect<T> {
    /// Creates a rectangle; a negative width or height becomes zero.
    /// ```
    /// let r = syrp_geom::Rect::new(1.0, 2.0, -3.0, 4.0);
    /// assert_eq!((r.width(), r.height()), (0.0, 4.0));
    /// ```
    pub fn new(x: T, y: T, width: T, height: T) -> Self {
        Self {
            x,
            y,
            w: non_negative(width),
            h: non_negative(height),
        }
    }

    /// Creates a rectangle from its edges; if right < left or bottom < top that size is zero.
    /// ```
    /// assert_eq!(syrp_geom::Rect::from_ltrb(1, 2, 5, 9), syrp_geom::Rect::new(1, 2, 4, 7));
    /// ```
    #[doc(alias = "leftTopRightBottom")]
    pub fn from_ltrb(left: T, top: T, right: T, bottom: T) -> Self {
        Self::new(left, top, right - left, bottom - top)
    }

    /// Left edge.
    /// ```
    /// assert_eq!(syrp_geom::Rect::new(1, 2, 3, 4).x(), 1);
    /// ```
    #[doc(alias = "getX")]
    pub fn x(&self) -> T {
        self.x
    }

    /// Top edge.
    /// ```
    /// assert_eq!(syrp_geom::Rect::new(1, 2, 3, 4).y(), 2);
    /// ```
    #[doc(alias = "getY")]
    pub fn y(&self) -> T {
        self.y
    }

    /// Width (never negative).
    /// ```
    /// assert_eq!(syrp_geom::Rect::new(1, 2, 3, 4).width(), 3);
    /// ```
    #[doc(alias = "getWidth")]
    pub fn width(&self) -> T {
        self.w
    }

    /// Height (never negative).
    /// ```
    /// assert_eq!(syrp_geom::Rect::new(1, 2, 3, 4).height(), 4);
    /// ```
    #[doc(alias = "getHeight")]
    pub fn height(&self) -> T {
        self.h
    }

    /// Right edge, `x + width` (outside the rectangle).
    /// ```
    /// assert_eq!(syrp_geom::Rect::new(1, 2, 3, 4).right(), 4);
    /// ```
    #[doc(alias = "getRight")]
    pub fn right(&self) -> T {
        self.x + self.w
    }

    /// Bottom edge, `y + height` (outside the rectangle).
    /// ```
    /// assert_eq!(syrp_geom::Rect::new(1, 2, 3, 4).bottom(), 6);
    /// ```
    #[doc(alias = "getBottom")]
    pub fn bottom(&self) -> T {
        self.y + self.h
    }

    /// Top-left corner.
    /// ```
    /// assert_eq!(syrp_geom::Rect::new(1, 2, 3, 4).position(), syrp_geom::Point::new(1, 2));
    /// ```
    #[doc(alias = "getPosition")]
    #[doc(alias = "getTopLeft")]
    pub fn position(&self) -> Point<T> {
        Point::new(self.x, self.y)
    }

    /// Centre point; integer rectangles round the half-size towards zero.
    /// ```
    /// assert_eq!(syrp_geom::Rect::new(0, 0, 5, 4).centre(), syrp_geom::Point::new(2, 2));
    /// ```
    #[doc(alias = "getCentre")]
    pub fn centre(&self) -> Point<T> {
        Point::new(self.x + self.w.half(), self.y + self.h.half())
    }

    /// True if the width or height is zero.
    /// ```
    /// assert!(syrp_geom::Rect::new(3, 3, 0, 10).is_empty());
    /// ```
    #[doc(alias = "isEmpty")]
    pub fn is_empty(&self) -> bool {
        self.w <= T::ZERO || self.h <= T::ZERO
    }

    /// Moves the rectangle so its left edge is `x`, keeping its size.
    /// ```
    /// assert_eq!(syrp_geom::Rect::new(1, 2, 3, 4).with_x(9), syrp_geom::Rect::new(9, 2, 3, 4));
    /// ```
    #[doc(alias = "withX")]
    pub fn with_x(self, x: T) -> Self {
        Self { x, ..self }
    }

    /// Moves the rectangle so its top edge is `y`, keeping its size.
    /// ```
    /// assert_eq!(syrp_geom::Rect::new(1, 2, 3, 4).with_y(9), syrp_geom::Rect::new(1, 9, 3, 4));
    /// ```
    #[doc(alias = "withY")]
    pub fn with_y(self, y: T) -> Self {
        Self { y, ..self }
    }

    /// Changes the width, keeping the position.
    /// ```
    /// assert_eq!(syrp_geom::Rect::new(1, 2, 3, 4).with_width(7).width(), 7);
    /// ```
    #[doc(alias = "withWidth")]
    pub fn with_width(self, width: T) -> Self {
        Self::new(self.x, self.y, width, self.h)
    }

    /// Changes the height, keeping the position.
    /// ```
    /// assert_eq!(syrp_geom::Rect::new(1, 2, 3, 4).with_height(7).height(), 7);
    /// ```
    #[doc(alias = "withHeight")]
    pub fn with_height(self, height: T) -> Self {
        Self::new(self.x, self.y, self.w, height)
    }

    /// Changes the size, keeping the position.
    /// ```
    /// assert_eq!(syrp_geom::Rect::new(1, 2, 3, 4).with_size(5, 6), syrp_geom::Rect::new(1, 2, 5, 6));
    /// ```
    #[doc(alias = "withSize")]
    pub fn with_size(self, width: T, height: T) -> Self {
        Self::new(self.x, self.y, width, height)
    }

    /// Moves the rectangle so its centre is `c`, keeping the size.
    /// ```
    /// use syrp_geom::{Point, Rect};
    /// assert_eq!(Rect::new(0, 0, 4, 2).with_centre(Point::new(10, 10)), Rect::new(8, 9, 4, 2));
    /// ```
    #[doc(alias = "withCentre")]
    pub fn with_centre(self, c: Point<T>) -> Self {
        Self {
            x: c.x - self.w.half(),
            y: c.y - self.h.half(),
            ..self
        }
    }

    /// Moves the left edge to `left`, keeping the right edge; the width clamps at zero.
    /// ```
    /// assert_eq!(syrp_geom::Rect::new(0, 0, 10, 5).with_left(4), syrp_geom::Rect::new(4, 0, 6, 5));
    /// ```
    #[doc(alias = "withLeft")]
    pub fn with_left(self, left: T) -> Self {
        Self::from_ltrb(left, self.y, self.right(), self.bottom())
    }

    /// Moves the right edge to `right`, keeping the left edge; the width clamps at zero.
    /// ```
    /// assert_eq!(syrp_geom::Rect::new(0, 0, 10, 5).with_right(4), syrp_geom::Rect::new(0, 0, 4, 5));
    /// ```
    #[doc(alias = "withRight")]
    pub fn with_right(self, right: T) -> Self {
        Self::from_ltrb(self.x, self.y, right, self.bottom())
    }

    /// Moves the top edge to `top`, keeping the bottom edge; the height clamps at zero.
    /// ```
    /// assert_eq!(syrp_geom::Rect::new(0, 0, 5, 10).with_top(4), syrp_geom::Rect::new(0, 4, 5, 6));
    /// ```
    #[doc(alias = "withTop")]
    pub fn with_top(self, top: T) -> Self {
        Self::from_ltrb(self.x, top, self.right(), self.bottom())
    }

    /// Moves the bottom edge to `bottom`, keeping the top edge; the height clamps at zero.
    /// ```
    /// assert_eq!(syrp_geom::Rect::new(0, 0, 5, 10).with_bottom(4), syrp_geom::Rect::new(0, 0, 5, 4));
    /// ```
    #[doc(alias = "withBottom")]
    pub fn with_bottom(self, bottom: T) -> Self {
        Self::from_ltrb(self.x, self.y, self.right(), bottom)
    }

    /// Moves the rectangle by `(dx, dy)`.
    /// ```
    /// assert_eq!(syrp_geom::Rect::new(1, 1, 2, 2).translated(3, -1), syrp_geom::Rect::new(4, 0, 2, 2));
    /// ```
    pub fn translated(self, dx: T, dy: T) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
            ..self
        }
    }

    /// True if `p` is inside: left and top edges included, right and bottom excluded.
    /// ```
    /// use syrp_geom::{Point, Rect};
    /// let r = Rect::new(0, 0, 10, 10);
    /// assert!(r.contains_point(Point::new(0, 9)));
    /// assert!(!r.contains_point(Point::new(0, 10)));
    /// ```
    #[doc(alias = "contains")]
    pub fn contains_point(&self, p: Point<T>) -> bool {
        p.x >= self.x && p.y >= self.y && p.x < self.right() && p.y < self.bottom()
    }

    /// True if `other` lies entirely within this rectangle (shared edges allowed).
    /// ```
    /// use syrp_geom::Rect;
    /// assert!(Rect::new(0, 0, 10, 10).contains(&Rect::new(5, 5, 5, 5)));
    /// assert!(!Rect::new(0, 0, 10, 10).contains(&Rect::new(5, 5, 6, 5)));
    /// ```
    pub fn contains(&self, other: &Self) -> bool {
        other.x >= self.x
            && other.y >= self.y
            && other.right() <= self.right()
            && other.bottom() <= self.bottom()
    }

    /// True if the two rectangles overlap with a non-zero area. Touching edges do not count.
    /// ```
    /// use syrp_geom::Rect;
    /// assert!(Rect::new(0, 0, 10, 10).intersects(&Rect::new(9, 9, 5, 5)));
    /// assert!(!Rect::new(0, 0, 10, 10).intersects(&Rect::new(10, 0, 5, 5)));
    /// ```
    pub fn intersects(&self, other: &Self) -> bool {
        !self.intersection(other).is_empty()
    }

    /// The overlapping region, or `Rect::default()` (all zero) if they do not overlap.
    /// ```
    /// use syrp_geom::Rect;
    /// let a = Rect::new(0, 0, 10, 10);
    /// assert_eq!(a.intersection(&Rect::new(5, 5, 10, 10)), Rect::new(5, 5, 5, 5));
    /// assert_eq!(a.intersection(&Rect::new(20, 0, 1, 1)), Rect::default());
    /// ```
    #[doc(alias = "getIntersection")]
    pub fn intersection(&self, other: &Self) -> Self {
        let l = self.x.max_of(other.x);
        let t = self.y.max_of(other.y);
        let r = self.right().min_of(other.right());
        let b = self.bottom().min_of(other.bottom());
        if r > l && b > t {
            Self::from_ltrb(l, t, r, b)
        } else {
            Self::default()
        }
    }

    /// The smallest rectangle containing both. An empty rectangle does not contribute, so
    /// if one is empty the other is returned (`other` if both are empty).
    /// ```
    /// use syrp_geom::Rect;
    /// let u = Rect::new(0, 0, 2, 2).union(&Rect::new(5, 5, 1, 1));
    /// assert_eq!(u, Rect::new(0, 0, 6, 6));
    /// assert_eq!(Rect::new(9, 9, 0, 0).union(&Rect::new(1, 1, 1, 1)), Rect::new(1, 1, 1, 1));
    /// ```
    #[doc(alias = "getUnion")]
    pub fn union(&self, other: &Self) -> Self {
        if self.is_empty() {
            return *other;
        }
        if other.is_empty() {
            return *self;
        }
        Self::from_ltrb(
            self.x.min_of(other.x),
            self.y.min_of(other.y),
            self.right().max_of(other.right()),
            self.bottom().max_of(other.bottom()),
        )
    }

    /// Moves each left/right edge inwards by `dx` and each top/bottom edge by `dy`.
    /// The size clamps at zero.
    /// ```
    /// assert_eq!(syrp_geom::Rect::new(0, 0, 10, 10).reduced(2, 1), syrp_geom::Rect::new(2, 1, 6, 8));
    /// ```
    pub fn reduced(self, dx: T, dy: T) -> Self {
        Self::new(self.x + dx, self.y + dy, self.w - dx - dx, self.h - dy - dy)
    }

    /// Moves each left/right edge outwards by `dx` and each top/bottom edge by `dy`.
    /// The size clamps at zero (for negative amounts).
    /// ```
    /// assert_eq!(syrp_geom::Rect::new(2, 2, 2, 2).expanded(1, 2), syrp_geom::Rect::new(1, 0, 4, 6));
    /// ```
    pub fn expanded(self, dx: T, dy: T) -> Self {
        self.reduced(-dx, -dy)
    }

    /// Clamps a slice amount to `0..=limit`.
    fn clamp_amount(amount: T, limit: T) -> T {
        non_negative(amount).min_of(limit)
    }

    /// Removes a strip of height `amount` from the top and returns it.
    /// The amount is clamped to `0..=height`.
    /// ```
    /// use syrp_geom::Rect;
    /// let mut r = Rect::new(100, 100, 300, 300);
    /// assert_eq!(r.take_top(50), Rect::new(100, 100, 300, 50));
    /// assert_eq!(r, Rect::new(100, 150, 300, 250));
    /// ```
    #[doc(alias = "removeFromTop")]
    pub fn take_top(&mut self, amount: T) -> Self {
        let a = Self::clamp_amount(amount, self.h);
        let taken = Self { h: a, ..*self };
        *self = Self {
            y: self.y + a,
            h: self.h - a,
            ..*self
        };
        taken
    }

    /// Removes a strip of height `amount` from the bottom and returns it.
    /// The amount is clamped to `0..=height`.
    /// ```
    /// use syrp_geom::Rect;
    /// let mut r = Rect::new(0, 0, 10, 10);
    /// assert_eq!(r.take_bottom(3), Rect::new(0, 7, 10, 3));
    /// assert_eq!(r, Rect::new(0, 0, 10, 7));
    /// ```
    #[doc(alias = "removeFromBottom")]
    pub fn take_bottom(&mut self, amount: T) -> Self {
        let a = Self::clamp_amount(amount, self.h);
        let rest = self.h - a;
        let taken = Self {
            y: self.y + rest,
            h: a,
            ..*self
        };
        self.h = rest;
        taken
    }

    /// Removes a strip of width `amount` from the left and returns it.
    /// The amount is clamped to `0..=width`.
    /// ```
    /// use syrp_geom::Rect;
    /// let mut r = Rect::new(0, 0, 10, 10);
    /// assert_eq!(r.take_left(4), Rect::new(0, 0, 4, 10));
    /// assert_eq!(r, Rect::new(4, 0, 6, 10));
    /// ```
    #[doc(alias = "removeFromLeft")]
    pub fn take_left(&mut self, amount: T) -> Self {
        let a = Self::clamp_amount(amount, self.w);
        let taken = Self { w: a, ..*self };
        *self = Self {
            x: self.x + a,
            w: self.w - a,
            ..*self
        };
        taken
    }

    /// Removes a strip of width `amount` from the right and returns it.
    /// The amount is clamped to `0..=width`.
    /// ```
    /// use syrp_geom::Rect;
    /// let mut r = Rect::new(0, 0, 10, 10);
    /// assert_eq!(r.take_right(4), Rect::new(6, 0, 4, 10));
    /// assert_eq!(r, Rect::new(0, 0, 6, 10));
    /// ```
    #[doc(alias = "removeFromRight")]
    pub fn take_right(&mut self, amount: T) -> Self {
        let a = Self::clamp_amount(amount, self.w);
        let rest = self.w - a;
        let taken = Self {
            x: self.x + rest,
            w: a,
            ..*self
        };
        self.w = rest;
        taken
    }

    /// Converts to another coordinate type, converting x, y, width and height independently
    /// (rounding to nearest, halves away from zero, when the target is an integer).
    /// ```
    /// let r: syrp_geom::Rect<f32> = syrp_geom::Rect::new(1, 2, 3, 4).cast();
    /// assert_eq!(r, syrp_geom::Rect::new(1.0, 2.0, 3.0, 4.0));
    /// ```
    pub fn cast<U: Coord>(self) -> Rect<U> {
        let c = |v: T| U::from_f64(v.to_f64());
        Rect::new(c(self.x), c(self.y), c(self.w), c(self.h))
    }

    /// Rounds x, y, width and height each to the nearest integer, halves away from zero.
    /// Values outside the `i32` range saturate and NaN becomes 0. Because the size is rounded
    /// separately from the position, the right edge may differ from `right().round()` by one.
    /// ```
    /// let r = syrp_geom::Rect::new(0.5_f64, 1.4, 2.5, -0.2).to_nearest_int();
    /// assert_eq!(r, syrp_geom::Rect::new(1, 1, 3, 0));
    /// ```
    #[doc(alias = "toNearestInt")]
    pub fn to_nearest_int(self) -> Rect<i32> {
        self.cast()
    }
}

//! [`Point`]: a 2D position or offset.

use crate::Coord;
use core::ops::{Add, Mul, Neg, Sub};

/// A 2D point (or vector) with coordinates of type `T`.
///
/// The y axis points down, as in screen coordinates.
/// ```
/// use syrp_geom::Point;
/// let p = Point::new(3, 4) + Point::new(1, 1);
/// assert_eq!(p, Point::new(4, 5));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point<T> {
    /// Horizontal coordinate.
    pub x: T,
    /// Vertical coordinate (grows downwards).
    pub y: T,
}

impl<T: Coord> Point<T> {
    /// Creates a point.
    /// ```
    /// let p = syrp_geom::Point::new(1.5_f32, 2.0);
    /// assert_eq!(p.x, 1.5);
    /// ```
    pub const fn new(x: T, y: T) -> Self {
        Self { x, y }
    }

    /// The point (0, 0).
    /// ```
    /// assert_eq!(syrp_geom::Point::<i32>::origin(), syrp_geom::Point::new(0, 0));
    /// ```
    pub fn origin() -> Self {
        Self::new(T::ZERO, T::ZERO)
    }

    /// Returns the point moved by `(dx, dy)`.
    /// ```
    /// assert_eq!(syrp_geom::Point::new(1, 2).translated(1, -1), syrp_geom::Point::new(2, 1));
    /// ```
    pub fn translated(self, dx: T, dy: T) -> Self {
        Self::new(self.x + dx, self.y + dy)
    }

    /// Euclidean distance to `other`, computed in `f64`.
    /// ```
    /// assert_eq!(syrp_geom::Point::new(0, 0).distance_from(syrp_geom::Point::new(3, 4)), 5.0);
    /// ```
    #[doc(alias = "getDistanceFrom")]
    pub fn distance_from(self, other: Self) -> f64 {
        let dx = (other.x - self.x).to_f64();
        let dy = (other.y - self.y).to_f64();
        dx.hypot(dy)
    }

    /// Converts each coordinate to another [`Coord`] type (rounding to nearest for integers).
    /// ```
    /// let p: syrp_geom::Point<i32> = syrp_geom::Point::new(1.6_f64, -1.4).cast();
    /// assert_eq!(p, syrp_geom::Point::new(2, -1));
    /// ```
    pub fn cast<U: Coord>(self) -> Point<U> {
        Point::new(U::from_f64(self.x.to_f64()), U::from_f64(self.y.to_f64()))
    }

    /// Rounds each coordinate to the nearest integer (halves away from zero).
    /// ```
    /// assert_eq!(syrp_geom::Point::new(2.5_f32, 0.4).to_nearest_int(), syrp_geom::Point::new(3, 0));
    /// ```
    #[doc(alias = "roundToInt")]
    pub fn to_nearest_int(self) -> Point<i32> {
        self.cast()
    }
}

impl<T: Coord> Add for Point<T> {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.x + o.x, self.y + o.y)
    }
}

impl<T: Coord> Sub for Point<T> {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y)
    }
}

impl<T: Coord> Neg for Point<T> {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y)
    }
}

/// Scales both coordinates by a scalar.
impl<T: Coord> Mul<T> for Point<T> {
    type Output = Self;
    fn mul(self, k: T) -> Self {
        Self::new(self.x * k, self.y * k)
    }
}

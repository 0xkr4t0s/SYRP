//! [`BorderSize`]: the thickness of the four edges around a rectangle.

use crate::{Coord, Rect};

/// Thicknesses for the top, left, bottom and right edges of a rectangle (a margin or inset).
/// ```
/// use syrp_geom::{BorderSize, Rect};
/// let b = BorderSize::new(1, 2, 3, 4);
/// assert_eq!(b.subtracted_from(Rect::new(0, 0, 20, 20)), Rect::new(2, 1, 14, 16));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BorderSize<T> {
    /// Thickness of the top edge.
    pub top: T,
    /// Thickness of the left edge.
    pub left: T,
    /// Thickness of the bottom edge.
    pub bottom: T,
    /// Thickness of the right edge.
    pub right: T,
}

impl<T: Coord> BorderSize<T> {
    /// Creates a border, in the order top, left, bottom, right.
    /// ```
    /// assert_eq!(syrp_geom::BorderSize::new(1, 2, 3, 4).bottom, 3);
    /// ```
    pub const fn new(top: T, left: T, bottom: T, right: T) -> Self {
        Self {
            top,
            left,
            bottom,
            right,
        }
    }

    /// A border with the same thickness on every edge.
    /// ```
    /// assert_eq!(syrp_geom::BorderSize::uniform(2), syrp_geom::BorderSize::new(2, 2, 2, 2));
    /// ```
    pub const fn uniform(thickness: T) -> Self {
        Self::new(thickness, thickness, thickness, thickness)
    }

    /// `top + bottom`.
    /// ```
    /// assert_eq!(syrp_geom::BorderSize::new(1, 2, 3, 4).top_and_bottom(), 4);
    /// ```
    #[doc(alias = "getTopAndBottom")]
    pub fn top_and_bottom(&self) -> T {
        self.top + self.bottom
    }

    /// `left + right`.
    /// ```
    /// assert_eq!(syrp_geom::BorderSize::new(1, 2, 3, 4).left_and_right(), 6);
    /// ```
    #[doc(alias = "getLeftAndRight")]
    pub fn left_and_right(&self) -> T {
        self.left + self.right
    }

    /// True if every edge is zero.
    /// ```
    /// assert!(syrp_geom::BorderSize::<f32>::default().is_empty());
    /// ```
    #[doc(alias = "isEmpty")]
    pub fn is_empty(&self) -> bool {
        self.top == T::ZERO
            && self.left == T::ZERO
            && self.bottom == T::ZERO
            && self.right == T::ZERO
    }

    /// Shrinks `r` by the border (each edge moves inwards). The size clamps at zero.
    /// ```
    /// use syrp_geom::{BorderSize, Rect};
    /// assert_eq!(BorderSize::uniform(1).subtracted_from(Rect::new(0, 0, 4, 4)), Rect::new(1, 1, 2, 2));
    /// ```
    #[doc(alias = "subtractedFrom")]
    pub fn subtracted_from(&self, r: Rect<T>) -> Rect<T> {
        Rect::from_ltrb(
            r.x() + self.left,
            r.y() + self.top,
            r.right() - self.right,
            r.bottom() - self.bottom,
        )
    }

    /// Grows `r` by the border (each edge moves outwards).
    /// ```
    /// use syrp_geom::{BorderSize, Rect};
    /// assert_eq!(BorderSize::uniform(1).added_to(Rect::new(1, 1, 2, 2)), Rect::new(0, 0, 4, 4));
    /// ```
    #[doc(alias = "addedTo")]
    pub fn added_to(&self, r: Rect<T>) -> Rect<T> {
        Rect::from_ltrb(
            r.x() - self.left,
            r.y() - self.top,
            r.right() + self.right,
            r.bottom() + self.bottom,
        )
    }
}

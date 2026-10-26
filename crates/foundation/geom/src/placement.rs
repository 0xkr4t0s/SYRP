//! [`Justification`] and [`RectanglePlacement`]: how one rectangle is positioned in another.

use crate::{AffineTransform, Coord, Rect};
use core::ops::BitOr;

/// Alignment flags: one horizontal and one vertical choice, combined with `|`.
///
/// Horizontally, [`HORIZONTALLY_CENTRED`](Self::HORIZONTALLY_CENTRED) wins over
/// [`RIGHT`](Self::RIGHT), and anything else (including no flag) means left. Vertically,
/// [`VERTICALLY_CENTRED`](Self::VERTICALLY_CENTRED) wins over [`BOTTOM`](Self::BOTTOM), and
/// anything else means top. [`HORIZONTALLY_JUSTIFIED`](Self::HORIZONTALLY_JUSTIFIED) only
/// matters to text layout; for rectangles it behaves like left.
/// ```
/// use syrp_geom::{Justification as J, Rect};
/// let r = (J::RIGHT | J::BOTTOM).applied_to_rect(Rect::new(0, 0, 10, 10), Rect::new(0, 0, 100, 50));
/// assert_eq!(r, Rect::new(90, 40, 10, 10));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Justification(u32);

impl Justification {
    /// Align to the left edge.
    pub const LEFT: Self = Self(1 << 0);
    /// Align to the right edge.
    pub const RIGHT: Self = Self(1 << 1);
    /// Centre horizontally.
    pub const HORIZONTALLY_CENTRED: Self = Self(1 << 2);
    /// Align to the top edge.
    pub const TOP: Self = Self(1 << 3);
    /// Align to the bottom edge.
    pub const BOTTOM: Self = Self(1 << 4);
    /// Centre vertically.
    pub const VERTICALLY_CENTRED: Self = Self(1 << 5);
    /// Stretch text lines to the full width (text layout only).
    pub const HORIZONTALLY_JUSTIFIED: Self = Self(1 << 6);
    /// Centred both ways.
    pub const CENTRED: Self = Self(Self::HORIZONTALLY_CENTRED.0 | Self::VERTICALLY_CENTRED.0);
    /// Left and vertically centred.
    pub const CENTRED_LEFT: Self = Self(Self::LEFT.0 | Self::VERTICALLY_CENTRED.0);
    /// Right and vertically centred.
    pub const CENTRED_RIGHT: Self = Self(Self::RIGHT.0 | Self::VERTICALLY_CENTRED.0);
    /// Top and horizontally centred.
    pub const CENTRED_TOP: Self = Self(Self::TOP.0 | Self::HORIZONTALLY_CENTRED.0);
    /// Bottom and horizontally centred.
    pub const CENTRED_BOTTOM: Self = Self(Self::BOTTOM.0 | Self::HORIZONTALLY_CENTRED.0);
    /// Top-left corner.
    pub const TOP_LEFT: Self = Self(Self::TOP.0 | Self::LEFT.0);
    /// Top-right corner.
    pub const TOP_RIGHT: Self = Self(Self::TOP.0 | Self::RIGHT.0);
    /// Bottom-left corner.
    pub const BOTTOM_LEFT: Self = Self(Self::BOTTOM.0 | Self::LEFT.0);
    /// Bottom-right corner.
    pub const BOTTOM_RIGHT: Self = Self(Self::BOTTOM.0 | Self::RIGHT.0);

    const H_MASK: u32 = Self::LEFT.0
        | Self::RIGHT.0
        | Self::HORIZONTALLY_CENTRED.0
        | Self::HORIZONTALLY_JUSTIFIED.0;
    const V_MASK: u32 = Self::TOP.0 | Self::BOTTOM.0 | Self::VERTICALLY_CENTRED.0;

    /// The raw flag bits (this crate's own values, not JUCE's).
    /// ```
    /// use syrp_geom::Justification as J;
    /// assert_eq!(J::from_bits(J::CENTRED.bits()), J::CENTRED);
    /// ```
    #[doc(alias = "getFlags")]
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// Rebuilds flags from [`bits`](Self::bits); unknown bits are dropped.
    /// ```
    /// assert_eq!(syrp_geom::Justification::from_bits(1 << 31).bits(), 0);
    /// ```
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits & (Self::H_MASK | Self::V_MASK))
    }

    /// True if every flag in `flags` is set.
    /// ```
    /// use syrp_geom::Justification as J;
    /// assert!(J::TOP_LEFT.contains(J::LEFT) && !J::TOP_LEFT.contains(J::CENTRED));
    /// ```
    #[doc(alias = "testFlags")]
    pub const fn contains(self, flags: Self) -> bool {
        self.0 & flags.0 == flags.0
    }

    /// Only the horizontal flags.
    /// ```
    /// use syrp_geom::Justification as J;
    /// assert_eq!(J::CENTRED_LEFT.only_horizontal(), J::LEFT);
    /// ```
    #[doc(alias = "getOnlyHorizontalFlags")]
    pub const fn only_horizontal(self) -> Self {
        Self(self.0 & Self::H_MASK)
    }

    /// Only the vertical flags.
    /// ```
    /// use syrp_geom::Justification as J;
    /// assert_eq!(J::CENTRED_LEFT.only_vertical(), J::VERTICALLY_CENTRED);
    /// ```
    #[doc(alias = "getOnlyVerticalFlags")]
    pub const fn only_vertical(self) -> Self {
        Self(self.0 & Self::V_MASK)
    }

    /// Moves `item` (keeping its size) to its justified position inside `area`. The item may
    /// be larger than the area and then overhangs it. Centring offsets are
    /// `(area size - item size) / 2`; for `i32` the division truncates towards zero, so an odd
    /// spare pixel goes on the far side, while floats centre exactly.
    /// ```
    /// use syrp_geom::{Justification, Rect};
    /// let area = Rect::new(0, 0, 15, 15);
    /// assert_eq!(Justification::CENTRED.applied_to_rect(Rect::new(0, 0, 10, 10), area), Rect::new(2, 2, 10, 10));
    /// let areaf = Rect::new(0.0, 0.0, 15.0, 15.0);
    /// assert_eq!(Justification::CENTRED.applied_to_rect(Rect::new(0.0, 0.0, 10.0, 10.0), areaf).x(), 2.5);
    /// ```
    #[doc(alias = "appliedToRectangle")]
    pub fn applied_to_rect<T: Coord>(self, item: Rect<T>, area: Rect<T>) -> Rect<T> {
        let x = if self.contains(Self::HORIZONTALLY_CENTRED) {
            area.x() + (area.width() - item.width()).half()
        } else if self.contains(Self::RIGHT) {
            area.right() - item.width()
        } else {
            area.x()
        };
        let y = if self.contains(Self::VERTICALLY_CENTRED) {
            area.y() + (area.height() - item.height()).half()
        } else if self.contains(Self::BOTTOM) {
            area.bottom() - item.height()
        } else {
            area.y()
        };
        item.with_x(x).with_y(y)
    }
}

impl BitOr for Justification {
    type Output = Self;
    fn bitor(self, o: Self) -> Self {
        Self(self.0 | o.0)
    }
}

/// How to scale and align a source rectangle into a destination, keeping its aspect ratio
/// unless [`STRETCH_TO_FIT`](Self::STRETCH_TO_FIT) is set. Flags combine with `|`.
///
/// Size: by default the source is scaled to the largest size that fits inside the destination.
/// [`FILL_DESTINATION`](Self::FILL_DESTINATION) instead scales it to the smallest size that
/// covers the destination. [`ONLY_REDUCE_IN_SIZE`](Self::ONLY_REDUCE_IN_SIZE) caps the scale at
/// 1, [`ONLY_INCREASE_IN_SIZE`](Self::ONLY_INCREASE_IN_SIZE) floors it at 1, and
/// [`DO_NOT_RESIZE`](Self::DO_NOT_RESIZE) fixes it at 1.
///
/// Position: `X_LEFT` / `X_RIGHT` / otherwise centred, and `Y_TOP` / `Y_BOTTOM` / otherwise
/// centred. [`STRETCH_TO_FIT`](Self::STRETCH_TO_FIT) ignores every other flag and returns the
/// destination. The default is [`CENTRED`](Self::CENTRED).
/// ```
/// use syrp_geom::{Rect, RectanglePlacement as P};
/// let r = P::CENTRED.apply_to(Rect::new(0.0, 0.0, 20.0, 10.0), Rect::new(0.0, 0.0, 100.0, 100.0));
/// assert_eq!(r, Rect::new(0.0, 25.0, 100.0, 50.0));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RectanglePlacement(u32);

impl Default for RectanglePlacement {
    /// [`CENTRED`](Self::CENTRED).
    fn default() -> Self {
        Self::CENTRED
    }
}

impl RectanglePlacement {
    /// Align left edges.
    pub const X_LEFT: Self = Self(1 << 0);
    /// Align right edges.
    pub const X_RIGHT: Self = Self(1 << 1);
    /// Centre horizontally (also the behaviour when no x flag is set).
    pub const X_MID: Self = Self(1 << 2);
    /// Align top edges.
    pub const Y_TOP: Self = Self(1 << 3);
    /// Align bottom edges.
    pub const Y_BOTTOM: Self = Self(1 << 4);
    /// Centre vertically (also the behaviour when no y flag is set).
    pub const Y_MID: Self = Self(1 << 5);
    /// Scale x and y independently to fill the destination exactly.
    pub const STRETCH_TO_FIT: Self = Self(1 << 6);
    /// Keep the aspect ratio and cover the whole destination (may overhang it).
    pub const FILL_DESTINATION: Self = Self(1 << 7);
    /// Never scale up.
    pub const ONLY_REDUCE_IN_SIZE: Self = Self(1 << 8);
    /// Never scale down.
    pub const ONLY_INCREASE_IN_SIZE: Self = Self(1 << 9);
    /// Keep the source size; only align it.
    pub const DO_NOT_RESIZE: Self =
        Self(Self::ONLY_REDUCE_IN_SIZE.0 | Self::ONLY_INCREASE_IN_SIZE.0);
    /// Centred both ways, fitting inside.
    pub const CENTRED: Self = Self(Self::X_MID.0 | Self::Y_MID.0);

    /// The raw flag bits (this crate's own values, not JUCE's).
    /// ```
    /// assert_ne!(syrp_geom::RectanglePlacement::X_LEFT.bits(), 0);
    /// ```
    #[doc(alias = "getFlags")]
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// True if every flag in `flags` is set.
    /// ```
    /// use syrp_geom::RectanglePlacement as P;
    /// assert!(P::DO_NOT_RESIZE.contains(P::ONLY_REDUCE_IN_SIZE));
    /// ```
    #[doc(alias = "testFlags")]
    pub const fn contains(self, flags: Self) -> bool {
        self.0 & flags.0 == flags.0
    }

    /// Scale factors and placed `(x, y, w, h)` in `f64`, or `None` if `source` is empty.
    fn place<T: Coord>(self, source: &Rect<T>, dest: &Rect<T>) -> Option<(f64, f64, [f64; 4])> {
        if source.is_empty() {
            return None;
        }
        let f = |v: T| v.to_f64();
        let (sw, sh) = (f(source.width()), f(source.height()));
        let [dx, dy, dw, dh] = [f(dest.x()), f(dest.y()), f(dest.width()), f(dest.height())];
        if self.contains(Self::STRETCH_TO_FIT) {
            return Some((dw / sw, dh / sh, [dx, dy, dw, dh]));
        }
        let (fx, fy) = (dw / sw, dh / sh);
        let mut k = if self.contains(Self::FILL_DESTINATION) {
            fx.max(fy)
        } else {
            fx.min(fy)
        };
        if self.contains(Self::ONLY_REDUCE_IN_SIZE) {
            k = k.min(1.0);
        }
        if self.contains(Self::ONLY_INCREASE_IN_SIZE) {
            k = k.max(1.0);
        }
        let (w, h) = (sw * k, sh * k);
        let align = |lo: Self, hi: Self, start: f64, space: f64, size: f64| {
            if self.contains(lo) {
                start
            } else if self.contains(hi) {
                start + space - size
            } else {
                start + (space - size) * 0.5
            }
        };
        let x = align(Self::X_LEFT, Self::X_RIGHT, dx, dw, w);
        let y = align(Self::Y_TOP, Self::Y_BOTTOM, dy, dh, h);
        Some((k, k, [x, y, w, h]))
    }

    /// Where `source` ends up when placed into `dest`. An empty `source` is returned unchanged.
    /// The maths is done in `f64`; for `i32` each edge is then rounded to nearest (halves away
    /// from zero), so the result keeps the exact edges' rounding rather than a rounded size.
    /// ```
    /// use syrp_geom::{Rect, RectanglePlacement as P};
    /// let int = P::DO_NOT_RESIZE.apply_to(Rect::new(0, 0, 10, 10), Rect::new(0, 0, 15, 10));
    /// assert_eq!(int, Rect::new(3, 0, 10, 10)); // 2.5 to 12.5 rounds to 3 to 13
    /// ```
    #[doc(alias = "appliedTo")]
    pub fn apply_to<T: Coord>(self, source: Rect<T>, dest: Rect<T>) -> Rect<T> {
        match self.place(&source, &dest) {
            None => source,
            Some((_, _, [x, y, w, h])) => Rect::from_ltrb(
                T::from_f64(x),
                T::from_f64(y),
                T::from_f64(x + w),
                T::from_f64(y + h),
            ),
        }
    }

    /// The transform that maps `source` onto its placed position in `dest` (exact `f64`, no
    /// rounding). An empty `source` gives the identity.
    /// ```
    /// use syrp_geom::{Point, Rect, RectanglePlacement as P};
    /// let t = P::STRETCH_TO_FIT.transform_to_fit(Rect::new(0, 0, 10, 10), Rect::new(5, 5, 20, 40));
    /// assert_eq!(t.transform_point(Point::new(10.0, 10.0)), Point::new(25.0, 45.0));
    /// ```
    #[doc(alias = "getTransformToFit")]
    pub fn transform_to_fit<T: Coord>(self, source: Rect<T>, dest: Rect<T>) -> AffineTransform {
        match self.place(&source, &dest) {
            None => AffineTransform::IDENTITY,
            Some((kx, ky, [x, y, _, _])) => {
                AffineTransform::translation(-source.x().to_f64(), -source.y().to_f64())
                    .followed_by(&AffineTransform::scale(kx, ky))
                    .translated(x, y)
            }
        }
    }
}

impl BitOr for RectanglePlacement {
    type Output = Self;
    fn bitor(self, o: Self) -> Self {
        Self(self.0 | o.0)
    }
}

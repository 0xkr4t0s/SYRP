//! [`RectList`]: a region made of non-overlapping rectangles.

use crate::{Coord, Point, Rect};

/// A region of the plane stored as a list of non-overlapping, non-empty rectangles.
///
/// After every operation the list is canonical: no two rectangles overlap, none is empty, and
/// no two rectangles that share a whole edge are left unmerged. The order of the rectangles
/// is unspecified. Unlike the other types in this crate, `RectList` owns a heap allocation.
/// ```
/// use syrp_geom::{Rect, RectList};
/// let mut region = RectList::new();
/// region.add(Rect::new(0, 0, 10, 10));
/// region.add(Rect::new(10, 0, 5, 10));
/// assert_eq!(region.len(), 1); // merged into one 15 x 10 rectangle
/// region.subtract(Rect::new(5, 5, 20, 20));
/// assert_eq!(region.bounds(), Rect::new(0, 0, 15, 10));
/// ```
#[doc(alias = "RectangleList")]
#[derive(Clone, Debug, Default)]
pub struct RectList<T> {
    rects: Vec<Rect<T>>,
}

/// Pushes the parts of `r` that lie outside `hole` onto `out` (at most four pieces).
fn push_difference<T: Coord>(r: Rect<T>, hole: &Rect<T>, out: &mut Vec<Rect<T>>) {
    let i = r.intersection(hole);
    if i.is_empty() {
        out.push(r);
        return;
    }
    let pieces = [
        Rect::from_ltrb(r.x(), r.y(), r.right(), i.y()),
        Rect::from_ltrb(r.x(), i.bottom(), r.right(), r.bottom()),
        Rect::from_ltrb(r.x(), i.y(), i.x(), i.bottom()),
        Rect::from_ltrb(i.right(), i.y(), r.right(), i.bottom()),
    ];
    out.extend(pieces.into_iter().filter(|p| !p.is_empty()));
}

/// True if `a` and `b` share a whole edge, so their union is exactly their combined area.
fn mergeable<T: Coord>(a: &Rect<T>, b: &Rect<T>) -> bool {
    let side_by_side =
        a.y() == b.y() && a.height() == b.height() && (a.right() == b.x() || b.right() == a.x());
    let stacked =
        a.x() == b.x() && a.width() == b.width() && (a.bottom() == b.y() || b.bottom() == a.y());
    side_by_side || stacked
}

impl<T: Coord> RectList<T> {
    /// An empty region.
    /// ```
    /// assert!(syrp_geom::RectList::<i32>::new().is_empty());
    /// ```
    pub fn new() -> Self {
        Self { rects: Vec::new() }
    }

    /// A region holding one rectangle (nothing if it is empty).
    /// ```
    /// let r = syrp_geom::RectList::from_rect(syrp_geom::Rect::new(0, 0, 4, 4));
    /// assert_eq!(r.len(), 1);
    /// ```
    pub fn from_rect(r: Rect<T>) -> Self {
        let mut list = Self::new();
        list.add(r);
        list
    }

    /// Number of rectangles in the canonical list.
    /// ```
    /// assert_eq!(syrp_geom::RectList::<f32>::new().len(), 0);
    /// ```
    #[doc(alias = "getNumRectangles")]
    pub fn len(&self) -> usize {
        self.rects.len()
    }

    /// True if the region covers no area.
    /// ```
    /// assert!(syrp_geom::RectList::from_rect(syrp_geom::Rect::new(0, 0, 0, 9)).is_empty());
    /// ```
    #[doc(alias = "isEmpty")]
    pub fn is_empty(&self) -> bool {
        self.rects.is_empty()
    }

    /// The rectangles making up the region, in unspecified order.
    /// ```
    /// let r = syrp_geom::RectList::from_rect(syrp_geom::Rect::new(1, 1, 2, 2));
    /// assert_eq!(r.iter().count(), 1);
    /// ```
    pub fn iter(&self) -> core::slice::Iter<'_, Rect<T>> {
        self.rects.iter()
    }

    /// Removes everything, keeping the allocation.
    /// ```
    /// let mut r = syrp_geom::RectList::from_rect(syrp_geom::Rect::new(1, 1, 2, 2));
    /// r.clear();
    /// assert!(r.is_empty());
    /// ```
    pub fn clear(&mut self) {
        self.rects.clear();
    }

    /// Adds the area of `r` to the region. Empty rectangles are ignored.
    /// ```
    /// use syrp_geom::{Rect, RectList};
    /// let mut r = RectList::from_rect(Rect::new(0, 0, 10, 10));
    /// r.add(Rect::new(5, 5, 10, 10));
    /// assert!(r.contains_rect(&Rect::new(10, 10, 5, 5)));
    /// ```
    pub fn add(&mut self, r: Rect<T>) {
        if r.is_empty() {
            return;
        }
        let mut pending = vec![r];
        let mut next = Vec::new();
        for existing in &self.rects {
            for piece in pending.drain(..) {
                push_difference(piece, existing, &mut next);
            }
            core::mem::swap(&mut pending, &mut next);
            if pending.is_empty() {
                return;
            }
        }
        self.settle(pending);
    }

    /// Removes the area of `r` from the region.
    /// ```
    /// use syrp_geom::{Point, Rect, RectList};
    /// let mut r = RectList::from_rect(Rect::new(0, 0, 10, 10));
    /// r.subtract(Rect::new(2, 2, 2, 2));
    /// assert!(!r.contains_point(Point::new(3, 3)) && r.contains_point(Point::new(1, 1)));
    /// ```
    pub fn subtract(&mut self, r: Rect<T>) {
        if r.is_empty() || !self.intersects_rect(&r) {
            return;
        }
        // Untouched rects stay settled; only the pieces of cut rects need merging.
        let mut pieces = Vec::new();
        let mut i = 0;
        while i < self.rects.len() {
            if self.rects[i].intersects(&r) {
                push_difference(self.rects.swap_remove(i), &r, &mut pieces);
            } else {
                i += 1;
            }
        }
        self.settle(pieces);
    }

    /// Keeps only the part of the region inside `r`.
    /// ```
    /// use syrp_geom::{Rect, RectList};
    /// let mut r = RectList::from_rect(Rect::new(0, 0, 10, 10));
    /// r.clip_to(&Rect::new(5, 5, 10, 10));
    /// assert_eq!(r.bounds(), Rect::new(5, 5, 5, 5));
    /// ```
    #[doc(alias = "clipTo")]
    pub fn clip_to(&mut self, r: &Rect<T>) {
        let clipped: Vec<_> = self
            .rects
            .drain(..)
            .map(|x| x.intersection(r))
            .filter(|x| !x.is_empty())
            .collect();
        self.settle(clipped);
    }

    /// The smallest rectangle enclosing the whole region (`Rect::default()` when empty).
    /// ```
    /// use syrp_geom::{Rect, RectList};
    /// let mut r = RectList::from_rect(Rect::new(0, 0, 1, 1));
    /// r.add(Rect::new(5, 5, 1, 1));
    /// assert_eq!(r.bounds(), Rect::new(0, 0, 6, 6));
    /// ```
    #[doc(alias = "getBounds")]
    pub fn bounds(&self) -> Rect<T> {
        self.rects
            .iter()
            .fold(Rect::default(), |acc, r| acc.union(r))
    }

    /// True if `p` is inside one of the rectangles (left/top edges in, right/bottom out).
    /// ```
    /// use syrp_geom::{Point, Rect, RectList};
    /// assert!(RectList::from_rect(Rect::new(0, 0, 2, 2)).contains_point(Point::new(1, 1)));
    /// ```
    #[doc(alias = "containsPoint")]
    pub fn contains_point(&self, p: Point<T>) -> bool {
        self.rects.iter().any(|r| r.contains_point(p))
    }

    /// True if every part of `r` is covered by the region (even if it spans several rects).
    /// An empty `r` is always contained.
    /// ```
    /// use syrp_geom::{Rect, RectList};
    /// let mut r = RectList::from_rect(Rect::new(0, 0, 5, 5));
    /// r.add(Rect::new(5, 0, 5, 3));
    /// assert!(r.contains_rect(&Rect::new(3, 0, 4, 3)));
    /// assert!(!r.contains_rect(&Rect::new(3, 0, 4, 4)));
    /// ```
    #[doc(alias = "containsRectangle")]
    pub fn contains_rect(&self, r: &Rect<T>) -> bool {
        let mut rest = RectList::from_rect(*r);
        for x in &self.rects {
            rest.subtract(*x);
        }
        rest.is_empty()
    }

    /// True if any part of `r` overlaps the region with a non-zero area.
    /// ```
    /// use syrp_geom::{Rect, RectList};
    /// let r = RectList::from_rect(Rect::new(0, 0, 5, 5));
    /// assert!(r.intersects_rect(&Rect::new(4, 4, 5, 5)));
    /// assert!(!r.intersects_rect(&Rect::new(5, 0, 5, 5)));
    /// ```
    #[doc(alias = "intersectsRectangle")]
    pub fn intersects_rect(&self, r: &Rect<T>) -> bool {
        self.rects.iter().any(|x| x.intersects(r))
    }

    /// Moves `worklist` rects (which must not overlap the list or each other) into the list,
    /// merging as it goes. The list is kept free of mergeable pairs: each incoming rect is
    /// checked once against the list; on a match the partner is removed and the union goes
    /// back on the worklist. Cost is O(list length) per incoming rect or merge.
    fn settle(&mut self, mut worklist: Vec<Rect<T>>) {
        while let Some(r) = worklist.pop() {
            match self.rects.iter().position(|x| mergeable(x, &r)) {
                Some(i) => worklist.push(self.rects.swap_remove(i).union(&r)),
                None => self.rects.push(r),
            }
        }
    }
}

impl<'a, T: Coord> IntoIterator for &'a RectList<T> {
    type Item = &'a Rect<T>;
    type IntoIter = core::slice::Iter<'a, Rect<T>>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

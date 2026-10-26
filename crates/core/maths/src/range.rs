//! `Range<T>` (spec B1).

use std::ops::{Add, Sub};

/// A half-open range of values, `start..end`, with `start <= end`.
///
/// Works with any `Copy` type that has ordering, addition and subtraction, in practice the
/// integer and float primitives. An empty range has `start == end`. Integer arithmetic follows
/// the usual overflow rules (it panics in debug builds on overflow).
///
/// ```
/// use syrp_maths::Range;
/// let r = Range::new(10, 2);
/// assert_eq!((r.start(), r.end(), r.length()), (2, 10, 8));
/// assert!(r.contains(2) && !r.contains(10));
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Range<T> {
    start: T,
    end: T,
}

impl<T> Range<T>
where
    T: Copy + PartialOrd + Add<Output = T> + Sub<Output = T>,
{
    /// Creates a range between `a` and `b`, whichever order they are given in.
    ///
    /// ```
    /// use syrp_maths::Range;
    /// assert_eq!(Range::new(3.0, 1.0), Range::new(1.0, 3.0));
    /// ```
    pub fn new(a: T, b: T) -> Self {
        if b < a {
            Self { start: b, end: a }
        } else {
            Self { start: a, end: b }
        }
    }

    /// Creates the range `start..start + length`; a negative length gives an empty range.
    ///
    /// ```
    /// use syrp_maths::Range;
    /// assert_eq!(Range::with_start_and_length(5, 3), Range::new(5, 8));
    /// ```
    #[doc(alias = "withStartAndLength")]
    pub fn with_start_and_length(start: T, length: T) -> Self {
        let end = start + length;
        Self {
            start,
            end: if end < start { start } else { end },
        }
    }

    /// The first value in the range.
    ///
    /// ```
    /// assert_eq!(syrp_maths::Range::new(4, 9).start(), 4);
    /// ```
    #[doc(alias = "getStart")]
    pub fn start(self) -> T {
        self.start
    }

    /// The end of the range, which is not itself inside it.
    ///
    /// ```
    /// assert_eq!(syrp_maths::Range::new(4, 9).end(), 9);
    /// ```
    #[doc(alias = "getEnd")]
    pub fn end(self) -> T {
        self.end
    }

    /// `end - start`.
    ///
    /// ```
    /// assert_eq!(syrp_maths::Range::new(0.5, 2.0).length(), 1.5);
    /// ```
    #[doc(alias = "getLength")]
    pub fn length(self) -> T {
        self.end - self.start
    }

    /// True when `start == end`.
    ///
    /// ```
    /// use syrp_maths::Range;
    /// assert!(Range::new(3, 3).is_empty());
    /// assert!(!Range::new(3, 4).is_empty());
    /// ```
    #[doc(alias = "isEmpty")]
    pub fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// True when `start <= value < end`. An empty range contains nothing.
    ///
    /// ```
    /// use syrp_maths::Range;
    /// let r = Range::new(0, 4);
    /// assert!(r.contains(0) && r.contains(3) && !r.contains(4));
    /// ```
    pub fn contains(self, value: T) -> bool {
        self.start <= value && value < self.end
    }

    /// True when every value of `other` is inside this range (an empty `other` counts when its
    /// position lies within `start..=end`).
    ///
    /// ```
    /// use syrp_maths::Range;
    /// assert!(Range::new(0, 10).contains_range(Range::new(2, 10)));
    /// assert!(!Range::new(0, 10).contains_range(Range::new(5, 11)));
    /// ```
    #[doc(alias = "contains")]
    pub fn contains_range(self, other: Self) -> bool {
        self.start <= other.start && other.end <= self.end
    }

    /// Clamps `value` to `start..=end`.
    ///
    /// ```
    /// use syrp_maths::Range;
    /// let r = Range::new(1.0, 2.0);
    /// assert_eq!((r.clip_value(0.0), r.clip_value(1.5), r.clip_value(3.0)), (1.0, 1.5, 2.0));
    /// ```
    #[doc(alias = "clipValue")]
    pub fn clip_value(self, value: T) -> T {
        if value < self.start {
            self.start
        } else if self.end < value {
            self.end
        } else {
            value
        }
    }

    /// True when the two ranges share at least one value. Ranges that only touch end to start
    /// do not intersect, and an empty range intersects nothing.
    ///
    /// ```
    /// use syrp_maths::Range;
    /// assert!(Range::new(0, 5).intersects(Range::new(4, 8)));
    /// assert!(!Range::new(0, 5).intersects(Range::new(5, 8)));
    /// ```
    pub fn intersects(self, other: Self) -> bool {
        max(self.start, other.start) < min(self.end, other.end)
    }

    /// The overlap of the two ranges. When they do not overlap the result is empty and lies
    /// at the larger of the two starts (or this range's end, whichever is larger).
    ///
    /// ```
    /// use syrp_maths::Range;
    /// assert_eq!(Range::new(0, 5).get_intersection_with(Range::new(3, 9)), Range::new(3, 5));
    /// assert!(Range::new(0, 2).get_intersection_with(Range::new(4, 9)).is_empty());
    /// ```
    #[doc(alias = "getIntersectionWith")]
    pub fn get_intersection_with(self, other: Self) -> Self {
        let start = max(self.start, other.start);
        Self {
            start,
            end: max(start, min(self.end, other.end)),
        }
    }

    /// The smallest range that covers both ranges, including any gap between them.
    ///
    /// ```
    /// use syrp_maths::Range;
    /// assert_eq!(Range::new(0, 2).get_union_with(Range::new(5, 9)), Range::new(0, 9));
    /// ```
    #[doc(alias = "getUnionWith")]
    pub fn get_union_with(self, other: Self) -> Self {
        Self {
            start: min(self.start, other.start),
            end: max(self.end, other.end),
        }
    }

    /// The range grown by `amount` at both ends (a negative amount shrinks it; the result is
    /// re-ordered if the ends cross).
    ///
    /// ```
    /// use syrp_maths::Range;
    /// assert_eq!(Range::new(10, 20).expanded(5), Range::new(5, 25));
    /// ```
    pub fn expanded(self, amount: T) -> Self {
        Self::new(self.start - amount, self.end + amount)
    }

    /// The same length, moved so that it starts at `new_start`.
    ///
    /// ```
    /// use syrp_maths::Range;
    /// assert_eq!(Range::new(10, 14).moved_to_start_at(0), Range::new(0, 4));
    /// ```
    #[doc(alias = "movedToStartAt")]
    pub fn moved_to_start_at(self, new_start: T) -> Self {
        Self {
            start: new_start,
            end: new_start + self.length(),
        }
    }

    /// Fits `range` inside this range by moving it, keeping its length. If `range` is longer
    /// than this range, the result is this range.
    ///
    /// ```
    /// use syrp_maths::Range;
    /// let outer = Range::new(0, 10);
    /// assert_eq!(outer.constrain_range(Range::new(8, 12)), Range::new(6, 10));
    /// assert_eq!(outer.constrain_range(Range::new(-3, 1)), Range::new(0, 4));
    /// assert_eq!(outer.constrain_range(Range::new(-5, 20)), outer);
    /// ```
    #[doc(alias = "constrainRange")]
    pub fn constrain_range(self, range: Self) -> Self {
        let length = range.length();
        if self.length() <= length {
            return self;
        }
        if range.start < self.start {
            range.moved_to_start_at(self.start)
        } else if self.end < range.end {
            range.moved_to_start_at(self.end - length)
        } else {
            range
        }
    }
}

fn min<T: PartialOrd>(a: T, b: T) -> T {
    if b < a { b } else { a }
}

fn max<T: PartialOrd>(a: T, b: T) -> T {
    if a < b { b } else { a }
}

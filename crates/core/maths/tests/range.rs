//! B1: `Range<T>` for integer and float types.

use syrp_maths::Range;

#[test]
fn b1_new_orders_ends_and_reports_length() {
    let r = Range::new(9, 3);
    assert_eq!((r.start(), r.end(), r.length()), (3, 9, 6));
    let f = Range::new(2.5_f64, -1.0);
    assert_eq!((f.start(), f.end(), f.length()), (-1.0, 2.5, 3.5));
    assert_eq!(Range::with_start_and_length(4_i64, -2), Range::new(4, 4));
}

#[test]
fn b1_empty_means_start_equals_end() {
    assert!(Range::new(5_u32, 5).is_empty());
    assert!(Range::<f32>::default().is_empty());
    assert!(!Range::new(0.0_f32, f32::MIN_POSITIVE).is_empty());
}

#[test]
fn b1_contains_is_half_open() {
    let r = Range::new(-2, 2);
    assert!(r.contains(-2) && r.contains(1));
    assert!(!r.contains(2) && !r.contains(-3));
    assert!(!Range::new(3, 3).contains(3));
    let f = Range::new(0.0, 1.0);
    assert!(f.contains(0.0) && f.contains(0.999_999) && !f.contains(1.0));
    assert!(f.contains_range(Range::new(0.0, 1.0)) && !f.contains_range(Range::new(0.5, 1.5)));
}

#[test]
fn b1_clip_value_clamps_to_both_ends() {
    let r = Range::new(10, 20);
    assert_eq!(
        [
            r.clip_value(0),
            r.clip_value(15),
            r.clip_value(20),
            r.clip_value(99)
        ],
        [10, 15, 20, 20]
    );
    assert_eq!(Range::new(-1.0_f32, 1.0).clip_value(-7.5), -1.0);
}

#[test]
fn b1_intersects_needs_shared_values() {
    let r = Range::new(0, 10);
    assert!(r.intersects(Range::new(9, 12)));
    assert!(r.intersects(Range::new(-5, 50)));
    assert!(!r.intersects(Range::new(10, 12)));
    assert!(!r.intersects(Range::new(-3, 0)));
    assert!(!r.intersects(Range::new(5, 5)));
}

#[test]
fn b1_intersection_and_union() {
    let a = Range::new(0, 10);
    assert_eq!(
        a.get_intersection_with(Range::new(4, 20)),
        Range::new(4, 10)
    );
    assert_eq!(a.get_intersection_with(Range::new(2, 3)), Range::new(2, 3));
    let disjoint = a.get_intersection_with(Range::new(15, 20));
    assert!(disjoint.is_empty());
    assert_eq!(disjoint.start(), 15);
    assert_eq!(a.get_union_with(Range::new(15, 20)), Range::new(0, 20));
    assert_eq!(
        Range::new(1.0, 2.0).get_union_with(Range::new(-1.0, 1.5)),
        Range::new(-1.0, 2.0)
    );
}

#[test]
fn b1_expanded_and_moved_to_start_at() {
    assert_eq!(Range::new(10, 20).expanded(3), Range::new(7, 23));
    assert_eq!(Range::new(10, 20).expanded(-8), Range::new(12, 18));
    assert_eq!(Range::new(10, 20).expanded(-6), Range::new(14, 16));
    assert_eq!(Range::new(0.5, 1.0).expanded(0.25), Range::new(0.25, 1.25));
    let moved = Range::new(10, 14).moved_to_start_at(-2);
    assert_eq!((moved.start(), moved.end()), (-2, 2));
}

#[test]
fn b1_constrain_range_moves_inside_and_keeps_length() {
    let outer = Range::new(0, 100);
    assert_eq!(
        outer.constrain_range(Range::new(90, 110)),
        Range::new(80, 100)
    );
    assert_eq!(outer.constrain_range(Range::new(-10, 5)), Range::new(0, 15));
    assert_eq!(
        outer.constrain_range(Range::new(40, 60)),
        Range::new(40, 60)
    );
    assert_eq!(outer.constrain_range(Range::new(-1, 200)), outer);
    assert_eq!(outer.constrain_range(Range::new(5, 105)), outer);
    let f = Range::new(0.0, 1.0).constrain_range(Range::new(0.75, 1.5));
    assert_eq!(f, Range::new(0.25, 1.0));
}

//! Behaviour tests for B5 (BorderSize, Line, Parallelogram).

use std::f64::consts::{FRAC_PI_2, PI};
use syrp_geom::{AffineTransform, BorderSize, Line, Parallelogram, Point, Rect};

#[test]
fn b5_border_size_shrinks_and_grows_rects() {
    let b = BorderSize::new(1, 2, 3, 4);
    assert_eq!((b.top_and_bottom(), b.left_and_right()), (4, 6));
    let r = Rect::new(10, 10, 20, 20);
    assert_eq!(b.subtracted_from(r), Rect::new(12, 11, 14, 16));
    assert_eq!(b.subtracted_from(b.added_to(r)), r);
    assert!(b.subtracted_from(Rect::new(0, 0, 5, 5)).is_empty()); // clamps, never negative
    assert!(BorderSize::<i32>::default().is_empty() && !BorderSize::uniform(0.5).is_empty());
    let f = BorderSize::uniform(0.25_f32).subtracted_from(Rect::new(0.0, 0.0, 1.0, 1.0));
    assert_eq!(f, Rect::new(0.25, 0.25, 0.5, 0.5));
}

#[test]
fn b5_line_length_and_angle() {
    let o = Point::new(0.0, 0.0);
    let at = |x, y| Line::new(o, Point::new(x, y)).angle();
    assert_eq!(
        (at(0.0, -1.0), at(1.0, 0.0), at(0.0, 1.0), at(-1.0, 0.0)),
        (0.0, FRAC_PI_2, PI, -FRAC_PI_2)
    );
    assert_eq!(Line::new(o, o).angle(), 0.0);
    assert_eq!(Line::new(Point::new(1, 1), Point::new(4, 5)).length(), 5.0);
    let l = Line::new(Point::new(2, 3), Point::new(5, 7));
    assert_eq!(l.reversed().reversed(), l);
}

#[test]
fn b5_line_point_along_rounds_only_for_integers() {
    let f = Line::new(Point::new(0.0, 0.0), Point::new(3.0, 4.0));
    assert_eq!(f.point_along(2.5), Point::new(1.5, 2.0));
    assert_eq!(f.point_along(-5.0), Point::new(-3.0, -4.0));
    assert_eq!(f.point_along_proportionally(0.5), Point::new(1.5, 2.0));
    let i = Line::new(Point::new(0, 0), Point::new(3, 4));
    assert_eq!(i.point_along(2.5), Point::new(2, 2)); // (1.5, 2.0) rounds half away from zero
    assert_eq!(i.reversed().point_along(2.5), Point::new(2, 2)); // (1.5, 2.0) again
    let neg = Line::new(Point::new(0, 0), Point::new(-3, 0));
    assert_eq!(neg.point_along_proportionally(0.5), Point::new(-2, 0)); // -1.5 rounds to -2
    let zero = Line::new(Point::new(7, 7), Point::new(7, 7));
    assert_eq!(zero.point_along(10.0), Point::new(7, 7));
}

#[test]
fn b5_line_intersection_for_float_and_integer_lines() {
    let a = Line::new(Point::new(0.0, 0.0), Point::new(3.0, 1.0));
    let b = Line::new(Point::new(0.0, 1.0), Point::new(3.0, 0.0));
    assert_eq!(a.intersection(&b), Some(Point::new(1.5, 0.5)));
    let ai = Line::new(Point::new(0, 0), Point::new(3, 1));
    let bi = Line::new(Point::new(0, 1), Point::new(3, 0));
    assert_eq!(ai.intersection(&bi), Some(Point::new(2, 1))); // (1.5, 0.5) rounded
    // Touching at an endpoint counts; disjoint and parallel segments do not meet.
    let c = Line::new(Point::new(3.0, 1.0), Point::new(5.0, -1.0));
    assert_eq!(a.intersection(&c), Some(Point::new(3.0, 1.0)));
    let d = Line::new(Point::new(10.0, 0.0), Point::new(10.0, 5.0));
    assert_eq!(a.intersection(&d), None);
    let e = Line::new(Point::new(0.0, 0.0), Point::new(6.0, 2.0));
    assert_eq!(a.intersection(&e), None);
}

#[test]
fn b5_parallelogram_from_rect_and_transform() {
    let p = Parallelogram::from(Rect::new(1, 2, 4, 3));
    assert_eq!(p.bottom_right(), Point::new(5, 5));
    assert_eq!((p.width(), p.height()), (4.0, 3.0));
    assert_eq!(p.bounding_box(), Rect::new(1, 2, 4, 3));
    let rot = AffineTransform::rotation(FRAC_PI_2);
    let pf = Parallelogram::from(Rect::new(0.0_f64, 0.0, 10.0, 5.0)).transformed_by(&rot);
    let bb = pf.bounding_box();
    assert!((bb.x() + 5.0).abs() < 1e-9 && bb.y().abs() < 1e-9);
    assert!((bb.width() - 5.0).abs() < 1e-9 && (bb.height() - 10.0).abs() < 1e-9);
    // Integer corners round to nearest; float corners keep the exact value.
    let half = AffineTransform::scale(0.5, 0.5);
    let pi = Parallelogram::from(Rect::new(0, 0, 3, 3)).transformed_by(&half);
    assert_eq!(pi.top_right, Point::new(2, 0));
    let pf = Parallelogram::from(Rect::new(0.0, 0.0, 3.0, 3.0)).transformed_by(&half);
    assert_eq!(pf.top_right, Point::new(1.5, 0.0));
    let sheared = Parallelogram::new(Point::new(0, 0), Point::new(4, 0), Point::new(-2, 3));
    assert_eq!(sheared.bounding_box(), Rect::from_ltrb(-2, 0, 4, 3));
}

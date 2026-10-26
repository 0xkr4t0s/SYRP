//! Behaviour tests for B1 (generic Point/Rect) and B2 (Rect operations).

use syrp_geom::{Coord, Point, Rect};

fn roundtrip<T: Coord>(x: T, y: T, w: T, h: T) -> Rect<T> {
    let r = Rect::new(x, y, w, h);
    Rect::from_ltrb(r.x(), r.y(), r.right(), r.bottom())
}

#[test]
fn b1_point_and_rect_are_generic_over_i32_f32_f64() {
    assert_eq!(roundtrip(1, 2, 3, 4), Rect::new(1, 2, 3, 4));
    assert_eq!(
        roundtrip(1.5_f32, 2.0, 3.0, 4.0),
        Rect::new(1.5, 2.0, 3.0, 4.0)
    );
    assert_eq!(
        roundtrip(-1.0_f64, 0.5, 2.0, 0.25),
        Rect::new(-1.0, 0.5, 2.0, 0.25)
    );
    assert_eq!(
        Point::new(1.25_f64, -2.5) * 2.0 - Point::new(0.5, 0.0),
        Point::new(2.0, -5.0)
    );
}

#[test]
fn b1_negative_sizes_normalise_to_empty() {
    let r = Rect::new(5, 5, -10, -1);
    assert_eq!((r.width(), r.height()), (0, 0));
    assert!(r.is_empty() && Rect::from_ltrb(10, 0, 0, 10).is_empty());
    assert!(Rect::new(0.0, 0.0, f64::NAN, 1.0).is_empty());
    assert!(Rect::new(0, 0, 4, 4).with_width(-3).is_empty());
}

#[test]
fn b2_intersection_union_and_contains() {
    let a = Rect::new(0, 0, 10, 10);
    assert_eq!(
        a.intersection(&Rect::new(-5, 2, 8, 3)),
        Rect::new(0, 2, 3, 3)
    );
    assert!(!a.intersects(&Rect::new(10, 0, 5, 5)) && !a.intersects(&Rect::new(2, 2, 0, 5)));
    let (p, q) = (
        Rect::new(0.0, 0.0, 1.0, 1.0),
        Rect::new(3.0, -2.0, 1.0, 1.0),
    );
    assert_eq!(p.union(&q), Rect::from_ltrb(0.0, -2.0, 4.0, 1.0));
    assert_eq!(p.union(&Rect::new(100.0, 100.0, 0.0, 5.0)), p);
    assert!(a.contains_point(Point::new(9, 9)) && !a.contains_point(Point::new(-1, 5)));
    assert!(a.contains(&a) && !a.contains(&Rect::new(-1, 0, 2, 2)));
}

#[test]
fn b2_reduced_and_expanded() {
    let r = Rect::new(10, 10, 20, 20);
    assert_eq!(r.reduced(5, 2), Rect::new(15, 12, 10, 16));
    assert_eq!(r.reduced(5, 2).expanded(5, 2), r);
    assert!(r.reduced(11, 0).is_empty());
}

#[test]
fn b2_take_edges_mutate_and_return_the_slice() {
    let mut r = Rect::new(0, 0, 100, 100);
    assert_eq!(r.take_top(10), Rect::new(0, 0, 100, 10));
    assert_eq!(r.take_bottom(20), Rect::new(0, 80, 100, 20));
    assert_eq!(r.take_left(30), Rect::new(0, 10, 30, 70));
    assert_eq!(r.take_right(40), Rect::new(60, 10, 40, 70));
    assert_eq!(r, Rect::new(30, 10, 30, 70));
    // Amounts are clamped to 0..=size.
    assert_eq!(r.take_left(1000), Rect::new(30, 10, 30, 70));
    assert!(r.is_empty());
    let mut s = Rect::new(0.0, 0.0, 5.0, 5.0);
    assert!(s.take_top(-3.0).is_empty());
    assert_eq!(s, Rect::new(0.0, 0.0, 5.0, 5.0));
}

#[test]
fn b2_with_builders() {
    let r = Rect::new(10, 20, 30, 40);
    assert_eq!(r.with_x(0).with_y(0).with_size(1, 2), Rect::new(0, 0, 1, 2));
    assert_eq!(r.with_centre(Point::new(0, 0)), Rect::new(-15, -20, 30, 40));
    assert_eq!(r.with_left(20), Rect::new(20, 20, 20, 40));
    assert_eq!(r.with_left(50), Rect::new(50, 20, 0, 40));
    assert_eq!(r.with_right(5), Rect::new(10, 20, 0, 40));
    assert_eq!(r.with_top(30).with_bottom(50), Rect::new(10, 30, 30, 20));
    assert_eq!(r.translated(-10, -20).position(), Point::origin());
}

#[test]
fn b2_to_nearest_int_rounds_halves_away_from_zero() {
    let r = Rect::new(-0.5_f32, 1.5, 2.49, 3.5).to_nearest_int();
    assert_eq!(r, Rect::new(-1, 2, 2, 4));
    assert_eq!(
        Point::new(-2.5_f64, 2.5).to_nearest_int(),
        Point::new(-3, 3)
    );
    let huge = Rect::new(1e20_f64, f64::NAN, 1.0, 1.0).to_nearest_int();
    assert_eq!((huge.x(), huge.y()), (i32::MAX, 0));
}

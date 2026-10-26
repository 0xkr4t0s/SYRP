//! Behaviour tests for B4 (AffineTransform).

use std::f64::consts::{FRAC_PI_2, PI};
use syrp_geom::{AffineTransform as T, Point, Rect};

fn close(a: Point<f64>, b: Point<f64>) -> bool {
    (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9
}

#[test]
fn b4_identity_translation_scale_shear() {
    assert!(T::default().is_identity() && !T::translation(1.0, 0.0).is_identity());
    assert_eq!(
        T::default().transform_point(Point::new(3, -4)),
        Point::new(3, -4)
    );
    let p = Point::new(2.0, 4.0);
    assert_eq!(
        T::translation(2.0, -1.0).transform_point(p),
        Point::new(4.0, 3.0)
    );
    assert_eq!(T::scale(2.0, 3.0).transform_point(p), Point::new(4.0, 12.0));
    assert_eq!(T::shear(0.5, 2.0).transform_point(p), Point::new(4.0, 8.0));
}

#[test]
fn b4_rotation_in_radians_with_optional_pivot() {
    let r = T::rotation(FRAC_PI_2);
    assert!(close(
        r.transform_point(Point::new(1.0, 0.0)),
        Point::new(0.0, 1.0)
    ));
    assert!(close(
        r.transform_point(Point::new(0.0, 1.0)),
        Point::new(-1.0, 0.0)
    ));
    let p = T::rotation_about(PI, 10.0, 10.0);
    assert!(close(
        p.transform_point(Point::new(12.0, 10.0)),
        Point::new(8.0, 10.0)
    ));
}

#[test]
fn b4_followed_by_applies_self_first() {
    let (a, b) = (T::translation(10.0, 0.0), T::scale(2.0, 2.0));
    let one = Point::new(1.0, 1.0);
    assert_eq!(
        a.followed_by(&b).transform_point(one),
        Point::new(22.0, 2.0)
    );
    assert_eq!(
        b.followed_by(&a).transform_point(one),
        Point::new(12.0, 2.0)
    );
    assert_eq!(b.translated(10.0, 0.0), b.followed_by(&a));
}

#[test]
fn b4_inversion_returns_none_when_singular() {
    assert!(T::scale(0.0, 5.0).inverted().is_none());
    assert!(T::new(1.0, 2.0, 0.0, 2.0, 4.0, 0.0).inverted().is_none());
    assert!(T::scale(f64::INFINITY, 1.0).inverted().is_none());
    let t = T::rotation(0.3)
        .followed_by(&T::scale(2.0, 0.5))
        .translated(4.0, -7.0);
    let p = Point::new(3.5, -1.25);
    assert!(close(
        t.inverted().unwrap().transform_point(t.transform_point(p)),
        p
    ));
}

#[test]
fn b4_transforms_rects_to_their_bounding_box() {
    let r = T::rotation_about(PI / 4.0, 1.0, 1.0).transform_rect(Rect::new(0.0, 0.0, 2.0, 2.0));
    assert!((r.width() - 2.0 * 2.0_f64.sqrt()).abs() < 1e-9);
    assert!(close(r.centre(), Point::new(1.0, 1.0)));
    // Integer rects are widened outwards so they still enclose the transformed shape.
    let ir = T::scale(1.5, 1.5).transform_rect(Rect::new(1, 1, 1, 1));
    assert_eq!(ir, Rect::from_ltrb(1, 1, 3, 3));
}

#[test]
fn b4_inversion_returns_none_when_the_inverse_overflows() {
    // Not singular (determinant 1e-300), but the inverse translation overflows.
    let t = T::new(1e-300, 0.0, 1e10, 0.0, 1.0, 0.0);
    assert!(!t.is_singular());
    assert!(t.inverted().is_none());
}

#[test]
fn b4_float_rects_are_not_snapped_to_integers() {
    let r = T::translation(1e-10, 0.0).transform_rect(Rect::new(0.0, 0.0, 1.0, 1.0));
    assert_eq!(r.x(), 1e-10);
    let r32 = T::translation(1e-6, 0.0).transform_rect(Rect::new(0.0_f32, 0.0, 1.0, 1.0));
    assert_eq!(r32.x(), 1e-6_f32);
    // Integer rects still absorb rounding noise.
    let ir = T::translation(1e-10, 0.0).transform_rect(Rect::new(0, 0, 1, 1));
    assert_eq!(ir, Rect::new(0, 0, 1, 1));
}

//! B8: property tests for algebraic laws.

use proptest::prelude::*;
use syrp_geom::{AffineTransform, Point, Rect};

fn int_rect() -> impl Strategy<Value = Rect<i32>> {
    (-1000..1000, -1000..1000, -50..500, -50..500).prop_map(|(x, y, w, h)| Rect::new(x, y, w, h))
}

fn float_rect() -> impl Strategy<Value = Rect<f64>> {
    (-1e3..1e3, -1e3..1e3, -10.0..500.0, -10.0..500.0)
        .prop_map(|(x, y, w, h)| Rect::new(x, y, w, h))
}

fn elementary() -> impl Strategy<Value = AffineTransform> {
    prop_oneof![
        (-100.0..100.0, -100.0..100.0).prop_map(|(x, y)| AffineTransform::translation(x, y)),
        (-7.0..7.0, -50.0..50.0, -50.0..50.0)
            .prop_map(|(a, x, y)| AffineTransform::rotation_about(a, x, y)),
        (0.25..4.0, 0.25..4.0, any::<bool>()).prop_map(|(x, y, flip): (f64, f64, bool)| {
            AffineTransform::scale(if flip { -x } else { x }, y)
        }),
        (-2.0..2.0, -0.4..0.4).prop_map(|(x, y)| AffineTransform::shear(x, y)),
    ]
}

fn transform() -> impl Strategy<Value = AffineTransform> {
    prop::collection::vec(elementary(), 1..4).prop_map(|ts| {
        ts.iter()
            .fold(AffineTransform::IDENTITY, |acc, t| acc.followed_by(t))
    })
}

/// Containment allowing for the last-bit rounding of `x + width` in floating point.
fn contains_approx(outer: &Rect<f64>, inner: &Rect<f64>) -> bool {
    let e = 1e-9;
    inner.x() >= outer.x() - e
        && inner.y() >= outer.y() - e
        && inner.right() <= outer.right() + e
        && inner.bottom() <= outer.bottom() + e
}

fn assert_identity(t: &AffineTransform) -> Result<(), TestCaseError> {
    let id = AffineTransform::IDENTITY;
    let pairs = [
        (t.m00, id.m00),
        (t.m01, id.m01),
        (t.m02, id.m02),
        (t.m10, id.m10),
        (t.m11, id.m11),
        (t.m12, id.m12),
    ];
    for (got, want) in pairs {
        prop_assert!((got - want).abs() <= 1e-9, "{t:?} is not the identity");
    }
    Ok(())
}

proptest! {
    #[test]
    fn b8_intersection_is_commutative_i32(a in int_rect(), b in int_rect()) {
        prop_assert_eq!(a.intersection(&b), b.intersection(&a));
        prop_assert_eq!(a.intersects(&b), b.intersects(&a));
        let i = a.intersection(&b);
        prop_assert!(i.is_empty() || (a.contains(&i) && b.contains(&i)));
    }

    #[test]
    fn b8_intersection_is_commutative_f64(a in float_rect(), b in float_rect()) {
        prop_assert_eq!(a.intersection(&b), b.intersection(&a));
    }

    #[test]
    fn b8_union_contains_both(a in int_rect(), b in int_rect()) {
        let u = a.union(&b);
        prop_assert!(a.is_empty() || u.contains(&a));
        prop_assert!(b.is_empty() || u.contains(&b));
    }

    #[test]
    fn b8_union_contains_both_f64(a in float_rect(), b in float_rect()) {
        let u = a.union(&b);
        prop_assert!(a.is_empty() || contains_approx(&u, &a));
        prop_assert!(b.is_empty() || contains_approx(&u, &b));
    }

    #[test]
    fn b8_transform_followed_by_inverse_is_identity(t in transform(), x in -1e3..1e3_f64, y in -1e3..1e3_f64) {
        let inv = t.inverted().expect("generated transforms are invertible");
        assert_identity(&t.followed_by(&inv))?;
        assert_identity(&inv.followed_by(&t))?;
        let p = Point::new(x, y);
        let back = inv.transform_point(t.transform_point(p));
        prop_assert!((back.x - x).abs() <= 1e-9 && (back.y - y).abs() <= 1e-9);
    }
}

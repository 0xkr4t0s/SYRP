//! B8: property tests for algebraic laws.

use proptest::prelude::*;
use syrp_geom::Rect;

fn int_rect() -> impl Strategy<Value = Rect<i32>> {
    (-1000..1000, -1000..1000, -50..500, -50..500).prop_map(|(x, y, w, h)| Rect::new(x, y, w, h))
}

fn float_rect() -> impl Strategy<Value = Rect<f64>> {
    (-1e3..1e3, -1e3..1e3, -10.0..500.0, -10.0..500.0)
        .prop_map(|(x, y, w, h)| Rect::new(x, y, w, h))
}

/// Containment allowing for the last-bit rounding of `x + width` in floating point.
fn contains_approx(outer: &Rect<f64>, inner: &Rect<f64>) -> bool {
    let e = 1e-9;
    inner.x() >= outer.x() - e
        && inner.y() >= outer.y() - e
        && inner.right() <= outer.right() + e
        && inner.bottom() <= outer.bottom() + e
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
}

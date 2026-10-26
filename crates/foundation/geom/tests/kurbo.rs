//! Behaviour tests for B7 (kurbo interop). The crate's dev-dependency on itself enables the
//! `kurbo` feature, so these run under a plain `cargo test`.
#![cfg(feature = "kurbo")]

use syrp_geom::{AffineTransform, Line, Point, Rect};

#[test]
fn b7_points_rects_and_lines_round_trip_through_kurbo() {
    let p = Point::new(1.5, -2.0);
    assert_eq!(kurbo::Point::from(p), kurbo::Point::new(1.5, -2.0));
    assert_eq!(Point::from(kurbo::Point::from(p)), p);
    assert_eq!(Point::from(kurbo::Vec2::from(p)), p);
    let r = Rect::new(1.0, 2.0, 3.0, 4.0);
    let k: kurbo::Rect = r.into();
    assert_eq!((k.x0, k.y0, k.x1, k.y1), (1.0, 2.0, 4.0, 6.0));
    assert_eq!(Rect::from(k), r);
    // An inverted kurbo rect is empty here, as with `Rect::from_ltrb`.
    assert!(Rect::from(kurbo::Rect::new(5.0, 0.0, 1.0, 3.0)).is_empty());
    let l = Line::new(Point::new(0.0, 1.0), Point::new(2.0, 3.0));
    let kl: kurbo::Line = l.into();
    assert_eq!(
        (kl.p0, kl.p1),
        (kurbo::Point::new(0.0, 1.0), kurbo::Point::new(2.0, 3.0))
    );
    assert_eq!(Line::from(kl), l);
}

#[test]
fn b7_affine_transforms_agree_with_kurbo() {
    let t = AffineTransform::rotation(0.7)
        .followed_by(&AffineTransform::shear(0.3, -0.2))
        .followed_by(&AffineTransform::scale(2.0, 0.5))
        .translated(4.0, -9.0);
    let k: kurbo::Affine = t.into();
    for (x, y) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (3.5, -2.25)] {
        let ours = t.transform_point(Point::new(x, y));
        let theirs = k * kurbo::Point::new(x, y);
        assert!((ours.x - theirs.x).abs() < 1e-12 && (ours.y - theirs.y).abs() < 1e-12);
    }
    assert_eq!(AffineTransform::from(k), t);
    // Composition order matches: our `a.followed_by(b)` is kurbo's `b * a`.
    let (a, b) = (
        AffineTransform::translation(1.0, 0.0),
        AffineTransform::scale(3.0, 3.0),
    );
    let kc = kurbo::Affine::from(b) * kurbo::Affine::from(a);
    assert_eq!(AffineTransform::from(kc), a.followed_by(&b));
}

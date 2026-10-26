//! Behaviour tests for B6 (Justification and RectanglePlacement).

use syrp_geom::{Justification as J, Point, Rect, RectanglePlacement as P};

#[test]
fn b6_justification_flags() {
    assert!(
        J::CENTRED.contains(J::HORIZONTALLY_CENTRED) && J::CENTRED.contains(J::VERTICALLY_CENTRED)
    );
    assert_eq!(J::BOTTOM_RIGHT.only_horizontal(), J::RIGHT);
    assert_eq!(J::BOTTOM_RIGHT.only_vertical(), J::BOTTOM);
    assert_eq!(J::LEFT | J::TOP, J::TOP_LEFT);
    assert_eq!(
        J::from_bits(J::CENTRED_TOP.bits() | 1 << 20),
        J::CENTRED_TOP
    );
}

#[test]
fn b6_justification_applied_to_rect_in_both_coordinate_kinds() {
    let area = Rect::new(10, 20, 15, 15);
    let item = Rect::new(0, 0, 10, 10);
    assert_eq!(
        J::TOP_LEFT.applied_to_rect(item, area),
        Rect::new(10, 20, 10, 10)
    );
    assert_eq!(
        J::default().applied_to_rect(item, area),
        Rect::new(10, 20, 10, 10)
    );
    assert_eq!(
        J::BOTTOM_RIGHT.applied_to_rect(item, area),
        Rect::new(15, 25, 10, 10)
    );
    // Integer centring truncates the half-spare (5 / 2 = 2); floats centre exactly.
    assert_eq!(
        J::CENTRED.applied_to_rect(item, area),
        Rect::new(12, 22, 10, 10)
    );
    let f = J::CENTRED.applied_to_rect(item.cast::<f64>(), area.cast());
    assert_eq!(f, Rect::new(12.5, 22.5, 10.0, 10.0));
    // An item larger than the area overhangs it; truncation is towards zero (-5 / 2 = -2).
    let big = Rect::new(0, 0, 20, 20);
    assert_eq!(
        J::CENTRED.applied_to_rect(big, area),
        Rect::new(8, 18, 20, 20)
    );
    assert_eq!(
        J::CENTRED
            .applied_to_rect(big.cast::<f32>(), area.cast())
            .x(),
        7.5
    );
}

#[test]
fn b6_placement_fit_fill_and_stretch() {
    let src = Rect::new(0.0, 0.0, 20.0, 10.0);
    let dst = Rect::new(0.0, 0.0, 100.0, 100.0);
    assert_eq!(
        P::CENTRED.apply_to(src, dst),
        Rect::new(0.0, 25.0, 100.0, 50.0)
    );
    assert_eq!(P::default(), P::CENTRED);
    assert_eq!(
        (P::X_LEFT | P::Y_TOP).apply_to(src, dst),
        Rect::new(0.0, 0.0, 100.0, 50.0)
    );
    assert_eq!(
        (P::X_RIGHT | P::Y_BOTTOM).apply_to(src, dst),
        Rect::new(0.0, 50.0, 100.0, 50.0)
    );
    let fill = P::FILL_DESTINATION.apply_to(src, dst);
    assert_eq!(fill, Rect::new(-50.0, 0.0, 200.0, 100.0));
    assert_eq!(
        (P::FILL_DESTINATION | P::X_LEFT).apply_to(src, dst).x(),
        0.0
    );
    assert_eq!((P::STRETCH_TO_FIT | P::X_LEFT).apply_to(src, dst), dst);
}

#[test]
fn b6_placement_only_reduce_only_increase_and_do_not_resize() {
    let small = Rect::new(0.0, 0.0, 10.0, 10.0);
    let large = Rect::new(0.0, 0.0, 400.0, 400.0);
    let dst = Rect::new(0.0, 0.0, 100.0, 100.0);
    assert_eq!(
        P::ONLY_REDUCE_IN_SIZE.apply_to(small, dst),
        Rect::new(45.0, 45.0, 10.0, 10.0)
    );
    assert_eq!(P::ONLY_REDUCE_IN_SIZE.apply_to(large, dst), dst);
    assert_eq!(P::ONLY_INCREASE_IN_SIZE.apply_to(small, dst), dst);
    assert_eq!(
        P::ONLY_INCREASE_IN_SIZE.apply_to(large, dst),
        Rect::new(-150.0, -150.0, 400.0, 400.0)
    );
    assert_eq!(P::DO_NOT_RESIZE.apply_to(large, dst).width(), 400.0);
    // An empty source is returned unchanged and gives the identity transform.
    let empty = Rect::new(5.0, 5.0, 0.0, 3.0);
    assert_eq!(P::CENTRED.apply_to(empty, dst), empty);
    assert!(P::CENTRED.transform_to_fit(empty, dst).is_identity());
}

#[test]
fn b6_placement_rounds_edges_for_integers_but_not_floats() {
    let dst = Rect::new(0, 0, 10, 10);
    let src = Rect::new(0, 0, 3, 1);
    // Exact result is y = 3.333.. to 6.666..; integer edges round to 3 and 7.
    assert_eq!(P::CENTRED.apply_to(src, dst), Rect::new(0, 3, 10, 4));
    let f = P::CENTRED.apply_to(src.cast::<f64>(), dst.cast());
    assert!((f.y() - 10.0 / 3.0).abs() < 1e-12 && (f.height() - 10.0 / 3.0).abs() < 1e-12);
    // Half-pixel offsets round away from zero on both edges, keeping the size.
    let r = P::DO_NOT_RESIZE.apply_to(Rect::new(0, 0, 10, 10), Rect::new(0, 0, 15, 10));
    assert_eq!(r, Rect::new(3, 0, 10, 10));
    let rf = P::DO_NOT_RESIZE.apply_to(
        Rect::new(0.0_f32, 0.0, 10.0, 10.0),
        Rect::new(0.0, 0.0, 15.0, 10.0),
    );
    assert_eq!(rf, Rect::new(2.5, 0.0, 10.0, 10.0));
}

#[test]
fn b6_transform_to_fit_maps_source_onto_placed_rect() {
    let src = Rect::new(10.0, 10.0, 20.0, 10.0);
    let dst = Rect::new(0.0, 0.0, 100.0, 100.0);
    for p in [
        P::CENTRED,
        P::FILL_DESTINATION | P::X_RIGHT,
        P::STRETCH_TO_FIT,
        P::DO_NOT_RESIZE | P::Y_BOTTOM,
    ] {
        let t = p.transform_to_fit(src, dst);
        assert_eq!(t.transform_rect(src), p.apply_to(src, dst), "{p:?}");
    }
    let t = P::STRETCH_TO_FIT.transform_to_fit(Rect::new(0, 0, 10, 10), Rect::new(5, 5, 20, 40));
    assert_eq!(
        t.transform_point(Point::new(10.0, 10.0)),
        Point::new(25.0, 45.0)
    );
}

#[test]
fn b6_i32_and_f32_paths_agree_up_to_their_documented_rounding() {
    let flags = [
        P::CENTRED,
        P::X_LEFT | P::Y_BOTTOM,
        P::X_RIGHT | P::Y_TOP | P::FILL_DESTINATION,
        P::STRETCH_TO_FIT,
        P::DO_NOT_RESIZE,
        P::ONLY_REDUCE_IN_SIZE | P::X_RIGHT,
    ];
    let sources = [
        Rect::new(3, 1, 7, 3),
        Rect::new(0, 0, 25, 40),
        Rect::new(-4, 2, 11, 11),
    ];
    let dests = [Rect::new(0, 0, 10, 10), Rect::new(5, -3, 33, 17)];
    for p in flags {
        for s in sources {
            for d in dests {
                let exact = p.apply_to(s.cast::<f64>(), d.cast());
                // f32 is the f64 result narrowed (to within f32 precision).
                let f = p.apply_to(s.cast::<f32>(), d.cast());
                let near = |a: f32, b: f64| (f64::from(a) - b).abs() < 1e-4;
                assert!(
                    near(f.x(), exact.x()) && near(f.right(), exact.right()),
                    "{p:?} {s:?} {d:?}"
                );
                assert!(
                    near(f.y(), exact.y()) && near(f.bottom(), exact.bottom()),
                    "{p:?} {s:?} {d:?}"
                );
                // i32 rounds each exact edge to nearest, halves away from zero.
                let r = |v: f64| v.round() as i32;
                let expect = Rect::from_ltrb(
                    r(exact.x()),
                    r(exact.y()),
                    r(exact.right()),
                    r(exact.bottom()),
                );
                assert_eq!(p.apply_to(s, d), expect, "{p:?} {s:?} {d:?}");
                // The transform is exact f64 maths, so it is the same for every source type.
                assert_eq!(
                    p.transform_to_fit(s, d),
                    p.transform_to_fit(s.cast::<f32>(), d.cast())
                );
            }
        }
    }
}

#[test]
fn b6_justification_centring_in_i32_and_f32() {
    let item = Rect::new(0, 0, 4, 3);
    for (area, ix, iy, fx, fy) in [
        (Rect::new(0, 0, 9, 8), 2, 2, 2.5, 2.5), // odd spare: i32 truncates
        (Rect::new(-9, -9, 9, 8), -7, -7, -6.5, -6.5),
        (Rect::new(1, 1, 1, 0), 0, 0, -0.5, -0.5), // item overhangs: truncation towards zero
    ] {
        let i = J::CENTRED.applied_to_rect(item, area);
        assert_eq!((i.x(), i.y()), (ix, iy), "{area:?}");
        let f = J::CENTRED.applied_to_rect(item.cast::<f32>(), area.cast());
        assert_eq!((f.x(), f.y()), (fx, fy), "{area:?}");
        assert_eq!(
            (i.width(), i.height(), f.width(), f.height()),
            (4, 3, 4.0, 3.0)
        );
    }
}

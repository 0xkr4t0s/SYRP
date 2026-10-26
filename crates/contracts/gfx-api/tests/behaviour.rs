// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tests for the behaviours of work/specs/WP-0.2-07.md delivered in this increment (B2-B4, B7).

use syrp_geom::{Point, Rect};
use syrp_gfx_api::{
    Brush, BrushKind, BrushRef, Cap, Colour, FillRule, FontHandle, GradientStop, ImageHandle, Join,
    Path, PathEl, PathVerb, Stroke,
};

fn pt(x: f32, y: f32) -> Point<f32> {
    Point::new(x, y)
}

#[test]
fn b2_colours_and_solid_brushes() {
    let c = Colour::from_rgba(1, 2, 3, 4);
    assert_eq!(c.argb(), 0x0401_0203);
    assert_eq!(Colour::from_argb(c.argb()), c);
    assert_eq!(c.to_rgba(), [1, 2, 3, 4]);
    assert_eq!(Colour::default(), Colour::TRANSPARENT);
    let b = Brush::from(Colour::WHITE);
    assert_eq!(b, Brush::solid(Colour::WHITE));
    assert_eq!(b.as_view().kind(), BrushKind::Solid(Colour::WHITE));
    assert!(b.as_view().stops().is_empty());
    assert_eq!(BrushRef::from(Colour::WHITE), b.as_view());
}

#[test]
fn b2_gradient_kinds_keep_their_geometry() {
    let (p0, p1, p2) = (pt(0.0, 1.0), pt(2.0, 3.0), pt(9.0, 0.0));
    let kind = |b: Brush| b.as_view().kind();
    let linear = kind(Brush::linear(p0, p1, []));
    assert!(matches!(linear, BrushKind::Linear { start, end, .. } if (start, end) == (p0, p1)));
    let radial = kind(Brush::radial(p2, 4.0, []));
    assert!(
        matches!(radial, BrushKind::Radial { centre, radius, .. } if (centre, radius) == (p2, 4.0))
    );
    let conical = kind(Brush::two_point_conical(p0, 1.0, p2, 6.0, []));
    let BrushKind::TwoPointConical {
        start,
        start_radius,
        end,
        end_radius,
        ..
    } = conical
    else {
        panic!("not conical");
    };
    assert_eq!((start, start_radius, end, end_radius), (p0, 1.0, p2, 6.0));
}

#[test]
fn b2_negative_nan_and_negative_zero_radii_become_positive_zero() {
    for r in [-3.0, f32::NAN, -0.0] {
        let BrushKind::Radial { radius, .. } = Brush::radial(pt(0.0, 0.0), r, []).as_view().kind()
        else {
            panic!("not radial");
        };
        // Bit-exact: +0.0, so equal brushes always encode identically.
        assert_eq!(radius.to_bits(), 0.0_f32.to_bits(), "radius {r}");
    }
    let b = Brush::two_point_conical(pt(0.0, 0.0), -1.0, pt(1.0, 0.0), f32::INFINITY, []);
    let BrushKind::TwoPointConical {
        start_radius,
        end_radius,
        ..
    } = b.as_view().kind()
    else {
        panic!("not conical");
    };
    assert_eq!((start_radius, end_radius), (0.0, f32::INFINITY));
}

#[test]
fn b2_stop_offsets_are_clamped_and_sorted_stably() {
    let (a, b, c, d) = (
        Colour::from_argb(1),
        Colour::from_argb(2),
        Colour::from_argb(3),
        Colour::from_argb(4),
    );
    let stops = [
        GradientStop::new(2.0, a),
        GradientStop::new(0.5, b),
        GradientStop::new(-1.0, c),
        GradientStop::new(0.5, d),
    ];
    let brush = Brush::linear(pt(0.0, 0.0), pt(1.0, 0.0), stops);
    let got: Vec<_> = brush
        .as_view()
        .stops()
        .iter()
        .map(|s| (s.offset(), s.colour()))
        .collect();
    // Equal offsets keep insertion order (b before d): a hard colour edge.
    assert_eq!(got, [(0.0, c), (0.5, b), (0.5, d), (1.0, a)]);
    assert_eq!(GradientStop::new(f32::NAN, a).offset(), 0.0);
    assert_eq!(GradientStop::new(f32::INFINITY, a).offset(), 1.0);
}

#[test]
fn b3_path_records_verbs_and_points() {
    let mut p = Path::new();
    p.move_to(0.0, 0.0);
    p.line_to(1.0, 0.0);
    p.quad_to(2.0, 0.0, 2.0, 1.0);
    p.cubic_to(2.0, 2.0, 1.0, 3.0, 0.0, 3.0);
    p.close();
    use PathVerb::*;
    assert_eq!(
        p.as_view().verbs(),
        [MoveTo, LineTo, QuadTo, CubicTo, Close]
    );
    let n: usize = p.as_view().verbs().iter().map(|v| v.point_count()).sum();
    assert_eq!(p.as_view().points().len(), n);
    let els: Vec<_> = elements_of(&p).collect();
    assert_eq!(els[2], PathEl::QuadTo(pt(2.0, 0.0), pt(2.0, 1.0)));
    assert_eq!(
        els[3],
        PathEl::CubicTo(pt(2.0, 2.0), pt(1.0, 3.0), pt(0.0, 3.0))
    );
    assert_eq!(els[4], PathEl::Close);
}

#[test]
fn b3_drawing_without_a_subpath_starts_one_at_the_current_point() {
    let mut p = Path::new();
    p.line_to(4.0, 0.0); // empty path: starts at the origin
    p.close();
    p.close(); // no open subpath: ignored
    p.line_to(0.0, 4.0); // after close: starts at the closed subpath's start
    let els: Vec<_> = elements_of(&p).collect();
    assert_eq!(
        els,
        [
            PathEl::MoveTo(pt(0.0, 0.0)),
            PathEl::LineTo(pt(4.0, 0.0)),
            PathEl::Close,
            PathEl::MoveTo(pt(0.0, 0.0)),
            PathEl::LineTo(pt(0.0, 4.0)),
        ]
    );
    let mut q = Path::new();
    q.move_to(5.0, 5.0);
    q.close();
    q.cubic_to(6.0, 6.0, 7.0, 7.0, 8.0, 8.0);
    assert_eq!(
        q.as_view().elements().nth(2),
        Some(PathEl::MoveTo(pt(5.0, 5.0)))
    );
}

/// `elements` must not capture the borrow of the view itself (edition 2024 capture rules).
fn elements_of(path: &Path) -> impl Iterator<Item = PathEl> + '_ {
    path.as_view().elements()
}

#[test]
fn b3_bounds_cover_all_points_including_control_points() {
    let mut p = Path::new();
    assert_eq!(p.bounds(), None);
    p.move_to(-1.0, 2.0);
    p.quad_to(5.0, -10.0, 10.0, 2.0);
    assert_eq!(p.bounds(), Some(Rect::from_ltrb(-1.0, -10.0, 10.0, 2.0)));
    p.line_to(f32::NAN, 100.0); // NaN points are skipped
    assert_eq!(p.bounds(), Some(Rect::from_ltrb(-1.0, -10.0, 10.0, 2.0)));
    assert_eq!(p.as_view().bounds(), p.bounds());
}

#[test]
fn b3_fill_rule_and_clear() {
    let mut p = Path::new();
    p.set_fill_rule(FillRule::EvenOdd);
    p.move_to(3.0, 3.0);
    p.line_to(4.0, 4.0);
    p.clear();
    assert!(p.is_empty() && p.as_view().points().is_empty());
    assert_eq!(p.as_view().fill_rule(), FillRule::EvenOdd);
    p.line_to(1.0, 0.0); // clear also resets the current point to the origin
    assert_eq!(p.as_view().points()[0], pt(0.0, 0.0));
}

#[test]
fn b3_stroke_parameters_and_sanitising() {
    let s = Stroke::new(2.0);
    let r = s.as_view();
    assert_eq!(
        (r.width(), r.join(), r.cap(), r.miter_limit()),
        (2.0, Join::Miter, Cap::Butt, 4.0)
    );
    assert!(r.dashes().is_empty());
    let s = Stroke::new(f32::NAN)
        .with_join(Join::Bevel)
        .with_cap(Cap::Square)
        .with_miter_limit(-2.0)
        .with_dashes([5.0, -1.0, f32::NAN, 2.5], f32::INFINITY);
    let r = s.as_view();
    assert_eq!(
        (r.width(), r.join(), r.cap(), r.miter_limit()),
        (0.0, Join::Bevel, Cap::Square, 0.0)
    );
    assert_eq!(r.dashes(), [5.0, 0.0, 0.0, 2.5]);
    assert_eq!(r.dash_offset(), 0.0);
    let s = s.with_dashes([1.0], -3.0); // replaces; negative offsets are allowed
    assert_eq!(
        (s.as_view().dashes(), s.as_view().dash_offset()),
        (&[1.0][..], -3.0)
    );
}

#[test]
fn b4_handles_are_opaque_ids() {
    let (a, b) = (ImageHandle::from_raw(1), ImageHandle::from_raw(u64::MAX));
    assert_eq!((a.raw(), b.raw()), (1, u64::MAX));
    assert!(a < b && a != b);
    assert_eq!(FontHandle::from_raw(9), FontHandle::from_raw(9));
    assert_eq!(FontHandle::from_raw(9).raw(), 9);
}

/// B7 freezes the public API (the snapshot is committed by the PR manager after review);
/// ADR 0003 D3 requires frozen contract types to have no public fields.
#[test]
fn b7_contract_types_have_no_public_fields() {
    let src = [
        include_str!("../src/lib.rs"),
        include_str!("../src/path.rs"),
        include_str!("../src/brush.rs"),
        include_str!("../src/list.rs"),
        include_str!("../src/render.rs"),
        include_str!("../src/encode.rs"),
    ];
    let mut in_struct = false;
    for line in src.iter().flat_map(|s| s.lines()) {
        if line.starts_with("pub struct") {
            assert!(!line.contains("(pub"), "public tuple field: {line}");
            in_struct = line.ends_with('{');
        } else if line.starts_with('}') {
            in_struct = false;
        } else if in_struct {
            assert!(
                !line.trim_start().starts_with("pub"),
                "public field: {line}"
            );
        }
    }
}

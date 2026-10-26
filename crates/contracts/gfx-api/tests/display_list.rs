// SPDX-License-Identifier: MIT OR Apache-2.0
//! B1 and B6 tests (work/specs/WP-0.2-07.md): recording, reading back, hashing and JSON.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use syrp_geom::{AffineTransform, Point, Rect, RectanglePlacement};
use syrp_gfx_api::{
    BlendMode, Brush, BrushKind, Colour, DisplayList, FillRule, FontHandle, Glyph, GradientStop,
    ImageHandle, Op, Path, PathVerb, ResamplingQuality, Stroke,
};

thread_local! {
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

/// Counts allocations made on the current thread, so parallel tests do not interfere.
struct Counting;

// SAFETY: forwards every call unchanged to the system allocator.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|n| n.set(n.get() + 1));
        // SAFETY: the caller upholds `GlobalAlloc::alloc`'s contract, which we pass on.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` came from `System` through this allocator with this layout.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|n| n.set(n.get() + 1));
        // SAFETY: as for `dealloc`; the caller upholds `realloc`'s size contract.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn allocations_during(f: impl FnOnce()) -> usize {
    let before = ALLOCATIONS.with(Cell::get);
    f();
    ALLOCATIONS.with(Cell::get) - before
}

fn triangle() -> Path {
    let mut p = Path::new();
    p.move_to(0.0, 0.0);
    p.line_to(10.0, 0.0);
    p.quad_to(10.0, 10.0, 0.0, 10.0);
    p.close();
    p
}

fn gradient() -> Brush {
    let stops = [
        GradientStop::new(0.0, Colour::BLACK),
        GradientStop::new(1.0, Colour::WHITE),
    ];
    Brush::linear(Point::new(0.0, 0.0), Point::new(10.0, 0.0), stops)
}

/// Records one op of every kind.
fn record_all(list: &mut DisplayList, path: &Path, brush: &Brush, stroke: &Stroke, run: &[Glyph]) {
    list.push_transform(AffineTransform::translation(1.0, 2.0));
    list.push_clip_rect(Rect::new(0, 0, 100, 100));
    list.push_clip_path(path);
    list.push_clip_mask(ImageHandle::from_raw(5), AffineTransform::scale(2.0, 2.0));
    list.push_layer(0.5, BlendMode::Multiply);
    list.fill_rect(Rect::new(1.0_f32, 2.0, 3.0, 4.0), brush);
    list.fill_path(path, Colour::WHITE);
    list.stroke_path(path, stroke, brush);
    let dest = Rect::new(0, 0, 32, 16);
    list.draw_image(
        ImageHandle::from_raw(7),
        dest,
        RectanglePlacement::CENTRED,
        ResamplingQuality::High,
    );
    list.draw_glyphs(FontHandle::from_raw(3), 14.0, run, Colour::BLACK);
    list.pop_all();
}

#[test]
fn b1_ops_read_back_in_recording_order_as_borrowed_views() {
    let (path, brush) = (triangle(), gradient());
    let stroke = Stroke::new(2.0).with_dashes([4.0, 2.0], 1.0);
    let run = [
        Glyph::new(36, Point::new(1.0, 20.0)),
        Glyph::new(37, Point::new(9.0, 20.0)),
    ];
    let mut list = DisplayList::new();
    record_all(&mut list, &path, &brush, &stroke, &run);
    drop((path, brush, stroke)); // the list holds copies in its arenas
    let ops: Vec<Op<'_>> = list.ops().collect();
    assert_eq!((ops.len(), list.len(), list.ops().len()), (15, 15, 15));
    assert_eq!(list.get(0), Some(ops[0]));
    assert_eq!(list.get(15), None);
    let Op::PushTransform { transform, .. } = ops[0] else {
        panic!("{:?}", ops[0])
    };
    assert_eq!(transform, AffineTransform::translation(1.0, 2.0));
    assert!(
        matches!(ops[1], Op::PushClipRect { rect, .. } if rect == Rect::new(0.0, 0.0, 100.0, 100.0))
    );
    let Op::PushClipPath { path, .. } = ops[2] else {
        panic!("{:?}", ops[2])
    };
    assert_eq!(path, triangle().as_view());
    let Op::PushClipMask {
        image, transform, ..
    } = ops[3]
    else {
        panic!("{:?}", ops[3])
    };
    assert_eq!(
        (image.raw(), transform),
        (5, AffineTransform::scale(2.0, 2.0))
    );
    assert!(matches!(
        ops[4],
        Op::PushLayer {
            opacity: 0.5,
            blend: BlendMode::Multiply,
            ..
        }
    ));
    let Op::FillRect { rect, brush, .. } = ops[5] else {
        panic!("{:?}", ops[5])
    };
    assert_eq!(
        (rect, brush),
        (Rect::new(1.0, 2.0, 3.0, 4.0), gradient().as_view())
    );
    let Op::FillPath { path, brush, .. } = ops[6] else {
        panic!("{:?}", ops[6])
    };
    assert_eq!(
        (path.verbs().len(), brush.kind()),
        (4, BrushKind::Solid(Colour::WHITE))
    );
    let Op::StrokePath {
        path,
        stroke,
        brush,
        ..
    } = ops[7]
    else {
        panic!("{:?}", ops[7])
    };
    assert_eq!(path.points().len(), 4);
    assert_eq!(
        (stroke.width(), stroke.dashes(), stroke.dash_offset()),
        (2.0, &[4.0, 2.0][..], 1.0)
    );
    assert_eq!(brush.stops().len(), 2);
    let Op::DrawImage {
        image,
        dest,
        placement,
        quality,
        ..
    } = ops[8]
    else {
        panic!("{:?}", ops[8])
    };
    assert_eq!((image.raw(), dest), (7, Rect::new(0.0, 0.0, 32.0, 16.0)));
    assert_eq!(
        (placement, quality),
        (RectanglePlacement::CENTRED, ResamplingQuality::High)
    );
    let Op::DrawGlyphs {
        font,
        size,
        glyphs,
        brush,
        ..
    } = ops[9]
    else {
        panic!("{:?}", ops[9])
    };
    assert_eq!(
        (font.raw(), size, glyphs, brush.kind()),
        (3, 14.0, &run[..], BrushKind::Solid(Colour::BLACK))
    );
    assert!(ops[10..].iter().all(|op| *op == Op::Pop));
}

#[test]
fn b1_paths_brushes_and_glyphs_share_the_list_arenas() {
    let (a, mut b) = (triangle(), Path::new());
    b.set_fill_rule(FillRule::EvenOdd);
    b.line_to(5.0, 5.0);
    let mut list = DisplayList::new();
    list.fill_path(&a, &gradient());
    list.push_clip_path(&b);
    list.fill_path(&a, &gradient());
    let paths: Vec<_> = list
        .ops()
        .filter_map(|op| match op {
            Op::FillPath { path, .. } | Op::PushClipPath { path, .. } => Some(path),
            _ => None,
        })
        .collect();
    // Consecutive paths sit back to back in one verb arena and one point arena.
    for w in paths.windows(2) {
        assert_eq!(w[0].points().as_ptr_range().end, w[1].points().as_ptr());
        assert_eq!(w[0].verbs().as_ptr_range().end, w[1].verbs().as_ptr());
    }
    assert_eq!(paths[1].verbs(), [PathVerb::MoveTo, PathVerb::LineTo]);
    assert_eq!(paths[1].fill_rule(), FillRule::EvenOdd);
    let stops = |i| match list.get(i) {
        Some(Op::FillPath { brush, .. }) => brush.stops(),
        other => panic!("{other:?}"),
    };
    assert_eq!(stops(0).as_ptr_range().end, stops(2).as_ptr());
}

#[test]
fn b1_rectangles_in_integer_and_float_coordinates_are_stored_as_f32() {
    let mut list = DisplayList::new();
    list.fill_rect(Rect::new(1, 2, 3, 4), Colour::BLACK); // small integers: exact
    list.fill_rect(Rect::new(0.1_f64, 0.0, 1.0, 1.0), Colour::BLACK); // rounded to nearest f32
    list.push_clip_rect(Rect::new(16_777_217, 0, 1, 1)); // above 2^24: nearest f32
    let rect = |i| match list.get(i) {
        Some(Op::FillRect { rect, .. } | Op::PushClipRect { rect, .. }) => rect,
        other => panic!("{other:?}"),
    };
    assert_eq!(rect(0), Rect::new(1.0, 2.0, 3.0, 4.0));
    assert_eq!(rect(1).x().to_bits(), 0.1_f32.to_bits());
    assert_eq!(rect(2).x(), 16_777_216.0);
}

#[test]
fn b1_pop_closes_the_innermost_scope_and_refuses_underflow() {
    let mut list = DisplayList::new();
    assert!(!list.pop());
    assert!(list.is_empty());
    list.push_transform(AffineTransform::IDENTITY);
    list.push_layer(1.0, BlendMode::Normal);
    assert_eq!(list.open_scopes(), 2);
    assert!(list.pop());
    assert_eq!(list.open_scopes(), 1);
    list.pop_all();
    assert!(!list.pop());
    assert_eq!((list.open_scopes(), list.len()), (0, 4));
}

#[test]
fn b1_layer_opacity_and_glyph_size_are_clamped() {
    let mut list = DisplayList::new();
    for o in [-1.0, 2.0, f32::NAN, -0.0] {
        list.push_layer(o, BlendMode::Screen);
    }
    list.draw_glyphs(FontHandle::from_raw(1), -5.0, &[], Colour::BLACK);
    let got: Vec<u32> = list
        .ops()
        .map(|op| match op {
            Op::PushLayer { opacity, .. } => opacity.to_bits(),
            Op::DrawGlyphs { size, .. } => size.to_bits(),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(got, [0.0_f32, 1.0, 0.0, 0.0, 0.0].map(f32::to_bits));
    assert_eq!(BlendMode::default(), BlendMode::Normal);
    assert_eq!(ResamplingQuality::default(), ResamplingQuality::Medium);
}

#[test]
fn b1_clear_keeps_capacity_so_rerecording_allocates_nothing() {
    let (path, brush) = (triangle(), gradient());
    let stroke = Stroke::new(1.0).with_dashes([3.0, 1.0], 0.0);
    let run = [Glyph::new(1, Point::new(0.0, 0.0)); 8];
    let mut list = DisplayList::new();
    record_all(&mut list, &path, &brush, &stroke, &run);
    let first = list.clone();
    let n = allocations_during(|| {
        list.clear();
        assert!(list.is_empty() && list.open_scopes() == 0);
        record_all(&mut list, &path, &brush, &stroke, &run);
    });
    assert_eq!(n, 0, "re-recording the same shape allocated");
    assert_eq!(list, first);
    // The counter works: a fresh list does allocate.
    assert!(
        allocations_during(|| record_all(&mut DisplayList::new(), &path, &brush, &stroke, &run))
            > 0
    );
}

/// Every op kind, with fixed values (the golden for the hash).
fn sample() -> DisplayList {
    let stroke = Stroke::new(2.0).with_dashes([4.0, 2.0], 1.0);
    let run = [Glyph::new(36, Point::new(1.0, 20.0))];
    let mut list = DisplayList::new();
    record_all(&mut list, &triangle(), &gradient(), &stroke, &run);
    list
}

#[test]
fn b6_same_ops_hash_the_same_whatever_the_recording_history() {
    let fresh = sample();
    // A list that grew bigger, was cleared (leaving spare capacity and stale arena bytes),
    // and then recorded the same ops.
    let mut reused = DisplayList::new();
    let mut big = triangle();
    big.cubic_to(1.0, 2.0, 3.0, 4.0, 5.0, 6.0);
    for _ in 0..50 {
        reused.fill_path(&big, &gradient());
        reused.draw_glyphs(
            FontHandle::from_raw(9),
            3.0,
            &[Glyph::new(1, Point::origin()); 9],
            Colour::WHITE,
        );
    }
    reused.clear();
    let s = sample();
    for op in s.ops() {
        // Re-record op by op through the public views, in another arena order.
        match op {
            Op::Pop => assert!(reused.pop()),
            _ => record_op(&mut reused, op),
        }
    }
    assert_eq!(reused.content_hash(), fresh.content_hash());
    assert_eq!(
        serde_json::to_value(&reused).unwrap(),
        serde_json::to_value(&fresh).unwrap()
    );
    assert_eq!(fresh.clone().content_hash(), fresh.content_hash());
}

fn record_op(list: &mut DisplayList, op: Op<'_>) {
    match op {
        Op::FillRect { rect, brush, .. } => list.fill_rect(rect, brush),
        Op::FillPath { path, brush, .. } => list.fill_path(path, brush),
        Op::StrokePath {
            path,
            stroke,
            brush,
            ..
        } => list.stroke_path(path, stroke, brush),
        Op::DrawImage {
            image,
            dest,
            placement,
            quality,
            ..
        } => list.draw_image(image, dest, placement, quality),
        Op::DrawGlyphs {
            font,
            size,
            glyphs,
            brush,
            ..
        } => list.draw_glyphs(font, size, glyphs, brush),
        Op::PushClipRect { rect, .. } => list.push_clip_rect(rect),
        Op::PushClipPath { path, .. } => list.push_clip_path(path),
        Op::PushClipMask {
            image, transform, ..
        } => list.push_clip_mask(image, transform),
        Op::PushTransform { transform, .. } => list.push_transform(transform),
        Op::PushLayer { opacity, blend, .. } => list.push_layer(opacity, blend),
        other => panic!("{other:?}"),
    }
}

#[test]
fn b6_any_change_in_content_or_order_changes_the_hash() {
    let one = |f: &dyn Fn(&mut DisplayList)| {
        let mut l = DisplayList::new();
        f(&mut l);
        l.content_hash()
    };
    let r = Rect::new(0, 0, 4, 4);
    let base = one(&|l| l.fill_rect(r, Colour::BLACK));
    let mut tri = triangle();
    let variants = [
        one(&|l| l.fill_rect(r, Colour::WHITE)),
        one(&|l| l.fill_rect(Rect::new(0, 0, 4, 5), Colour::BLACK)),
        one(&|l| l.fill_rect(r, &Brush::radial(Point::origin(), 1.0, []))),
        one(&|l| {
            l.fill_rect(r, Colour::BLACK);
            l.fill_rect(r, Colour::BLACK);
        }),
        one(&|l| l.fill_path(&triangle(), Colour::BLACK)),
        one(&|l| l.push_clip_rect(r)),
        one(&|_| ()),
    ];
    for (i, v) in variants.iter().enumerate() {
        assert_ne!(*v, base, "variant {i}");
    }
    // Order of ops and the path's fill rule both count.
    let ab = one(&|l| {
        l.push_layer(1.0, BlendMode::Normal);
        l.fill_rect(r, Colour::BLACK);
    });
    let ba = one(&|l| {
        l.fill_rect(r, Colour::BLACK);
        l.push_layer(1.0, BlendMode::Normal);
    });
    assert_ne!(ab, ba);
    let nonzero = one(&|l| l.fill_path(&tri, Colour::BLACK));
    tri.set_fill_rule(FillRule::EvenOdd);
    assert_ne!(one(&|l| l.fill_path(&tri, Colour::BLACK)), nonzero);
}

#[test]
fn b6_floats_hash_by_bit_pattern() {
    let hash = |x: f32| {
        let mut l = DisplayList::new();
        l.fill_rect(Rect::new(x, 0.0, 1.0, 1.0), Colour::BLACK);
        l.content_hash()
    };
    assert_ne!(hash(0.0), hash(-0.0));
    let (nan_a, nan_b) = (f32::from_bits(0x7fc0_0000), f32::from_bits(0x7fc0_0001));
    assert_eq!(hash(nan_a), hash(f32::from_bits(nan_a.to_bits())));
    assert_ne!(hash(nan_a), hash(nan_b));
    // Integer and float rects that convert to the same f32 values hash the same.
    let mut a = DisplayList::new();
    a.fill_rect(Rect::new(1, 2, 3, 4), Colour::BLACK);
    let mut b = DisplayList::new();
    b.fill_rect(Rect::new(1.0_f64, 2.0, 3.0, 4.0), Colour::BLACK);
    assert_eq!(a.content_hash(), b.content_hash());
}

#[test]
fn b6_hash_is_fixed_and_seedless() {
    // 64-bit FNV-1a: the empty list hashes to the offset basis; the golden below must never
    // change across runs, builds or platforms (a change is a contract change).
    assert_eq!(DisplayList::new().content_hash(), 0xcbf2_9ce4_8422_2325);
    assert_eq!(sample().content_hash(), GOLDEN);
}

const GOLDEN: u64 = 0x9edd_b817_af66_fcef;

#[test]
fn b6_serde_json_lists_ops_in_order() {
    let json = serde_json::to_value(sample()).unwrap();
    let ops = json.as_array().unwrap();
    assert_eq!(ops.len(), sample().len());
    let kinds: Vec<&str> = ops.iter().map(|o| o["op"].as_str().unwrap()).collect();
    assert_eq!(
        kinds[..6],
        [
            "push_transform",
            "push_clip_rect",
            "push_clip_path",
            "push_clip_mask",
            "push_layer",
            "fill_rect"
        ]
    );
    assert_eq!(
        ops[0]["transform"],
        serde_json::json!([1.0, 0.0, 1.0, 0.0, 1.0, 2.0])
    );
    assert_eq!(
        ops[2]["path"]["verbs"],
        serde_json::json!(["move_to", "line_to", "quad_to", "close"])
    );
    assert_eq!(ops[4]["blend"], "multiply");
    assert_eq!(ops[5]["brush"]["kind"], "linear");
    assert_eq!(
        ops[5]["brush"]["stops"][1],
        serde_json::json!([1.0, 0xffff_ffff_u32])
    );
    assert_eq!(ops[7]["dashes"], serde_json::json!([4.0, 2.0]));
    assert_eq!(ops[8]["quality"], "high");
    assert_eq!(ops[9]["glyphs"], serde_json::json!([[36, [1.0, 20.0]]]));
    assert_eq!(kinds[10], "pop");
    let mut nan = DisplayList::new();
    nan.push_transform(AffineTransform::translation(f64::NAN, 0.0));
    assert_eq!(
        serde_json::to_value(&nan).unwrap()[0]["transform"][2],
        serde_json::Value::Null
    );
}

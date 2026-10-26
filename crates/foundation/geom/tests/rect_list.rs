//! Behaviour tests for B3 (RectList), including a model-based property test on a pixel grid.

use proptest::prelude::*;
use syrp_geom::{Point, Rect, RectList};

/// Panics unless the list is canonical: non-empty, non-overlapping, nothing left to merge.
fn assert_canonical<T: syrp_geom::Coord>(list: &RectList<T>) {
    let rects: Vec<_> = list.iter().copied().collect();
    for (i, a) in rects.iter().enumerate() {
        assert!(!a.is_empty(), "empty rect {a:?}");
        for b in &rects[i + 1..] {
            assert!(!a.intersects(b), "{a:?} overlaps {b:?}");
            let u = a.union(b);
            let area = |r: &Rect<T>| r.width().to_f64() * r.height().to_f64();
            assert!(
                area(&u) != area(a) + area(b),
                "{a:?} and {b:?} should merge"
            );
        }
    }
}

#[test]
fn b3_add_merges_adjacent_and_never_overlaps() {
    let mut r = RectList::new();
    r.add(Rect::new(0, 0, 10, 10));
    r.add(Rect::new(10, 0, 10, 10));
    r.add(Rect::new(0, 10, 20, 5));
    assert_eq!(
        r.iter().copied().collect::<Vec<_>>(),
        [Rect::new(0, 0, 20, 15)]
    );
    r.add(Rect::new(15, 10, 10, 10)); // overlaps the corner
    assert_canonical(&r);
    assert!(
        r.contains_rect(&Rect::new(15, 10, 10, 10)) && r.contains_rect(&Rect::new(0, 0, 20, 15))
    );
    r.add(Rect::new(3, 3, 0, 50)); // empty: ignored
    assert_eq!(r.bounds(), Rect::new(0, 0, 25, 20));
}

#[test]
fn b3_subtract_clip_bounds_and_contains() {
    let mut r = RectList::from_rect(Rect::new(0.0, 0.0, 10.0, 10.0));
    r.subtract(Rect::new(2.5, 2.5, 5.0, 5.0));
    assert_canonical(&r);
    assert_eq!(r.len(), 4);
    assert!(!r.contains_point(Point::new(5.0, 5.0)) && r.contains_point(Point::new(1.0, 5.0)));
    assert!(r.contains_rect(&Rect::new(0.0, 0.0, 10.0, 2.5)));
    assert!(!r.contains_rect(&Rect::new(0.0, 0.0, 10.0, 2.6)));
    assert!(r.intersects_rect(&Rect::new(7.0, 7.0, 1.0, 1.0)));
    assert!(!r.intersects_rect(&Rect::new(3.0, 3.0, 1.0, 1.0)));
    r.clip_to(&Rect::new(0.0, 0.0, 10.0, 2.0));
    assert_eq!(
        r.iter().copied().collect::<Vec<_>>(),
        [Rect::new(0.0, 0.0, 10.0, 2.0)]
    );
    r.subtract(Rect::new(-1.0, -1.0, 20.0, 20.0));
    assert!(r.is_empty() && r.bounds() == Rect::default());
}

#[test]
fn b3_contains_rect_spanning_several_rects_in_both_coordinate_kinds() {
    let mut i = RectList::from_rect(Rect::new(0, 0, 5, 5));
    i.add(Rect::new(5, 0, 5, 3));
    assert!(i.contains_rect(&Rect::new(3, 0, 4, 3)) && !i.contains_rect(&Rect::new(3, 0, 4, 4)));
    let mut f = RectList::from_rect(Rect::new(0.0_f32, 0.0, 0.5, 0.5));
    f.add(Rect::new(0.5, 0.0, 0.5, 0.25));
    assert!(f.contains_rect(&Rect::new(0.25, 0.0, 0.5, 0.25)));
    assert!(!f.contains_rect(&Rect::new(0.25, 0.0, 0.5, 0.3)));
}

/// How a grid cell's new state follows from its old state and whether the op's rect covers it.
type Combine = fn(bool, bool) -> bool;

#[derive(Clone, Debug)]
enum Op {
    Add(Rect<i32>),
    Subtract(Rect<i32>),
    Clip(Rect<i32>),
}

fn small_rect() -> impl Strategy<Value = Rect<i32>> {
    (0..16, 0..16, -2..10, -2..10).prop_map(|(x, y, w, h)| Rect::new(x, y, w, h))
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        3 => small_rect().prop_map(Op::Add),
        2 => small_rect().prop_map(Op::Subtract),
        1 => small_rect().prop_map(Op::Clip),
    ]
}

proptest! {
    /// B3 (with B8): after any sequence of operations the list covers exactly the pixels a
    /// boolean grid model covers, and is canonical.
    #[test]
    fn b3_matches_a_pixel_model_and_stays_canonical(ops in prop::collection::vec(op(), 1..25)) {
        let mut list = RectList::new();
        let mut grid = [[false; 26]; 26];
        for op in &ops {
            let (r, f): (&Rect<i32>, Combine) = match op {
                Op::Add(r) => { list.add(*r); (r, |old, inside| old || inside) }
                Op::Subtract(r) => { list.subtract(*r); (r, |old, inside| old && !inside) }
                Op::Clip(r) => { list.clip_to(r); (r, |old, inside| old && inside) }
            };
            for (y, row) in grid.iter_mut().enumerate() {
                for (x, cell) in row.iter_mut().enumerate() {
                    *cell = f(*cell, r.contains_point(Point::new(x as i32, y as i32)));
                }
            }
        }
        assert_canonical(&list);
        let mut bounds = Rect::default();
        for (y, row) in grid.iter().enumerate() {
            for (x, cell) in row.iter().enumerate() {
                let p = Point::new(x as i32, y as i32);
                prop_assert_eq!(list.contains_point(p), *cell, "pixel {:?}", p);
                if *cell {
                    bounds = bounds.union(&Rect::new(p.x, p.y, 1, 1));
                }
            }
        }
        prop_assert_eq!(list.bounds(), bounds);
    }
}

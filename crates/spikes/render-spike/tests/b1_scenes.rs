// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B1: the shared scene set, built once from `DisplayList`s.

use syrp_gfx_api::{BrushKind, BrushRef, Op};
use syrp_render_spike::{SCALES, scenes};

#[derive(Default)]
struct Families {
    fill_path: bool,
    stroke: bool,
    linear: bool,
    radial: bool,
    conical: bool,
    glyphs: bool,
    image: bool,
    clip_rect: bool,
    clip_path: bool,
    transform: bool,
    layer: bool,
}

fn gradient(f: &mut Families, brush: BrushRef<'_>) {
    match brush.kind() {
        BrushKind::Linear { .. } => f.linear = true,
        BrushKind::Radial { .. } => f.radial = true,
        BrushKind::TwoPointConical { .. } => f.conical = true,
        _ => {}
    }
}

#[test]
fn b1_twenty_named_scenes_cover_every_feature_family() {
    let all = scenes();
    assert_eq!(all.len(), 20);
    let mut names: Vec<_> = all.iter().map(|s| s.name()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), 20, "scene names must be unique");

    let mut f = Families::default();
    for scene in &all {
        assert!(!scene.list().is_empty(), "{} is empty", scene.name());
        for op in scene.list().ops() {
            match op {
                Op::FillRect { brush, .. } => gradient(&mut f, brush),
                Op::FillPath { brush, .. } => {
                    f.fill_path = true;
                    gradient(&mut f, brush);
                }
                Op::StrokePath { .. } => f.stroke = true,
                Op::DrawGlyphs { brush, .. } => {
                    f.glyphs = true;
                    gradient(&mut f, brush);
                }
                Op::DrawImage { .. } => f.image = true,
                Op::PushClipRect { .. } => f.clip_rect = true,
                Op::PushClipPath { .. } => f.clip_path = true,
                Op::PushTransform { .. } => f.transform = true,
                Op::PushLayer { .. } => f.layer = true,
                _ => {}
            }
        }
    }
    let Families {
        fill_path,
        stroke,
        linear,
        radial,
        conical,
        glyphs,
        image,
        clip_rect,
        clip_path,
        transform,
        layer,
    } = f;
    assert!(fill_path && stroke && linear && radial && conical && glyphs);
    assert!(image && clip_rect && clip_path && transform && layer);
}

#[test]
fn b1_scene_lists_are_reproducible() {
    let (a, b) = (scenes(), scenes());
    for (x, y) in a.iter().zip(&b) {
        assert_eq!(
            x.list().content_hash(),
            y.list().content_hash(),
            "{}",
            x.name()
        );
    }
}

#[test]
fn b1_targets_cover_the_canvas_at_every_scale() {
    let scene = &scenes()[0];
    let (w, h) = scene.logical_size();
    for scale in SCALES {
        let t = scene.target(scale).unwrap();
        assert_eq!(
            (t.width(), t.height(), t.scale()),
            ((w * scale).ceil() as u32, (h * scale).ceil() as u32, scale)
        );
        assert!(t.pixels().unwrap().iter().all(|&b| b == 0));
    }
}

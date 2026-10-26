// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B1: both backends render the shared scene set at every scale.

use syrp_geom::{Rect, RectanglePlacement};
use syrp_gfx_api::{
    Colour, DisplayList, FontHandle, Glyph, ImageHandle, RenderError, RenderTarget,
    ResamplingQuality,
};
use syrp_render_spike::{Backend, SCALES, Tolerance, compare, scenes};

/// The one known semantic divergence: a gradient towards a transparent stop. tiny-skia
/// interpolates unpremultiplied colour, vello_cpu premultiplied, so the two disagree across half
/// the ramp (recorded in ADR 0004).
const DIVERGENT: &str = "gradient_linear";

fn render(backend: Backend, scene: &syrp_render_spike::Scene, scale: f64) -> RenderTarget {
    let mut t = scene.target(scale).unwrap();
    backend.renderer().render(scene.list(), &mut t).unwrap();
    t
}

#[test]
fn b1_both_backends_render_every_scene_at_every_scale() {
    for scene in scenes() {
        for scale in SCALES {
            for backend in Backend::ALL {
                let t = render(backend, &scene, scale);
                let px = t.pixels().unwrap();
                let painted = px.chunks_exact(4).filter(|p| p[3] != 0).count();
                let who = format!("{} {} @{scale}", backend.name(), scene.name());
                assert!(painted * 25 > px.len() / 4, "{who}: under 4% painted");
                let premul = px
                    .chunks_exact(4)
                    .all(|p| p[..3].iter().all(|&c| c <= p[3]));
                assert!(premul, "{who}: not premultiplied");
            }
        }
    }
}

#[test]
fn b1_backends_agree_within_a_cross_renderer_tolerance() {
    let loose = Tolerance::new(32, 0.02);
    for scene in scenes() {
        for scale in SCALES {
            let v = render(Backend::VelloCpu, &scene, scale);
            let s = render(Backend::TinySkia, &scene, scale);
            let r = compare(&v, &s, loose).unwrap();
            let who = format!("{} @{scale}: {:.4}", scene.name(), r.differing_ratio());
            assert_eq!(r.passed(), scene.name() != DIVERGENT, "{who}");
        }
    }
}

#[test]
fn b1_unknown_handles_and_clip_masks_are_errors() {
    let mut t = RenderTarget::with_pixels(8, 8, 1.0).unwrap();
    let bad_image = ImageHandle::from_raw(99);
    let bad_font = FontHandle::from_raw(99);
    for backend in Backend::ALL {
        let mut list = DisplayList::new();
        let fit = RectanglePlacement::CENTRED;
        list.draw_image(
            bad_image,
            Rect::new(0, 0, 8, 8),
            fit,
            ResamplingQuality::Low,
        );
        let err = backend.renderer().render(&list, &mut t).unwrap_err();
        assert_eq!(err, RenderError::UnknownImage(bad_image));

        list.clear();
        let run = [Glyph::new(0, Default::default())];
        list.draw_glyphs(bad_font, 10.0, &run, Colour::BLACK);
        let err = backend.renderer().render(&list, &mut t).unwrap_err();
        assert_eq!(err, RenderError::UnknownFont(bad_font));

        list.clear();
        list.push_clip_mask(ImageHandle::from_raw(1), Default::default());
        let err = backend.renderer().render(&list, &mut t).unwrap_err();
        assert!(matches!(err, RenderError::Backend(_)));
    }
}

#[test]
fn b1_scopes_left_open_are_closed_at_the_end() {
    let mut list = DisplayList::new();
    list.push_layer(0.5, Default::default());
    list.push_clip_rect(Rect::new(0, 0, 4, 8));
    list.fill_rect(Rect::new(0, 0, 8, 8), Colour::WHITE);
    for backend in Backend::ALL {
        let mut t = RenderTarget::with_pixels(8, 8, 1.0).unwrap();
        backend.renderer().render(&list, &mut t).unwrap();
        let px = t.pixels().unwrap();
        assert!(
            (126..=129).contains(&px[3]),
            "{}: {}",
            backend.name(),
            px[3]
        );
        assert_eq!(px[4 * 6 + 3], 0, "{}: clip ignored", backend.name());
    }
}

#[test]
fn b1_a_second_render_draws_over_the_first() {
    let mut first = DisplayList::new();
    first.fill_rect(Rect::new(0, 0, 2, 2), Colour::from_argb(0xffff_0000));
    let mut second = DisplayList::new();
    second.fill_rect(Rect::new(4, 4, 2, 2), Colour::from_argb(0xff00_00ff));
    second.fill_rect(Rect::new(1, 1, 2, 2), Colour::from_argb(0x8000_ff00));
    for backend in Backend::ALL {
        let mut t = RenderTarget::with_pixels(8, 8, 1.0).unwrap();
        let mut r = backend.renderer();
        r.render(&first, &mut t).unwrap();
        r.render(&second, &mut t).unwrap();
        let px = |x: usize, y: usize| t.pixels().unwrap()[(y * 8 + x) * 4..][..4].to_vec();
        let who = backend.name();
        assert_eq!(px(0, 0), [255, 0, 0, 255], "{who}: first render lost");
        assert_eq!(px(4, 4), [0, 0, 255, 255], "{who}: second render missing");
        let [r, g, b, a] = px(1, 1)[..] else {
            unreachable!()
        };
        assert!(a == 255 && b == 0, "{who}: overlap {:?}", px(1, 1));
        assert!(
            (126..=129).contains(&r) && (126..=129).contains(&g),
            "{who}: not src-over"
        );
    }
}

#[test]
fn b1_odd_dash_lists_repeat_instead_of_stroking_solid() {
    let mut line = syrp_gfx_api::Path::new();
    line.move_to(0.0, 2.0);
    line.line_to(32.0, 2.0);
    for (dashes, expected) in [
        (vec![3.0], "###...###...###...###...###...##"),
        (vec![3.0, 1.0, 2.0], "###.##...#..###.##...#..###.##.."),
    ] {
        let stroke = syrp_gfx_api::Stroke::new(2.0).with_dashes(dashes.clone(), 0.0);
        let mut list = DisplayList::new();
        list.stroke_path(&line, &stroke, Colour::BLACK);
        for backend in Backend::ALL {
            let mut t = RenderTarget::with_pixels(32, 4, 1.0).unwrap();
            backend.renderer().render(&list, &mut t).unwrap();
            let row = &t.pixels().unwrap()[32 * 4 * 2..][..32 * 4];
            let seen: String = row
                .chunks_exact(4)
                .map(|p| if p[3] > 127 { '#' } else { '.' })
                .collect();
            assert_eq!(seen, expected, "{} {dashes:?}", backend.name());
        }
    }
}

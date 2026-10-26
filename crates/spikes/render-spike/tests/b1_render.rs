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

// SPDX-License-Identifier: MIT OR Apache-2.0
//! B5 tests (work/specs/WP-0.2-07.md): `RenderTarget`, `Renderer` and `RenderError`.

use syrp_error::Diagnostic;
use syrp_geom::Rect;
use syrp_gfx_api::{
    Colour, DisplayList, FontHandle, ImageHandle, Op, RenderError, RenderTarget, Renderer,
};

#[test]
fn b5_integer_physical_sizes_are_exact_and_logical_size_is_fractional() {
    let t = RenderTarget::new(3, 5, 2.0).unwrap();
    assert_eq!((t.width(), t.height(), t.scale()), (3, 5, 2.0));
    assert_eq!(t.logical_size(), (1.5, 2.5));
    assert!(t.pixels().is_none());
    let t = RenderTarget::new(0, 0, 1.25).unwrap(); // a minimised window is legal
    assert_eq!(t.logical_size(), (0.0, 0.0));
}

#[test]
fn b5_logical_sizes_round_up_to_cover_the_area_but_snap_float_noise() {
    let size = |w: f64, s: f64| {
        let t = RenderTarget::from_logical(w, w, s, false).unwrap();
        (t.width(), t.height())
    };
    assert_eq!(size(100.0, 1.25), (125, 125)); // exact product
    assert_eq!(size(101.0, 1.25), (127, 127)); // 126.25: round up
    assert_eq!(size(10.0, 1.1), (11, 11)); // 11.000000000000002: noise, not 12
    assert_eq!(size(0.3, 3.0), (1, 1)); // 0.8999999999999999: round up (the snap only matters above a whole number)
    assert_eq!(size(10.5, 1.5), (16, 16)); // 15.75: round up
    assert_eq!(size(7.0, 1.0), (7, 7));
    assert_eq!(size(0.0, 2.0), (0, 0));
}

#[test]
fn b5_cpu_targets_own_a_cleared_rgba8_buffer() {
    let mut t = RenderTarget::from_logical(2.0, 1.0, 2.0, true).unwrap();
    assert_eq!((t.width(), t.height()), (4, 2));
    assert_eq!(t.pixels().unwrap().len(), 4 * 2 * 4);
    t.pixels_mut().unwrap()[0] = 9;
    t.resize(3, 3, 1.5).unwrap();
    assert_eq!(t.pixels().unwrap(), [0; 36]);
    assert_eq!((t.logical_size(), t.scale()), ((2.0, 2.0), 1.5));
    let before = t.clone();
    assert_eq!(t.resize(1, 1, f64::NAN), Err(RenderError::InvalidTarget));
    assert_eq!(t, before); // a failed resize changes nothing
}

#[test]
fn b5_bad_scales_and_sizes_are_rejected_with_a_code() {
    for scale in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let e = RenderTarget::new(1, 1, scale).unwrap_err();
        assert_eq!(e.code().as_str(), "SYRP-GFX-0001", "scale {scale}");
    }
    for w in [-1.0, f64::NAN, f64::INFINITY, 5e9] {
        assert_eq!(
            RenderTarget::from_logical(w, 1.0, 1.0, false),
            Err(RenderError::InvalidTarget)
        );
    }
    // Each side fits u32, but the RGBA buffer is too large to address on any platform.
    assert_eq!(
        RenderTarget::with_pixels(u32::MAX, u32::MAX, 1.0),
        Err(RenderError::InvalidTarget)
    );
    let codes = [
        RenderError::UnknownImage(ImageHandle::from_raw(1)),
        RenderError::UnknownFont(FontHandle::from_raw(1)),
        RenderError::Backend("device lost".into()),
    ]
    .map(|e| (e.code().as_str(), e.to_string()));
    assert_eq!(codes[0], ("SYRP-GFX-0002", "unknown image handle 1".into()));
    assert_eq!(codes[1].0, "SYRP-GFX-0003");
    assert_eq!(
        codes[2],
        ("SYRP-GFX-0004", "render backend failed: device lost".into())
    );
}

/// A renderer that fills solid rects into a CPU target, scaled, and knows no images.
struct SolidRects;

impl Renderer for SolidRects {
    fn render(&mut self, list: &DisplayList, target: &mut RenderTarget) -> Result<(), RenderError> {
        let (w, s) = (target.width() as usize, target.scale());
        for op in list.ops() {
            match op {
                Op::FillRect { rect, brush, .. } => {
                    let syrp_gfx_api::BrushKind::Solid(c) = brush.kind() else {
                        continue;
                    };
                    let px = |v: f32| (f64::from(v) * s).round() as usize;
                    let pixels = target.pixels_mut().ok_or(RenderError::InvalidTarget)?;
                    for y in px(rect.y())..px(rect.bottom()) {
                        for x in px(rect.x())..px(rect.right()) {
                            pixels[(y * w + x) * 4..][..4].copy_from_slice(&c.to_rgba());
                        }
                    }
                }
                Op::DrawImage { image, .. } => return Err(RenderError::UnknownImage(image)),
                _ => {}
            }
        }
        Ok(())
    }
}

#[test]
fn b5_renderers_apply_the_target_scale_and_report_errors() {
    let mut list = DisplayList::new();
    list.fill_rect(Rect::new(1, 0, 1, 1), Colour::WHITE);
    let mut target = RenderTarget::with_pixels(4, 2, 2.0).unwrap();
    let r: &mut dyn Renderer = &mut SolidRects;
    r.render(&list, &mut target).unwrap();
    let lit: Vec<bool> = target
        .pixels()
        .unwrap()
        .chunks(4)
        .map(|p| p[3] == 255)
        .collect();
    // Logical x 1..2 is physical 2..4 at scale 2, on both rows.
    assert_eq!(lit, [false, false, true, true, false, false, true, true]);
    let (image, dest) = (ImageHandle::from_raw(42), Rect::new(0, 0, 1, 1));
    list.draw_image(image, dest, Default::default(), Default::default());
    assert_eq!(
        r.render(&list, &mut target),
        Err(RenderError::UnknownImage(image))
    );
}

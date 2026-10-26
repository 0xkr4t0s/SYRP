// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B4: the golden comparator.

use syrp_gfx_api::{RenderError, RenderTarget};
use syrp_render_spike::{Tolerance, compare};

fn target(w: u32, h: u32) -> RenderTarget {
    RenderTarget::with_pixels(w, h, 1.0).unwrap()
}

/// Sets channel `ch` of pixel `i` to `v`.
fn poke(t: &mut RenderTarget, i: usize, ch: usize, v: u8) {
    t.pixels_mut().unwrap()[i * 4 + ch] = v;
}

#[test]
fn b4_identical_images_pass_exactly() {
    let mut g = target(4, 3);
    poke(&mut g, 5, 3, 200);
    let r = compare(&g, &g.clone(), Tolerance::EXACT).unwrap();
    assert_eq!((r.differing_pixels(), r.max_channel_delta()), (0, 0));
    assert_eq!(r.differing_ratio(), 0.0);
    assert!(r.passed());
}

#[test]
fn b4_per_pixel_tolerance_is_inclusive_on_every_channel() {
    let g = target(4, 1);
    let mut a = g.clone();
    poke(&mut a, 0, 0, 3); // red: exactly at the tolerance
    poke(&mut a, 1, 3, 4); // alpha: one past it
    poke(&mut a, 2, 2, 250); // blue: far past it
    let r = compare(&a, &g, Tolerance::new(3, 1.0)).unwrap();
    assert_eq!(r.differing_pixels(), 2);
    assert_eq!(r.max_channel_delta(), 250);
    assert_eq!(r.differing_ratio(), 0.5);
}

#[test]
fn b4_max_differing_pixel_ratio_decides_the_verdict() {
    let g = target(10, 10);
    let mut a = g.clone();
    for i in 0..5 {
        poke(&mut a, i, 1, 100);
    }
    assert!(compare(&a, &g, Tolerance::new(0, 0.05)).unwrap().passed());
    assert!(!compare(&a, &g, Tolerance::new(0, 0.049)).unwrap().passed());
    assert!(compare(&a, &g, Tolerance::new(100, 0.0)).unwrap().passed());
    assert_eq!(Tolerance::new(0, f64::NAN).max_ratio(), 0.0);
    assert_eq!(Tolerance::new(0, -1.0).max_ratio(), 0.0);
}

#[test]
fn b4_mismatched_or_bufferless_targets_are_rejected() {
    let a = target(4, 4);
    let err = compare(&a, &target(4, 5), Tolerance::EXACT).unwrap_err();
    assert_eq!(err, RenderError::InvalidTarget);
    let surface = RenderTarget::new(4, 4, 1.0).unwrap();
    assert_eq!(
        compare(&surface, &a, Tolerance::EXACT).unwrap_err(),
        RenderError::InvalidTarget
    );
}

#[test]
fn b4_diff_png_marks_differing_pixels_red() {
    let mut g = target(3, 2);
    for i in 0..6 {
        poke(&mut g, i, 3, 255);
    }
    let mut a = g.clone();
    poke(&mut a, 4, 0, 255);
    let report = compare(&a, &g, Tolerance::new(1, 0.0)).unwrap();
    let png_bytes = report.diff_png().unwrap();

    let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!((info.width, info.height), (3, 2));
    assert_eq!(info.color_type, png::ColorType::Rgba);
    let px: Vec<&[u8]> = buf.chunks_exact(4).collect();
    assert_eq!(px[4], [255, 0, 0, 255]);
    for (i, p) in px.iter().enumerate().filter(|(i, _)| *i != 4) {
        assert!(
            p[0] == p[1] && p[1] == p[2] && p[3] == 255,
            "pixel {i} should be grey: {p:?}"
        );
    }
}

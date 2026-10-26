// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The 20 spike scenes, each a `DisplayList` on a 120 x 90 logical canvas.

use syrp_geom::{AffineTransform, Point, Rect, RectanglePlacement};
use syrp_gfx_api::{
    BlendMode, Brush, Cap, Colour, DisplayList, FillRule, FontHandle, Glyph, GradientStop,
    ImageHandle, Join, Path, RenderError, RenderTarget, ResamplingQuality, Stroke,
};

/// An opaque photo-like image: smooth colour ramps plus seeded noise.
pub(crate) const PHOTO: ImageHandle = ImageHandle::from_raw(1);
/// A soft-edged disc on transparency (premultiplied).
pub(crate) const SPRITE: ImageHandle = ImageHandle::from_raw(2);
/// The synthetic outline font.
pub(crate) const OUTLINES: FontHandle = FontHandle::from_raw(1);

/// The device scales every scene is rendered at.
/// ```
/// assert_eq!(syrp_render_spike::SCALES, [1.0, 1.25, 1.5, 2.0]);
/// ```
pub const SCALES: [f64; 4] = [1.0, 1.25, 1.5, 2.0];

const W: f64 = 120.0;
const H: f64 = 90.0;

/// One named spike scene: a display list on a logical canvas.
/// ```
/// let scenes = syrp_render_spike::scenes();
/// assert_eq!(scenes[0].name(), "rects_solid");
/// assert!(!scenes[0].list().is_empty());
/// ```
#[derive(Clone, Debug)]
pub struct Scene {
    name: &'static str,
    list: DisplayList,
}

impl Scene {
    /// The scene's stable name (used in reports and golden file names).
    /// ```
    /// assert_eq!(syrp_render_spike::scenes()[19].name(), "layer_blend");
    /// ```
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// The recorded ops.
    /// ```
    /// assert!(syrp_render_spike::scenes()[1].list().len() > 0);
    /// ```
    pub fn list(&self) -> &DisplayList {
        &self.list
    }

    /// The logical canvas size in user units (the same for every scene).
    /// ```
    /// assert_eq!(syrp_render_spike::scenes()[0].logical_size(), (120.0, 90.0));
    /// ```
    pub fn logical_size(&self) -> (f64, f64) {
        (W, H)
    }

    /// A transparent CPU target covering the canvas at `scale`.
    /// ```
    /// let t = syrp_render_spike::scenes()[0].target(1.5)?;
    /// assert_eq!((t.width(), t.height()), (180, 135));
    /// # Ok::<(), syrp_gfx_api::RenderError>(())
    /// ```
    pub fn target(&self, scale: f64) -> Result<RenderTarget, RenderError> {
        RenderTarget::from_logical(W, H, scale, true)
    }
}

/// Appends a circle of four cubics, clockwise on screen unless `reverse`.
pub(crate) fn circle(p: &mut Path, cx: f32, cy: f32, r: f32, reverse: bool) {
    const K: f32 = 0.552_284_8;
    let s = if reverse { -1.0 } else { 1.0 };
    let k = K * r;
    p.move_to(cx + r, cy);
    p.cubic_to(cx + r, cy + s * k, cx + k, cy + s * r, cx, cy + s * r);
    p.cubic_to(cx - k, cy + s * r, cx - r, cy + s * k, cx - r, cy);
    p.cubic_to(cx - r, cy - s * k, cx - k, cy - s * r, cx, cy - s * r);
    p.cubic_to(cx + k, cy - s * r, cx + r, cy - s * k, cx + r, cy);
    p.close();
}

/// Appends a closed polygon.
pub(crate) fn polygon(p: &mut Path, pts: &[(f32, f32)]) {
    for (i, &(x, y)) in pts.iter().enumerate() {
        if i == 0 {
            p.move_to(x, y);
        } else {
            p.line_to(x, y);
        }
    }
    p.close();
}

fn c(argb: u32) -> Colour {
    Colour::from_argb(argb)
}

fn stops(cols: &[(f32, u32)]) -> Vec<GradientStop> {
    cols.iter()
        .map(|&(o, k)| GradientStop::new(o, c(k)))
        .collect()
}

fn pt(x: f32, y: f32) -> Point<f32> {
    Point::new(x, y)
}

fn star(cx: f32, cy: f32, outer: f32, inner: f32, n: u32, rule: FillRule) -> Path {
    let mut p = Path::new();
    let step = std::f32::consts::PI / n as f32;
    let pts: Vec<_> = (0..2 * n)
        .map(|i| {
            let r = if i % 2 == 0 { outer } else { inner };
            let a = i as f32 * step - std::f32::consts::FRAC_PI_2;
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect();
    polygon(&mut p, &pts);
    p.set_fill_rule(rule);
    p
}

/// A pentagram drawn as one self-crossing contour, so fill rules differ.
fn pentagram(rule: FillRule) -> Path {
    let mut p = Path::new();
    let pts: Vec<_> = (0..5)
        .map(|i| {
            let a = (i * 2 % 5) as f32 * 2.0 * std::f32::consts::PI / 5.0 - 1.570_796_4;
            (60.0 + 40.0 * a.cos(), 47.0 + 40.0 * a.sin())
        })
        .collect();
    polygon(&mut p, &pts);
    p.set_fill_rule(rule);
    p
}

fn disc(cx: f32, cy: f32, r: f32) -> Path {
    let mut p = Path::new();
    circle(&mut p, cx, cy, r, false);
    p
}

fn text(list: &mut DisplayList, s: &str, x: f32, y: f32, size: f32, brush: &Brush) {
    let glyphs: Vec<_> = s
        .bytes()
        .enumerate()
        .map(|(i, b)| {
            let id = b"olvax".iter().position(|&g| g == b).unwrap_or(0) as u32;
            Glyph::new(id, pt(x + i as f32 * 0.62 * size, y))
        })
        .collect();
    list.draw_glyphs(OUTLINES, size, &glyphs, brush);
}

fn backdrop(l: &mut DisplayList) {
    let b = Brush::linear(
        pt(0.0, 0.0),
        pt(120.0, 90.0),
        stops(&[(0.0, 0xff20_4080), (1.0, 0xffe0_c060)]),
    );
    l.fill_rect(Rect::new(0.0, 0.0, 120.0, 90.0), &b);
}

type Build = fn(&mut DisplayList);

const SCENES: [(&str, Build); 20] = [
    ("rects_solid", |l| {
        l.fill_rect(Rect::new(4.0, 4.0, 60.0, 40.0), c(0xffd0_3030));
        l.fill_rect(Rect::new(30.5, 20.25, 60.0, 40.0), c(0x8030_60d0));
        l.fill_rect(Rect::new(70.3, 60.7, 45.1, 25.6), c(0xff20_a040));
    }),
    ("path_star_nonzero", |l| {
        l.fill_path(&pentagram(FillRule::NonZero), c(0xff30_3090))
    }),
    ("path_star_evenodd", |l| {
        l.fill_path(&pentagram(FillRule::EvenOdd), c(0xff90_3030))
    }),
    ("path_cubic_blob", |l| {
        let mut p = Path::new();
        p.move_to(10.0, 45.0);
        p.cubic_to(10.0, -10.0, 110.0, 10.0, 100.0, 45.0);
        p.cubic_to(95.0, 95.0, 40.0, 70.0, 60.0, 50.0);
        p.quad_to(20.0, 100.0, 10.0, 45.0);
        p.close();
        l.fill_path(&p, c(0xff10_8070));
    }),
    ("stroke_joins_caps", |l| {
        let mut p = Path::new();
        polygon(&mut p, &[(10.0, 70.0), (30.0, 15.0), (50.0, 70.0)]);
        let mut open = Path::new();
        open.move_to(65.0, 70.0);
        open.line_to(85.0, 15.0);
        open.line_to(105.0, 70.0);
        l.stroke_path(&p, &Stroke::new(8.0).with_join(Join::Miter), c(0xff40_4040));
        let round = Stroke::new(8.0).with_join(Join::Round).with_cap(Cap::Round);
        l.stroke_path(&open, &round, c(0xffc0_6020));
        let bevel = Stroke::new(3.0)
            .with_join(Join::Bevel)
            .with_cap(Cap::Square);
        l.stroke_path(&open, &bevel, c(0xff20_20c0));
    }),
    ("stroke_dashes", |l| {
        let dashed = Stroke::new(3.0).with_dashes([8.0, 4.0, 2.0, 4.0], 3.0);
        l.stroke_path(&disc(60.0, 45.0, 35.0), &dashed, c(0xff60_2080));
        let fine = Stroke::new(1.0).with_dashes([3.0, 3.0], 0.0);
        l.stroke_path(
            &star(60.0, 45.0, 25.0, 10.0, 6, FillRule::NonZero),
            &fine,
            Colour::BLACK,
        );
    }),
    ("stroke_hairlines", |l| {
        for i in 0..12 {
            let mut p = Path::new();
            let x = 5.0 + i as f32 * 9.5;
            p.move_to(x, 5.0);
            p.line_to(x + 20.0, 85.0);
            let w = [0.25, 0.5, 1.0][i % 3];
            l.stroke_path(&p, &Stroke::new(w), Colour::BLACK);
        }
    }),
    ("gradient_linear", |l| {
        let b = Brush::linear(
            pt(10.0, 0.0),
            pt(110.0, 0.0),
            stops(&[(0.0, 0xffff_0000), (0.5, 0xff00_ff00), (1.0, 0x0000_00ff)]),
        );
        l.fill_rect(Rect::new(0.0, 0.0, 120.0, 90.0), &b);
    }),
    ("gradient_radial", |l| {
        let b = Brush::radial(
            pt(60.0, 45.0),
            40.0,
            stops(&[(0.0, 0xffff_ffff), (0.7, 0xff30_60c0), (1.0, 0x0030_60c0)]),
        );
        l.fill_rect(Rect::new(0.0, 0.0, 120.0, 90.0), &b);
    }),
    ("gradient_conical", |l| {
        let b = Brush::two_point_conical(
            pt(40.0, 40.0),
            5.0,
            pt(70.0, 50.0),
            40.0,
            stops(&[(0.0, 0xffff_e000), (1.0, 0xff80_0040)]),
        );
        l.fill_path(&disc(65.0, 48.0, 38.0), &b);
    }),
    ("gradient_hard_stop_path", |l| {
        let b = Brush::linear(
            pt(20.0, 10.0),
            pt(100.0, 80.0),
            stops(&[
                (0.0, 0xff00_8080),
                (0.5, 0xff00_8080),
                (0.5, 0xfff0_a000),
                (1.0, 0xff80_2000),
            ]),
        );
        l.fill_path(&star(60.0, 45.0, 42.0, 18.0, 7, FillRule::NonZero), &b);
    }),
    ("glyphs_solid", |l| {
        let ink = Brush::solid(c(0xff10_1010));
        text(l, "olvax", 6.0, 30.0, 20.0, &ink);
        text(l, "axolotl", 6.0, 55.0, 12.0, &ink);
        text(l, "lava olla vox", 6.0, 80.0, 7.0, &ink);
    }),
    ("glyphs_gradient_rotated", |l| {
        backdrop(l);
        l.push_transform(AffineTransform::rotation_about(-0.2, 60.0, 45.0));
        let b = Brush::linear(
            pt(0.0, 30.0),
            pt(0.0, 70.0),
            stops(&[(0.0, 0xffff_ffff), (1.0, 0xffff_d040)]),
        );
        text(l, "volvox", 10.0, 60.0, 24.0, &b);
        l.pop();
    }),
    ("image_stretched", |l| {
        let fit = RectanglePlacement::STRETCH_TO_FIT;
        l.draw_image(
            PHOTO,
            Rect::new(5.0, 5.0, 110.0, 80.0),
            fit,
            ResamplingQuality::Medium,
        );
    }),
    ("image_nearest_rotated", |l| {
        l.push_transform(AffineTransform::rotation_about(0.3, 60.0, 45.0));
        let fit = RectanglePlacement::STRETCH_TO_FIT;
        l.draw_image(
            PHOTO,
            Rect::new(30.0, 15.0, 60.0, 60.0),
            fit,
            ResamplingQuality::Low,
        );
        l.pop();
    }),
    ("image_alpha_centred", |l| {
        backdrop(l);
        let place = RectanglePlacement::CENTRED;
        l.draw_image(
            SPRITE,
            Rect::new(10.0, 10.0, 100.0, 70.0),
            place,
            ResamplingQuality::High,
        );
        l.draw_image(
            SPRITE,
            Rect::new(2.0, 60.0, 24.0, 24.0),
            place,
            ResamplingQuality::Medium,
        );
    }),
    ("clip_rects_nested", |l| {
        l.push_clip_rect(Rect::new(10.5, 10.5, 100.0, 70.0));
        backdrop(l);
        l.push_transform(AffineTransform::rotation_about(0.4, 60.0, 45.0));
        l.push_clip_rect(Rect::new(30.0, 25.0, 60.0, 40.0));
        l.fill_rect(Rect::new(0.0, 0.0, 120.0, 90.0), c(0xc0ff_ffff));
        l.pop_all();
    }),
    ("clip_path_mixed", |l| {
        l.push_clip_path(&star(60.0, 45.0, 44.0, 20.0, 5, FillRule::NonZero));
        let fit = RectanglePlacement::FILL_DESTINATION;
        l.draw_image(
            PHOTO,
            Rect::new(0.0, 0.0, 120.0, 90.0),
            fit,
            ResamplingQuality::Medium,
        );
        l.push_clip_path(&disc(60.0, 45.0, 22.0));
        l.fill_rect(Rect::new(0.0, 0.0, 120.0, 90.0), c(0x8000_0000));
        l.pop_all();
    }),
    ("layer_opacity", |l| {
        backdrop(l);
        l.push_layer(0.5, BlendMode::Normal);
        l.fill_path(&disc(45.0, 45.0, 30.0), c(0xffff_0000));
        l.fill_path(&disc(75.0, 45.0, 30.0), c(0xff00_00ff));
        l.pop();
    }),
    ("layer_blend", |l| {
        backdrop(l);
        l.push_layer(1.0, BlendMode::Multiply);
        l.fill_path(&disc(45.0, 40.0, 28.0), c(0xffff_8040));
        l.pop();
        l.push_layer(0.8, BlendMode::Screen);
        l.push_clip_rect(Rect::new(50.0, 20.0, 60.0, 60.0));
        l.fill_path(&disc(75.0, 50.0, 28.0), c(0xff40_80ff));
        l.pop_all();
    }),
];

/// The 20 spike scenes, in report order.
/// ```
/// let s = syrp_render_spike::scenes();
/// assert_eq!(s.len(), 20);
/// assert!(s.iter().any(|s| s.name() == "glyphs_solid"));
/// ```
pub fn scenes() -> Vec<Scene> {
    SCENES
        .iter()
        .map(|&(name, build)| {
            let mut list = DisplayList::new();
            build(&mut list);
            Scene { name, list }
        })
        .collect()
}

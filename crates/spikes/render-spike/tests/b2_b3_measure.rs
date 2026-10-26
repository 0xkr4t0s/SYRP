// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B2 determinism runs, B3 per-scene timing, and the B5 report generator behind ADR 0004.

use syrp_gfx_api::Renderer;
use syrp_render_spike::{
    Backend, SCALES, Tolerance, VelloCpuRenderer, VelloMode, check_determinism, compare,
    fingerprint, scenes, time_scene,
};

const RUNS: usize = 10;

#[test]
fn b2_ten_runs_give_identical_bytes_on_both_backends() {
    for scene in scenes() {
        for scale in SCALES {
            for backend in Backend::ALL {
                let d = check_determinism(|| backend.renderer(), &scene, scale, RUNS).unwrap();
                assert_eq!(d.runs(), RUNS);
                let who = format!("{} {} @{scale}", backend.name(), scene.name());
                assert!(
                    d.is_deterministic(),
                    "{who}: {} outputs",
                    d.distinct_outputs()
                );
            }
        }
    }
}

#[test]
fn b2_one_renderer_reused_ten_times_gives_identical_bytes() {
    for backend in Backend::ALL {
        let mut r = backend.renderer();
        for scene in scenes() {
            let prints: Vec<_> = (0..RUNS)
                .map(|_| {
                    let mut t = scene.target(1.25).unwrap();
                    r.render(scene.list(), &mut t).unwrap();
                    fingerprint(&t)
                })
                .collect();
            assert!(
                prints.windows(2).all(|w| w[0] == w[1]),
                "{} {}",
                backend.name(),
                scene.name()
            );
        }
    }
}

#[test]
fn b2_vello_simd_levels_differ_by_at_most_one_unit() {
    // vello_cpu's output depends on the SIMD level (measured: 11 of 80 renders differ by one
    // unit between SSE2 and AVX2), which is why the deterministic mode pins the level.
    for scene in scenes() {
        let mut base = scene.target(1.0).unwrap();
        let mut native = scene.target(1.0).unwrap();
        VelloCpuRenderer::new(VelloMode::Deterministic)
            .render(scene.list(), &mut base)
            .unwrap();
        VelloCpuRenderer::new(VelloMode::NativeSimd)
            .render(scene.list(), &mut native)
            .unwrap();
        let r = compare(&native, &base, Tolerance::new(1, 0.0)).unwrap();
        assert!(
            r.passed(),
            "{}: {:.4} of pixels off by more than 1",
            scene.name(),
            r.differing_ratio()
        );
    }
}

/// x86_64 reference for `VelloMode::Deterministic`: each scene's `content_hash` (its input) and
/// the `fingerprint` of its render at each of `SCALES`.
#[rustfmt::skip]
const X86_64: [(&str, u64, [u64; 4]); 20] = [
    ("rects_solid", 0x56ee57c6b4a022f5, [0x20a7335ff7bfa711, 0x7948a379baeaf35d, 0x22b0bd62c915a7c1, 0x95d86f56152e1375]),
    ("path_star_nonzero", 0xa79fc66c9bf018f6, [0x956a25314ce1cf85, 0xe6df881f9fa2c35d, 0x3d734b2359a6669d, 0xd8e8f3ec8a9a340d]),
    ("path_star_evenodd", 0x9bc5ffd2c0f4d023, [0x502b2dd5812bae71, 0xa1fa8da2dcd740b1, 0x19d8a9f757083475, 0xafcd8aea6f982785]),
    ("path_cubic_blob", 0xd04d80105a989d77, [0x60bd1eb86e5784b1, 0x7cbda883ef26b01c, 0xb865ea7dfbdf42d3, 0x36e09641723f40da]),
    ("stroke_joins_caps", 0x127d6b83d6b15c82, [0xcd9a837923a7c621, 0x646b2379c1d09ed6, 0x97c8d86df9838746, 0xc1ee973f110a52b9]),
    ("stroke_dashes", 0xe5e91f4ec4bae9a1, [0x9cb189a5ae826879, 0x5ec6459ea88c17b1, 0x1da4c8697784be47, 0x3654da47b0728950]),
    ("stroke_hairlines", 0xfb02254ed2bfa688, [0x06971ad78e1714f5, 0x92bb2613c0a8ae0c, 0x5d8a682399bc6fc7, 0xe2177c73338df9a3]),
    ("gradient_linear", 0xa4dd6d673a220a7d, [0xeba365c4a4e65e5d, 0xbcff4b323e6a4db9, 0xf9e69bff97dd6e77, 0x8ee9850be6e214f5]),
    ("gradient_radial", 0x8ab8684a3f209952, [0xce10aa28011ac47d, 0x08e8f81796725821, 0xbe1f328a311034ed, 0xeebec586f34940bd]),
    ("gradient_conical", 0x0d2c7426e3e2376e, [0x1c63f956282658f5, 0xe0c9b8ab146a67ac, 0x6e27a3e5bca111a2, 0x79ed30f7a8d6d8bc]),
    ("gradient_hard_stop_path", 0x1ab8a2e5113c2198, [0x86aff778f8fe97d5, 0x8889c9ba01b7d193, 0xe72082788cb523c6, 0x5feb4c72de233196]),
    ("glyphs_solid", 0x09b74d108a1d234c, [0x9fb2cd98cb03202e, 0x3cb3dde55ac0eb7f, 0xde8bfefb6b30b8e2, 0x5ab61b04b944267c]),
    ("glyphs_gradient_rotated", 0x8bcb37436f3267e1, [0x6fb2c9de843d2fd1, 0xe64458cb2dad2476, 0x3c35b7b8a0990a08, 0x84209550d37bb59b]),
    ("image_stretched", 0x6fcc9d4661d075ca, [0xfd7444a282831c59, 0xe542241df67bcf4b, 0x7ed2a0c4d56e604e, 0x80e76c9923ea62a1]),
    ("image_nearest_rotated", 0xb6244731a04740fc, [0x7a2f8ca4b6d574ec, 0x1d73e9a732e70ca0, 0x4114041772d0e485, 0x7955c1c93d68b189]),
    ("image_alpha_centred", 0x2ee62ddb0a716f16, [0x33c4f35c6d018c82, 0x3c7b8ae768574985, 0x9122774b0af41e32, 0xa59a794031b21284]),
    ("clip_rects_nested", 0xd0f9cd9a067c5420, [0x6952f771247ee173, 0x894aa5455fb688c2, 0xdcb6c71bd2ca9c84, 0x464db35ddcb2813d]),
    ("clip_path_mixed", 0xd4ec124898d34cb2, [0xd54a75049f088725, 0x6c0d95c211e2f490, 0xdd40b61d21e4f39e, 0xfff2ea2b68e71996]),
    ("layer_opacity", 0x7bfdebb88378f342, [0x5fa997c77d93752e, 0x4547c89b1d000705, 0xc01c7f570cfc311a, 0xcd93b1214362a8d9]),
    ("layer_blend", 0xae84af5f8d03e434, [0x811a8f3d896173a9, 0x33b1a2984b1f5ac7, 0xceb4c88ed286dd25, 0x3ba42207b76039f4]),
];

fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in bytes.chunks(3) {
        let n = c
            .iter()
            .enumerate()
            .fold(0_u32, |n, (i, &b)| n | u32::from(b) << (16 - 8 * i));
        for i in 0..=c.len() {
            out.push(ABC[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

#[test]
fn b2_vello_deterministic_renders_match_the_x86_64_fingerprints() {
    let (mut matched, mut dumps) = (0, Vec::new());
    for (scene, (name, input, prints)) in scenes().iter().zip(X86_64) {
        assert_eq!(
            (scene.name(), scene.list().content_hash()),
            (name, input),
            "scene input"
        );
        for (scale, print) in SCALES.into_iter().zip(prints) {
            let mut t = scene.target(scale).unwrap();
            VelloCpuRenderer::new(VelloMode::Deterministic)
                .render(scene.list(), &mut t)
                .unwrap();
            if fingerprint(&t) == print {
                matched += 1;
                continue;
            }
            let mut png_bytes = Vec::new();
            let mut enc = png::Encoder::new(&mut png_bytes, t.width(), t.height());
            enc.set_color(png::ColorType::Rgba);
            enc.write_header()
                .unwrap()
                .write_image_data(t.pixels().unwrap())
                .unwrap();
            dumps.push(format!("DUMP {name} {scale} {}", base64(&png_bytes)));
        }
    }
    let arch = std::env::consts::ARCH;
    assert!(
        dumps.is_empty(),
        "{}\nRESULT {arch}: {matched}/80 match",
        dumps.join("\n")
    );
}

#[test]
fn b3_every_scene_is_timed_on_both_backends() {
    for backend in Backend::ALL {
        let mut r = backend.renderer();
        for scene in scenes() {
            let t = time_scene(r.as_mut(), &scene, 2.0, 3).unwrap();
            assert_eq!(t.runs(), 3);
            assert!(t.min() > std::time::Duration::ZERO && t.min() <= t.median());
        }
    }
}

/// Prints the tables in ADR 0004. Run in release:
/// `cargo test -p syrp-render-spike --release --test b2_b3_measure -- --ignored --nocapture`
#[test]
#[ignore = "report generator for ADR 0004; timing needs a release build"]
fn b5_print_adr_report() {
    println!("| scene | vello 1x | skia 1x | vello 2x | skia 2x | differing @1x (tol 8 / 32) |");
    println!("|---|---:|---:|---:|---:|---:|");
    let mut totals = [[0.0; 4]; 2];
    for scene in scenes() {
        let mut cells = Vec::new();
        for scale in [1.0, 2.0] {
            for (i, backend) in Backend::ALL.into_iter().enumerate() {
                let t = time_scene(backend.renderer().as_mut(), &scene, scale, 51).unwrap();
                let us = t.median().as_secs_f64() * 1e6;
                totals[i][usize::from(scale > 1.0)] += us;
                cells.push(format!("{us:.0} µs"));
            }
        }
        let mut v = scene.target(1.0).unwrap();
        let mut s = scene.target(1.0).unwrap();
        Backend::VelloCpu
            .renderer()
            .render(scene.list(), &mut v)
            .unwrap();
        Backend::TinySkia
            .renderer()
            .render(scene.list(), &mut s)
            .unwrap();
        let d8 = compare(&v, &s, Tolerance::new(8, 0.0))
            .unwrap()
            .differing_ratio();
        let d32 = compare(&v, &s, Tolerance::new(32, 0.0))
            .unwrap()
            .differing_ratio();
        println!(
            "| {} | {} | {:.2}% / {:.2}% |",
            scene.name(),
            cells.join(" | "),
            d8 * 100.0,
            d32 * 100.0
        );
    }
    println!(
        "| total | {:.0} µs | {:.0} µs | {:.0} µs | {:.0} µs | |",
        totals[0][0], totals[1][0], totals[0][1], totals[1][1]
    );
}

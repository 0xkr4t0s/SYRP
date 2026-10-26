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

// SPDX-License-Identifier: MIT OR Apache-2.0
//! B6: Callgrind instruction counts for the kernels at every level (ADR 0003 D7).
//!
//! `cargo bench -p syrp-simd` re-runs this binary under `valgrind --tool=callgrind` once per
//! (case, level), counting only the instructions inside `measured_region`, and prints a table. It
//! skips (exit 0) when Valgrind is not installed. CI does not run it yet.
//!
//! It fails on a codegen regression: on x86-64 every case must run in at most 70% of its SSE2
//! instruction count at AVX2. `voice_f32x8` is lane-generic `Sample` code called through
//! `dispatch!`; if it stops being compiled for the level (the inlining rule), its `mul_add`
//! becomes a software `fma` call per lane at both levels and the check fails.

use std::hint::black_box;
use std::process::{Command, ExitCode};

use syrp_simd::{F32x8, Level, Sample, dispatch, kernels};

const N: usize = 4096;
const CASES: [&str; 8] = [
    "add",
    "mul",
    "mul_add",
    "mul_add_f64",
    "copy_with_gain",
    "find_min_max",
    "abs_max",
    "voice_f32x8",
];

#[inline(always)]
fn voice<S: Sample>(xs: &mut [S], gain: S, bias: S) {
    for x in xs {
        *x = x.mul_add(gain, bias).max(S::zero());
    }
}

/// Lane-generic `Sample` code dispatched as users write it. Kept out of `measured_region` so
/// the closure symbols `dispatch!` creates do not match Callgrind's toggle pattern.
#[inline(never)]
fn voice_case(level: Level, v: &mut [F32x8]) {
    let (g, o) = (F32x8::splat(0.99), F32x8::splat(0.01));
    dispatch!(level, _s => voice(v, g, o), scalar => voice(v, g, o));
}

/// Callgrind collects only inside this function (its name is the toggle pattern).
#[inline(never)]
fn measured_region(
    case: &str,
    level: Level,
    a: &mut [f32],
    b: &[f32],
    c: &mut [f64],
    v: &mut [F32x8],
) {
    match case {
        "add" => kernels::add(level, a, b),
        "mul" => kernels::mul(level, a, b),
        "mul_add" => kernels::mul_add(level, a, b, b),
        "mul_add_f64" => {
            let d = c.to_owned();
            kernels::mul_add(level, c, &d, &d)
        }
        "copy_with_gain" => kernels::copy_with_gain(level, a, b, 0.5),
        "find_min_max" => {
            black_box(kernels::find_min_max(level, b));
        }
        "abs_max" => {
            black_box(kernels::abs_max(level, b));
        }
        "voice_f32x8" => voice_case(level, v),
        _ => unreachable!("unknown case {case}"),
    }
}

/// Child mode: run one case at one level. Exit code 3 means the level is unavailable here.
fn child(case: &str, level: &str) -> ExitCode {
    let Some(level) = Level::supported().find(|l| format!("{l:?}") == level) else {
        return ExitCode::from(3);
    };
    let mut a: Vec<f32> = (0..N).map(|i| (i as f32 * 0.37).sin()).collect();
    let b: Vec<f32> = (0..N).map(|i| (i as f32 * 0.11).cos()).collect();
    let mut c: Vec<f64> = (0..N).map(|i| i as f64 * 1e-3).collect();
    let mut v = vec![F32x8::splat(0.5); N / 8];
    measured_region(black_box(case), level, &mut a, &b, &mut c, &mut v);
    black_box((a, c, v));
    ExitCode::SUCCESS
}

/// Instructions Callgrind collected for one case, or `None` when the level is unavailable.
fn measure(case: &str, level: &str) -> Option<u64> {
    let exe = std::env::current_exe().expect("bench executable path");
    let out = Command::new("valgrind")
        .args([
            "--tool=callgrind",
            "--callgrind-out-file=/dev/null",
            "--collect-atstart=no",
            "--toggle-collect=*measured_region*",
        ])
        .arg(exe)
        .args(["--child", case, level])
        .output()
        .expect("valgrind runs");
    if out.status.code() == Some(3) {
        return None;
    }
    assert!(out.status.success(), "{case} at {level} failed: {out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let line = stderr.lines().find(|l| l.contains("Collected :"));
    let n = line
        .and_then(|l| l.rsplit(' ').next())
        .and_then(|n| n.parse().ok());
    Some(n.unwrap_or_else(|| panic!("no Callgrind total for {case} at {level}:\n{stderr}")))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if let [_, flag, case, level] = &args[..]
        && flag == "--child"
    {
        return child(case, level);
    }
    if Command::new("valgrind").arg("--version").output().is_err() {
        eprintln!("syrp-simd instruction benches: valgrind not found, skipping");
        return ExitCode::SUCCESS;
    }
    let levels = ["Scalar", "Sse2", "Sse4_2", "Avx2", "Neon"];
    println!(
        "{:<16}{}",
        "case (N=4096)",
        levels.map(|l| format!("{l:>12}")).join("")
    );
    let mut regressions = Vec::new();
    for case in CASES {
        let counts = levels.map(|l| measure(case, l));
        let cells = counts.map(|c| c.map_or("-".to_owned(), |n| n.to_string()));
        println!("{case:<16}{}", cells.map(|c| format!("{c:>12}")).join(""));
        if let (Some(sse2), Some(avx2)) = (counts[1], counts[3])
            && avx2 * 10 > sse2 * 7
        {
            regressions.push(format!("{case}: Avx2 {avx2} vs Sse2 {sse2}"));
        }
    }
    if regressions.is_empty() {
        ExitCode::SUCCESS
    } else {
        eprintln!(
            "codegen regression (Avx2 above 70% of Sse2):\n{}",
            regressions.join("\n")
        );
        ExitCode::FAILURE
    }
}

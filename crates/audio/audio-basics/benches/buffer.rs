// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B6: Callgrind instruction counts for `AudioBuffer` operations and the `vector_ops` pack
//! paths (ADR 0003 D7), in the harness style of `syrp-simd`'s `benches/instructions.rs`.
//!
//! `cargo bench -p syrp-audio-basics --bench buffer` re-runs this binary under
//! `valgrind --tool=callgrind` once per (case, level), counting only the instructions inside
//! `measured_region`, and prints two tables. It skips (exit 0) when Valgrind is missing.
//!
//! - Buffer cases (`apply_gain`, `add_from`, `rms_level` on 2 x 512 frames) run at
//!   `Level::detect()`, once with `SYRP_SIMD=scalar` and once at the detected level.
//! - Pack cases call `vector_ops` operations that are written against `Sample` packs (no
//!   `syrp-simd` kernel) at each level. On x86-64 each must cost at most 70% of its `Sse2`
//!   count at `Avx2`; otherwise the inlining rule broke and the packs are not compiled wide.

use std::hint::black_box;
use std::process::{Command, ExitCode};

use syrp_audio_basics::{AudioBuffer, vector_ops};
use syrp_simd::Level;

const FRAMES: usize = 512;
const N: usize = 4096;
const BUFFER_CASES: [&str; 3] = ["apply_gain", "add_from", "rms_level"];
const PACK_CASES: [&str; 4] = [
    "multiply_scalar",
    "add_with_multiply",
    "clip",
    "find_maximum",
];

/// Callgrind collects only inside this function (its name is the toggle pattern).
#[inline(never)]
fn measured_region(
    case: &str,
    level: Level,
    buf: &mut AudioBuffer<f32>,
    src: &AudioBuffer<f32>,
    a: &mut [f32],
    b: &[f32],
) {
    match case {
        "apply_gain" => buf.apply_gain(0.5),
        "add_from" => {
            buf.add_from(0, 0, src, 0, 0..FRAMES, 0.5);
            buf.add_from(1, 0, src, 1, 0..FRAMES, 0.5);
        }
        "rms_level" => {
            black_box((buf.rms_level(0, 0..FRAMES), buf.rms_level(1, 0..FRAMES)));
        }
        "multiply_scalar" => vector_ops::multiply_scalar(level, a, 0.5),
        "add_with_multiply" => vector_ops::add_with_multiply(level, a, b, 0.5),
        "clip" => vector_ops::clip(level, a, -0.5, 0.5),
        "find_maximum" => {
            black_box(vector_ops::find_maximum(level, b));
        }
        _ => unreachable!("unknown case {case}"),
    }
}

/// Child mode: run one case at one level. Exit code 3 means the level is unavailable here.
fn child(case: &str, level: &str) -> ExitCode {
    let level = match level {
        "Detect" => Level::detect(),
        _ => match Level::supported().find(|l| format!("{l:?}") == level) {
            Some(l) => l,
            None => return ExitCode::from(3),
        },
    };
    let ramp = |k: f32| (0..FRAMES).map(move |i| (i as f32 * k).sin());
    let mut buf = AudioBuffer::<f32>::new(2, FRAMES).expect("small buffer");
    let mut src = AudioBuffer::<f32>::new(2, FRAMES).expect("small buffer");
    for ch in 0..2 {
        buf.channel_mut(ch)
            .iter_mut()
            .zip(ramp(0.37))
            .for_each(|(d, x)| *d = x);
        src.channel_mut(ch)
            .iter_mut()
            .zip(ramp(0.11))
            .for_each(|(d, x)| *d = x);
    }
    let mut a: Vec<f32> = (0..N).map(|i| (i as f32 * 0.37).sin()).collect();
    let b: Vec<f32> = (0..N).map(|i| (i as f32 * 0.11).cos()).collect();
    measured_region(black_box(case), level, &mut buf, &src, &mut a, &b);
    black_box((buf, a));
    ExitCode::SUCCESS
}

/// Instructions Callgrind collected for one case, or `None` when the level is unavailable.
fn measure(case: &str, level: &str, scalar_env: bool) -> Option<u64> {
    let exe = std::env::current_exe().expect("bench executable path");
    let mut cmd = Command::new("valgrind");
    cmd.args([
        "--tool=callgrind",
        "--callgrind-out-file=/dev/null",
        "--collect-atstart=no",
        "--toggle-collect=*measured_region*",
    ])
    .arg(exe)
    .args(["--child", case, level]);
    if scalar_env {
        cmd.env("SYRP_SIMD", "scalar");
    } else {
        cmd.env_remove("SYRP_SIMD");
    }
    let out = cmd.output().expect("valgrind runs");
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

fn row(name: &str, counts: &[Option<u64>]) {
    let cells: Vec<String> = counts
        .iter()
        .map(|c| format!("{:>12}", c.map_or("-".to_owned(), |n| n.to_string())))
        .collect();
    println!("{name:<20}{}", cells.join(""));
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if let [_, flag, case, level] = &args[..]
        && flag == "--child"
    {
        return child(case, level);
    }
    if Command::new("valgrind").arg("--version").output().is_err() {
        eprintln!("syrp-audio-basics buffer bench: valgrind not found, skipping");
        return ExitCode::SUCCESS;
    }
    println!("{:<20}{:>12}{:>12}", "buffer (2 x 512)", "Scalar", "Detect");
    for case in BUFFER_CASES {
        row(
            case,
            &[
                measure(case, "Detect", true),
                measure(case, "Detect", false),
            ],
        );
    }
    let levels = ["Scalar", "Sse2", "Sse4_2", "Avx2", "Neon"];
    println!(
        "\n{:<20}{}",
        "pack (N=4096)",
        levels.map(|l| format!("{l:>12}")).join("")
    );
    let mut regressions = Vec::new();
    for case in PACK_CASES {
        let counts = levels.map(|l| measure(case, l, false));
        row(case, &counts);
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

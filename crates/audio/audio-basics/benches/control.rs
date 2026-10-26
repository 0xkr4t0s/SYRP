// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B6: Callgrind instruction counts for `control` (ADR 0003 D7), in the harness style of
//! `benches/buffer.rs`.
//!
//! `cargo bench -p syrp-audio-basics --bench control` re-runs this binary under
//! `valgrind --tool=callgrind` once per (case, level), counting only the instructions inside
//! `measured_region`, run through `dispatch!`. It skips (exit 0) when Valgrind is missing, and
//! prints `-` for a level the CPU lacks or Valgrind cannot decode. Cases, 512 samples each:
//!
//! - `adsr_f32_x8`: eight `Adsr<f32>` envelopes, one after the other.
//! - `adsr_f32x8`: one `Adsr<F32x8>`, the same eight envelopes in lanes.
//! - `smooth_per_sample` and `smooth_control_16`: a `Multiplicative` `SmoothedValue<f32>`
//!   ramp applied as a gain, per sample and at a control rate of 16.
//!
//! The bench fails if, at any SIMD level, the pack envelope costs as much as the eight scalar
//! ones, which would mean the lane-generic code is not inlined into the dispatch body.

use std::hint::black_box;
use std::process::{Command, ExitCode};

use syrp_audio_basics::control::{Adsr, AdsrParams, Multiplicative, SmoothedValue};
use syrp_simd::{F32x8, Level, Sample, dispatch};

const FRAMES: usize = 512;
const CASES: [&str; 4] = [
    "adsr_f32_x8",
    "adsr_f32x8",
    "smooth_per_sample",
    "smooth_control_16",
];
const ATTACK: [f32; 8] = [0.001, 0.002, 0.003, 0.004, 0.005, 0.006, 0.007, 0.008];

fn params(lane: usize) -> AdsrParams<f32> {
    AdsrParams::new(ATTACK[lane], 0.003, 0.5, 0.004)
}

/// Callgrind collects only inside this function (its name is the toggle pattern).
#[inline(never)]
fn measured_region(case: &str, level: Level, mono: &mut [f32], packs: &mut [F32x8]) {
    match case {
        "adsr_f32_x8" => {
            let mut envs: [Adsr<f32>; 8] = core::array::from_fn(|l| Adsr::new(48_000.0, params(l)));
            dispatch!(level, _s => envs.iter_mut().for_each(|e| { e.note_on(true); e.apply_to(mono) }),
                scalar => envs.iter_mut().for_each(|e| { e.note_on(true); e.apply_to(mono) }));
            black_box(envs);
        }
        "adsr_f32x8" => {
            let lanes = AdsrParams::new(
                F32x8::from_array(ATTACK),
                F32x8::splat(0.003),
                F32x8::splat(0.5),
                F32x8::splat(0.004),
            );
            let mut env = Adsr::new(48_000.0, lanes);
            let on = F32x8::one().simd_gt(F32x8::zero());
            dispatch!(level, _s => { env.note_on(on); env.apply_to(packs) },
                scalar => { env.note_on(on); env.apply_to(packs) });
            black_box(env);
        }
        "smooth_per_sample" | "smooth_control_16" => {
            let rate = if case == "smooth_per_sample" { 1 } else { 16 };
            let mut v = SmoothedValue::<f32, Multiplicative>::new(0.01).with_control_rate(rate);
            v.reset(48_000.0, 0.1);
            v.set_target(1.0);
            dispatch!(level, _s => v.apply_gain(mono), scalar => v.apply_gain(mono));
            black_box(v);
        }
        _ => unreachable!("unknown case {case}"),
    }
}

/// Child mode: run one case at one level. Exit code 3 means the level is unavailable here.
fn child(case: &str, level: &str) -> ExitCode {
    let Some(level) = Level::supported().find(|l| format!("{l:?}") == level) else {
        return ExitCode::from(3);
    };
    let mut mono: Vec<f32> = (0..FRAMES).map(|i| (i as f32 * 0.37).sin()).collect();
    let mut packs = vec![F32x8::one(); FRAMES];
    measured_region(black_box(case), level, &mut mono, &mut packs);
    black_box((mono, packs));
    ExitCode::SUCCESS
}

/// Instructions Callgrind collected for one case, or `None` when the level is unavailable or
/// Valgrind cannot decode an instruction the level uses (it reports SIGILL).
fn measure(case: &str, level: &str) -> Option<u64> {
    let exe = std::env::current_exe().expect("bench executable path");
    let out = Command::new("valgrind")
        .args([
            "--tool=callgrind",
            "--callgrind-out-file=/dev/null",
            "--collect-atstart=no",
            // Exact name: the `dispatch!` bodies are closures named `measured_region::{{closure}}`,
            // and a wildcard would toggle collection off inside them.
            "--toggle-collect=control::measured_region",
        ])
        .arg(exe)
        .args(["--child", case, level])
        .output()
        .expect("valgrind runs");
    let stderr = String::from_utf8_lossy(&out.stderr);
    if out.status.code() == Some(3) || stderr.contains("Unrecognised instruction") {
        return None;
    }
    assert!(out.status.success(), "{case} at {level} failed: {out:?}");
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
        eprintln!("syrp-audio-basics control bench: valgrind not found, skipping");
        return ExitCode::SUCCESS;
    }
    let levels = ["Scalar", "Sse4_2", "Avx2", "Neon"];
    let header = levels.map(|l| format!("{l:>12}")).join("");
    println!("{:<22}{header}", "control (512 samples)");
    let counts = CASES.map(|case| {
        let row = levels.map(|level| measure(case, level));
        let cells = row.map(|c| format!("{:>12}", c.map_or("-".to_owned(), |n| n.to_string())));
        println!("{case:<22}{}", cells.join(""));
        row
    });
    let mut failed = false;
    for (i, level) in levels.iter().enumerate().skip(1) {
        if let (Some(scalar), Some(pack)) = (counts[0][i], counts[1][i])
            && pack >= scalar
        {
            eprintln!(
                "{level}: Adsr<F32x8> ({pack}) is not cheaper than eight Adsr<f32> ({scalar})"
            );
            failed = true;
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

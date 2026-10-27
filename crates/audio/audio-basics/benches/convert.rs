// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B4: Callgrind instruction counts for the `convert` pack paths (ADR 0003 D7), in the harness
//! style of `benches/buffer.rs`.
//!
//! `cargo bench -p syrp-audio-basics --bench convert` re-runs this binary under
//! `valgrind --tool=callgrind` once per (case, level), counting only the instructions inside
//! `measured_region`, and prints a table of `Level::Scalar` (the scalar codec) against
//! `Level::best()`. It skips (exit 0) when Valgrind is missing. Every case is 512 stereo
//! frames; the spec's headline case is `int24_to_f32_interleaved`.
//!
//! The bench fails if a case at the best level costs more than the scalar codec.

use std::hint::black_box;
use std::process::{Command, ExitCode};

use syrp_audio_api::{AudioBlock, AudioBlockMut};
use syrp_audio_basics::convert::{Endianness, SampleFormat, read_into_at, write_from_at};
use syrp_simd::Level;

const FRAMES: usize = 512;
const CHANNELS: usize = 2;
const CASES: [(&str, SampleFormat, bool, bool); 5] = [
    // (name, format, interleaved, decode)
    ("int24_to_f32_interleaved", SampleFormat::Int24, true, true),
    ("int24_to_f32_planar", SampleFormat::Int24, false, true),
    ("int16_to_f32_interleaved", SampleFormat::Int16, true, true),
    ("f32_to_int24_interleaved", SampleFormat::Int24, true, false),
    ("f32_to_int16_planar", SampleFormat::Int16, false, false),
];

/// Callgrind collects only inside this function (its name is the toggle pattern).
#[inline(never)]
fn measured_region(
    level: Level,
    (format, interleaved, decode): (SampleFormat, bool, bool),
    bytes: &mut [u8],
    planar: &mut [f32],
) {
    let e = Endianness::Little;
    if decode {
        let mut block = AudioBlockMut::new(planar, CHANNELS, FRAMES, FRAMES).expect("shape");
        read_into_at(level, bytes, format, e, interleaved, CHANNELS, &mut block).expect("size");
    } else {
        let block = AudioBlock::new(planar, CHANNELS, FRAMES, FRAMES).expect("shape");
        write_from_at(level, &block, bytes, format, e, interleaved).expect("size");
    }
}

/// Child mode: run one case at one level.
fn child(case: &str, level: &str) -> ExitCode {
    let level = if level == "Scalar" {
        Level::Scalar
    } else {
        Level::best()
    };
    let Some(&(_, format, interleaved, decode)) = CASES.iter().find(|c| c.0 == case) else {
        return ExitCode::from(2);
    };
    let n = CHANNELS * FRAMES;
    let mut bytes: Vec<u8> = (0..n * format.bytes_per_sample())
        .map(|i| (i * 37 % 251) as u8)
        .collect();
    let mut planar: Vec<f32> = (0..n).map(|i| (i as f32 * 0.37).sin()).collect();
    measured_region(
        black_box(level),
        black_box((format, interleaved, decode)),
        &mut bytes,
        &mut planar,
    );
    black_box((bytes, planar));
    ExitCode::SUCCESS
}

/// Instructions Callgrind collected for one case.
fn measure(case: &str, level: &str) -> u64 {
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
        .env_remove("SYRP_SIMD")
        .output()
        .expect("valgrind runs");
    assert!(out.status.success(), "{case} at {level} failed: {out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let line = stderr.lines().find(|l| l.contains("Collected :"));
    let n = line
        .and_then(|l| l.rsplit(' ').next())
        .and_then(|n| n.parse().ok());
    n.unwrap_or_else(|| panic!("no Callgrind total for {case} at {level}:\n{stderr}"))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if let [_, flag, case, level] = &args[..]
        && flag == "--child"
    {
        return child(case, level);
    }
    if Command::new("valgrind").arg("--version").output().is_err() {
        eprintln!("syrp-audio-basics convert bench: valgrind not found, skipping");
        return ExitCode::SUCCESS;
    }
    let best = format!("{:?}", Level::best());
    println!("{:<28}{:>12}{:>12}", "convert (2 x 512)", "Scalar", best);
    let mut regressions = Vec::new();
    for (case, ..) in CASES {
        let (scalar, fast) = (measure(case, "Scalar"), measure(case, "Best"));
        println!("{case:<28}{scalar:>12}{fast:>12}");
        if fast > scalar {
            regressions.push(format!("{case}: {best} {fast} vs Scalar {scalar}"));
        }
    }
    if regressions.is_empty() {
        ExitCode::SUCCESS
    } else {
        eprintln!(
            "pack path slower than the scalar codec:\n{}",
            regressions.join("\n")
        );
        ExitCode::FAILURE
    }
}

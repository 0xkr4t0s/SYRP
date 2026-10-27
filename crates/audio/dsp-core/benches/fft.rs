// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B7: Callgrind instruction count for an order-10 real forward transform (ADR 0003 D7), in
//! the harness style of `syrp-simd`'s `benches/instructions.rs`.
//!
//! `cargo bench -p syrp-dsp-core --bench fft` re-runs this binary under
//! `valgrind --tool=callgrind`, counting only the instructions inside `measured_region`: one
//! `Fft::perform_real_only_forward_transform` of 1024 points on a prepared `Fft`. It prints
//! the count, skips (exit 0) when Valgrind is missing, and fails above `BUDGET`.

use std::hint::black_box;
use std::process::{Command, ExitCode};

use syrp_dsp_core::Fft;

/// The order measured: 1024 points.
const ORDER: u32 = 10;
/// The most instructions one transform may take: 32,005 were measured when the bench was
/// written (x86-64, AVX path); the margin covers rustfft choosing SSE at run time.
const BUDGET: u64 = 60_000;

/// Callgrind collects only inside this function (its name is the toggle pattern).
#[inline(never)]
fn measured_region(fft: &mut Fft, data: &mut [f32]) {
    fft.perform_real_only_forward_transform(data, true);
}

/// Child mode: run the measured transform once on a warmed-up `Fft`.
fn child() -> ExitCode {
    let mut fft = Fft::new(ORDER).expect("order 10 is supported");
    let n = fft.size();
    let mut data: Vec<f32> = (0..2 * n)
        .map(|i| ((i * 7919) % 1000) as f32 / 1000.0 - 0.5)
        .collect();
    // Warm-up: first-touch effects stay outside the measured call.
    fft.perform_real_only_forward_transform(&mut data, true);
    measured_region(black_box(&mut fft), black_box(&mut data));
    black_box(data);
    ExitCode::SUCCESS
}

/// Instructions Callgrind collected in `measured_region`.
fn measure() -> u64 {
    let exe = std::env::current_exe().expect("bench executable path");
    let out = Command::new("valgrind")
        .args([
            "--tool=callgrind",
            "--callgrind-out-file=/dev/null",
            "--collect-atstart=no",
            "--toggle-collect=*measured_region*",
        ])
        .arg(exe)
        .arg("--child")
        .output()
        .expect("valgrind runs");
    assert!(out.status.success(), "child failed: {out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let line = stderr.lines().find(|l| l.contains("Collected :"));
    let n = line
        .and_then(|l| l.rsplit(' ').next())
        .and_then(|n| n.parse().ok());
    n.unwrap_or_else(|| panic!("no Callgrind total:\n{stderr}"))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--child") {
        return child();
    }
    if Command::new("valgrind").arg("--version").output().is_err() {
        eprintln!("syrp-dsp-core fft bench: valgrind not found, skipping");
        return ExitCode::SUCCESS;
    }
    let count = measure();
    println!("order-{ORDER} real forward FFT: {count} instructions (budget {BUDGET})");
    if count > BUDGET {
        eprintln!("the order-{ORDER} real forward FFT costs more than {BUDGET} instructions");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

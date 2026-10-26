// SPDX-License-Identifier: MIT OR Apache-2.0
//! B6: the Callgrind instruction-count bench runs and finds no regression to baseline codegen.
//! Ignored by default (it needs Valgrind and a release build, about a minute):
//! `cargo test -p syrp-simd --test benches -- --ignored`.

use std::process::Command;

#[test]
#[ignore = "needs valgrind; runs `cargo bench -p syrp-simd --bench instructions`"]
fn b6_instruction_counts_show_no_baseline_codegen_regression() {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = Command::new(cargo)
        .args(["bench", "-p", "syrp-simd", "--bench", "instructions"])
        .status()
        .expect("cargo runs");
    assert!(status.success(), "instruction-count bench failed: {status}");
}

// SPDX-License-Identifier: MIT OR Apache-2.0
//! Declares the `loom` cfg so model-checking builds do not warn about an unknown cfg.

fn main() {
    println!("cargo::rustc-check-cfg=cfg(loom)");
}

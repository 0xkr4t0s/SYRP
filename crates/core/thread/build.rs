// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Declares the `loom` cfg so model-checking builds do not warn about an unknown cfg.

fn main() {
    println!("cargo::rustc-check-cfg=cfg(loom)");
}

//! B8: Callgrind instruction counts for property access (ADR 0003 D7).
//!
//! `cargo bench -p syrp-state` re-runs this binary under `valgrind --tool=callgrind` once per
//! (case, property count), counting only the instructions inside `measured_region`, and prints
//! instructions per operation. It skips (exit 0) when Valgrind is not installed. CI does not
//! gate on it yet; the numbers are recorded in the crate's `AGENTS.md`.

use std::hint::black_box;
use std::process::{Command, ExitCode};

use syrp_loop_api::{ManualLoop, MessageThread};
use syrp_state::{NodeRef, Store};
use syrp_var::Identifier;

const OPS: u32 = 10_000;
const CASES: [&str; 3] = ["property", "set_no_listeners", "set_two_listeners"];
const SIZES: [usize; 3] = [4, 16, 64];

/// Callgrind collects only inside this function (its name is the toggle pattern).
#[inline(never)]
fn measured_region(
    case: &str,
    store: &Store,
    mt: &MessageThread<'_>,
    node: &NodeRef,
    ids: &[Identifier],
) {
    let mut at = 0;
    for n in 0..OPS {
        at = if at + 1 == ids.len() { 0 } else { at + 1 }; // no divide in the loop
        if case == "property" {
            black_box(store.property(node, ids[at]).unwrap());
        } else {
            store.set_property(mt, node, ids[at], f64::from(n)).unwrap();
        }
    }
}

/// Child mode: build a node with `size` properties and run one case.
fn child(case: &str, size: usize) {
    ManualLoop::new().with_token(|mt| {
        let store = Store::new();
        let parent = store.create(mt, Identifier::new("Parent"));
        let node = store.create(mt, Identifier::new("Node"));
        store.add_child(mt, &parent, &node, None).unwrap();
        let ids: Vec<Identifier> = (0..size)
            .map(|i| Identifier::new(&format!("p{i}")))
            .collect();
        for id in &ids {
            store.set_property(mt, &node, *id, -1.0).unwrap();
        }
        let _subs = (case == "set_two_listeners").then(|| {
            let sub = |n: &NodeRef| {
                store
                    .subscribe(mt, n, |_, _, c| {
                        black_box(c);
                    })
                    .unwrap()
            };
            (sub(&node), sub(&parent))
        });
        measured_region(black_box(case), &store, mt, &node, &ids);
    });
}

/// Instructions per operation for one case.
fn measure(case: &str, size: usize) -> u64 {
    let exe = std::env::current_exe().expect("bench executable path");
    let out = Command::new("valgrind")
        .args([
            "--tool=callgrind",
            "--callgrind-out-file=/dev/null",
            "--collect-atstart=no",
        ])
        .arg("--toggle-collect=*measured_region*")
        .arg(exe)
        .args(["--child", case, &size.to_string()])
        .output()
        .expect("valgrind runs");
    assert!(out.status.success(), "{case} at {size} failed: {out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let line = stderr.lines().find(|l| l.contains("Collected :"));
    let n: Option<u64> = line
        .and_then(|l| l.rsplit(' ').next())
        .and_then(|n| n.parse().ok());
    n.unwrap_or_else(|| panic!("no Callgrind total for {case} at {size}:\n{stderr}"))
        / u64::from(OPS)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if let [_, flag, case, size] = &args[..]
        && flag == "--child"
    {
        child(case, size.parse().expect("property count"));
        return ExitCode::SUCCESS;
    }
    if Command::new("valgrind").arg("--version").output().is_err() {
        eprintln!("syrp-state instruction benches: valgrind not found, skipping");
        return ExitCode::SUCCESS;
    }
    println!(
        "{:<20}{}",
        "instructions/op",
        SIZES.map(|s| format!("{s:>8}")).join("")
    );
    for case in CASES {
        let cells = SIZES.map(|s| format!("{:>8}", measure(case, s)));
        println!("{case:<20}{}", cells.join(""));
    }
    ExitCode::SUCCESS
}

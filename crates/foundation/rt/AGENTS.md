# syrp-rt

## Purpose
Real-time primitives for audio threads (master plan §5.4): the `RtSafe` marker, `AtomicF32`
and `AtomicF64`, a bounded SPSC queue (`spsc::channel`), deferred drop (`Collector` and
`DeferredDrop`), `RtHandoff<T>` for configuration swaps, and the test allocation guard
(`GuardedAllocator` plus `no_alloc`, code `SYRP-RT-0001` in `errors/rt.toml`). No direct JUCE
class; it replaces the ad-hoc lock-free FIFOs and atomics JUCE code builds by hand
(AbstractFifo, `std::atomic<float>`).

## Ledger rows
None yet.

## Invariants
- Nothing allocates, frees, locks or blocks after construction: `spsc::channel`,
  `Collector::handle` and `RtHandoff::new` allocate; every other method on the RT-side types
  is RT-safe, and the `b2`-`b5` tests prove it under `no_alloc`.
- `HandoffReader::latest` never frees: it switches only when its `DeferredDrop` has room, so
  the replaced value always goes to the collector. Unread published values are freed by the
  writer. Publish is an AcqRel swap; the reader's take is an AcqRel swap.
- SPSC positions run in `0..2 * capacity`; producer writes only `tail`, consumer only `head`;
  slot writes are published by a Release store and read after an Acquire load.
- `DeferredDrop::defer` takes `Box<T>` so only a pointer moves; it gives the box back when full.
- Every `unsafe` block and impl has a `// SAFETY:` comment (clippy `undocumented_unsafe_blocks`
  plus test `b7_...`). Atomics and cells go through `src/sync.rs` so loom checks the real code.
- No third-party runtime dependencies; `loom` only under `cfg(loom)`.

## Thread affinity
- Audio thread (RT-safe): `AtomicF32`/`AtomicF64` methods, `Producer`/`Consumer` methods,
  `DeferredDrop::{defer, is_full, capacity}`, `HandoffReader::{latest, current}`.
- Any non-RT thread: `spsc::channel`, `Collector::{new, handle, collect}`,
  `RtHandoff::{new, publish}`, and dropping any handle (the last handle alive frees the
  shared ring and queued values; dropping `HandoffReader` frees its value).
- `no_alloc` guards only the calling thread.

## Stability
experimental

## How to test
```sh
cargo syrp test --crate syrp-rt
# Miri (unit, integration and doc tests; the abort test is skipped because it spawns a process):
cargo +nightly miri test -p syrp-rt
# loom models of the SPSC queue (B3) and RtHandoff (B5):
RUSTFLAGS="--cfg loom" CARGO_TARGET_DIR=target/loom cargo test -p syrp-rt --release --test loom
```
`tests/rt.rs` installs `GuardedAllocator`, so an allocation inside `no_alloc` aborts the test
binary with `SYRP-RT-0001`. CI wiring for Miri and loom is a separate change request.

## Known gaps
- RT tokens (`RtCtx`, `PrepareCtx`) from the WP title are not specified as behaviours here;
  they belong with `syrp-plugin-api` (WP-0.2-06) unless the spec room moves them.
- SPSC has no cached indices or cache-line padding yet (correct, not tuned).
- `SYRP-RT-0001` is in `errors/rt.toml` but not yet in the CLI's embedded catalogue.

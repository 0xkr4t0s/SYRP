// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B5: byte flips and truncations of valid files never panic and never allocate more than
//! the input justifies. This binary holds one test so the allocation counter sees only it.

mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::io::Cursor;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::files::catalogue;
use proptest::prelude::*;
use syrp_audio_api::AudioBlockMut;
use syrp_format_aiff::AiffAudioFormat;
use syrp_format_api::AudioFormat;

/// Counts live heap bytes and their peak.
struct Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

// SAFETY: every call forwards to `System` with the caller's layout and pointer unchanged, so
// `System`'s guarantees hold; the counters only observe sizes.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
        PEAK.fetch_max(live, Ordering::Relaxed);
        // SAFETY: `layout` comes from the caller, which upholds `GlobalAlloc::alloc`'s contract.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        // SAFETY: `ptr` was returned by `alloc` above (that is, by `System`) with this `layout`.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

/// Heap growth allowed beyond twice the input: the boxed reader and the stream wrapper. Chunk
/// bodies, the text decoded from them and the read buffer (never larger than the sample data)
/// are bounded by the input.
const SLACK: usize = 4 * 1024;
/// Allowed bookkeeping per chunk (metadata list entries while they grow); every chunk costs
/// at least 8 input bytes, so the bound stays linear in the input, never in a claimed size.
const PER_CHUNK: usize = 1024;

/// Opens `bytes` and reads 64 frames of 2 channels; returns the peak heap growth minus the
/// input length.
fn exercise(bytes: Vec<u8>, out: &mut [f64]) -> usize {
    let input = bytes.len();
    let base = LIVE.load(Ordering::Relaxed);
    PEAK.store(base, Ordering::Relaxed);
    if let Ok(mut r) = AiffAudioFormat::new().create_reader(Box::new(Cursor::new(bytes))) {
        let block = AudioBlockMut::new(out, 2, 64, 64).unwrap();
        let _ = r.read(block.into(), -3);
        let _ = r.metadata().len();
    }
    PEAK.load(Ordering::Relaxed).saturating_sub(base + input)
}

proptest! {
    // 256 cases by default; set PROPTEST_CASES for a longer run.
    #[test]
    fn b5_flips_and_truncation_never_panic_or_overallocate(
        which in 0usize..20,
        flips in proptest::collection::vec((any::<prop::sample::Index>(), 1u8..=255), 0..6),
        cut in any::<prop::sample::Index>(),
        truncate in any::<bool>(),
    ) {
        let cases = catalogue();
        let mut bytes = cases[which % cases.len()].bytes.clone();
        for (at, x) in flips {
            let i = at.index(bytes.len());
            bytes[i] ^= x;
        }
        if truncate {
            bytes.truncate(cut.index(bytes.len() + 1));
        }
        let input = bytes.len();
        let mut out = vec![0.0f64; 128];
        let extra = exercise(bytes, &mut out);
        let bound = input + input / 8 * PER_CHUNK + SLACK;
        prop_assert!(extra <= bound, "{extra} extra bytes for {input} input bytes");
    }
}

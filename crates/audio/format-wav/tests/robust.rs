// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B5: round trips over random values, and byte-flip and truncation fuzzing of valid files
//! that must never panic or allocate more than the input justifies.

mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::io::Cursor;

use common::{open, read_all_f32, write_file};
use proptest::prelude::*;
use syrp_audio_api::{AudioBlockMut, ChannelSet};
use syrp_format_api::{
    AudioReader, AudioSpec, Bext, CuePoint, LoopKind, Metadata, MetadataItem, SampleKind,
    SampleLoop, Tag, WriterOptions,
};
use syrp_format_wav::{WavAudioFormat, WavReader};

thread_local! {
    /// While `Some`, the largest single allocation request on this thread. Const-initialised
    /// with no destructor, so the allocator can read it without allocating.
    static LARGEST: Cell<Option<usize>> = const { Cell::new(None) };
}

/// Forwards to `System` and records the largest request made while `measure` runs.
struct Counting;

fn note(size: usize) {
    let _ = LARGEST.try_with(|c| {
        if let Some(max) = c.get() {
            c.set(Some(max.max(size)));
        }
    });
}

// SAFETY: every method forwards to `System` with its arguments unchanged; `note` only reads
// and writes a thread-local `Cell` and never allocates.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        note(layout.size());
        // SAFETY: the caller upholds `GlobalAlloc::alloc`'s contract, passed on unchanged.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` came from `System` through this allocator with `layout`.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        note(new_size);
        // SAFETY: the caller upholds `GlobalAlloc::realloc`'s contract; `ptr` came from
        // `System` through this allocator.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

/// Runs `f` and returns the largest single allocation it made on this thread.
fn measure(f: impl FnOnce()) -> usize {
    LARGEST.with(|c| c.set(Some(0)));
    f();
    LARGEST.with(|c| c.replace(None)).unwrap_or(0)
}

const SHAPES: [(u32, SampleKind); 5] = [
    (8, SampleKind::Int),
    (16, SampleKind::Int),
    (24, SampleKind::Int),
    (32, SampleKind::Int),
    (32, SampleKind::Float),
];

const LAYOUTS: [ChannelSet; 4] = [
    ChannelSet::mono(),
    ChannelSet::stereo(),
    ChannelSet::surround_5_1(),
    ChannelSet::discrete(3),
];

fn rich_metadata() -> Metadata {
    let mut m = Metadata::new();
    m.push(MetadataItem::Bext(
        Bext::new().with_description("d").with_coding_history("h"),
    ));
    m.push(MetadataItem::Cue(CuePoint::new(1, 2).with_label("c")));
    let lp = SampleLoop::new(LoopKind::Forward, 0, 1).expect("ordered");
    m.push(MetadataItem::Loop(lp));
    m.push(MetadataItem::Tag(Tag::Title, "t".into()));
    m.push(MetadataItem::Other("iXML".into(), b"<x/>".to_vec()));
    m
}

/// A valid file: any writable shape and layout, `frames` frames, RIFF or RF64, with or
/// without metadata.
fn valid_file() -> impl Strategy<Value = Vec<u8>> {
    (
        0..SHAPES.len(),
        0..LAYOUTS.len(),
        0usize..12,
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(|(shape, layout, frames, rf64, meta)| {
            let (bits, kind) = SHAPES[shape];
            let set = LAYOUTS[layout];
            let spec = AudioSpec::new(44_100.0, set, bits, kind).expect("valid");
            let mut options = WriterOptions::new(spec);
            if meta {
                options = options.with_metadata(rich_metadata());
            }
            let wav = WavAudioFormat::new().with_rf64_threshold(if rf64 { 0 } else { u64::MAX });
            let ch: Vec<Vec<f32>> = (0..set.channel_count())
                .map(|c| (0..frames).map(|i| ((i + c) as f32 / 16.0) - 0.5).collect())
                .collect();
            write_file(wav, &options, &ch)
        })
}

fn open_and_read(bytes: &[u8]) {
    if let Ok(mut r) = WavReader::open(Box::new(Cursor::new(bytes.to_vec()))) {
        let ch = r.spec().channel_count();
        let n = usize::try_from(r.length()).expect("fits");
        assert!(n * r.spec().bits_per_sample() as usize / 8 <= bytes.len());
        let mut buf = vec![0.0f32; ch * n.max(1)];
        let block = AudioBlockMut::new(&mut buf, ch, n, n.max(1)).expect("shape");
        let _ = r.read(block.into(), 0);
    }
}

proptest! {
    #[test]
    fn b5_every_shape_round_trips_random_grid_values(
        shape in 0..SHAPES.len(),
        steps in proptest::collection::vec(-128i32..128, 1..40),
    ) {
        let (bits, kind) = SHAPES[shape];
        // Multiples of 2^-7 lie on every grid, 8-bit included; -1.0 is the lowest step.
        let ch = vec![steps.iter().map(|&s| s as f32 / 128.0).collect::<Vec<f32>>()];
        let spec = AudioSpec::new(48_000.0, ChannelSet::mono(), bits, kind).expect("valid");
        let file = write_file(WavAudioFormat::new(), &WriterOptions::new(spec), &ch);
        prop_assert_eq!(read_all_f32(open(file).expect("valid").as_mut()), ch);
    }

    #[test]
    fn b5_byte_flips_and_truncation_never_panic_or_overallocate(
        file in valid_file(),
        flips in proptest::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 0..8),
        cut in proptest::option::of(any::<prop::sample::Index>()),
    ) {
        let mut bytes = file;
        for (at, value) in flips {
            let i = at.index(bytes.len());
            bytes[i] = value;
        }
        if let Some(cut) = cut {
            bytes.truncate(cut.index(bytes.len() + 1));
        }
        let largest = measure(|| open_and_read(&bytes));
        // The decode scratch is 16 KiB; anything else is bounded by a multiple of the input.
        prop_assert!(largest <= 8 * bytes.len() + 64 * 1024, "{} for {} bytes", largest, bytes.len());
    }
}

#[test]
fn b5_huge_declared_sizes_in_a_tiny_file_allocate_little() {
    // A cue chunk claiming 2^32 - 1 points, and a ds64 table claiming as many entries.
    let mut cue = common::chunk(b"cue ", &u32::MAX.to_le_bytes());
    cue.extend(common::chunk(b"data", &[0; 2]));
    let file = common::riff(&[common::chunk(b"fmt ", &common::fmt(1, 1, 8000, 16)), cue]);
    let mut ds64 = vec![0u8; 28];
    ds64[24..28].copy_from_slice(&u32::MAX.to_le_bytes());
    let mut rf64 = common::riff(&[common::chunk(b"ds64", &ds64)]);
    rf64[..4].copy_from_slice(b"RF64");
    for bytes in [file, rf64] {
        let largest = measure(|| open_and_read(&bytes));
        assert!(largest <= 64 * 1024, "{largest}");
    }
}

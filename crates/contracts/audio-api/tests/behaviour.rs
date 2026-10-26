// SPDX-License-Identifier: MIT OR Apache-2.0
//! One or more tests per behaviour in work/specs/WP-0.2-02.md delivered so far
//! (B1, B3, B5, B6, B7; B2 and B4 follow in the next increment).

use syrp_audio_api::{
    AudioBlock, AudioBlockMut, AudioError, AudioSample, ChannelsMut, Precision, ProcessConfig,
    ProcessMode,
};
use syrp_error::Diagnostic;
use syrp_rt::{RtSafe, no_alloc};

#[global_allocator]
static ALLOC: syrp_rt::GuardedAllocator = syrp_rt::GuardedAllocator;

/// 64-byte aligned storage, as an owning buffer in `syrp-audio-basics` provides.
#[repr(C, align(64))]
struct Aligned<T, const N: usize>([T; N]);

fn ramp<const N: usize>() -> [f32; N] {
    std::array::from_fn(|i| i as f32)
}

#[test]
fn b1_views_read_planar_channels_at_the_stride() {
    let data = ramp::<12>();
    let block = AudioBlock::new(&data, 3, 3, 4).unwrap();
    assert_eq!(
        (block.channels(), block.frames(), block.stride()),
        (3, 3, 4)
    );
    assert_eq!(block.channel(0), &[0.0, 1.0, 2.0]);
    assert_eq!(block.channel(2), &[8.0, 9.0, 10.0]);
    assert_eq!(block.channel_padded(1), &[4.0, 5.0, 6.0, 7.0]);
    let all: Vec<&[f32]> = block.iter().collect();
    assert_eq!(all.len(), 3);
    assert_eq!(all[1], &[4.0, 5.0, 6.0]);
}

#[test]
fn b1_slicing_frames_and_channels_never_allocates() {
    let mut data = ramp::<16>();
    no_alloc(|| {
        let block = AudioBlock::new(&data, 4, 4, 4).unwrap();
        let sub = block.slice_channels(1..3).slice_frames(1..3);
        assert_eq!((sub.channels(), sub.frames(), sub.stride()), (2, 2, 4));
        assert_eq!(sub.channel(0), &[5.0, 6.0]);
        assert_eq!(sub.channel(1), &[9.0, 10.0]);
        assert_eq!(block.slice_frames(2..2).frames(), 0);
        assert_eq!(block.slice_channels(4..4).channels(), 0);
    });
    no_alloc(|| {
        let mut block = AudioBlockMut::new(&mut data, 4, 4, 4).unwrap();
        block
            .slice_channels_mut(3..4)
            .slice_frames_mut(2..4)
            .fill(-1.0);
        block.reborrow().channel_mut(0)[0] = 42.0;
    });
    assert_eq!(data[0], 42.0);
    assert_eq!(data[12..], [12.0, 13.0, -1.0, -1.0]);
}

#[test]
fn b1_channel_mut_and_padded_mut_have_frames_and_stride_lengths() {
    let mut data = [0.0_f64; 16];
    let mut block = AudioBlockMut::new(&mut data, 2, 5, 8).unwrap();
    assert_eq!(block.channel_mut(1).len(), 5);
    assert_eq!(block.channel_padded_mut(1).len(), 8);
    block.channel_padded_mut(1).fill(1.0);
    block.channel_mut(0).fill(2.0);
    assert_eq!(block.as_block().channel(0), &[2.0; 5]);
    assert_eq!(block.channel(1), &[1.0; 5]);
    assert_eq!(data[5..8], [0.0; 3]);
    assert_eq!(data[8..], [1.0; 8]);
}

#[test]
fn b1_frame_sub_ranges_keep_padding_only_at_the_end() {
    let mut data = [0.0_f32; 32];
    let mut block = AudioBlockMut::new(&mut data, 2, 10, 16).unwrap();
    let mut head = block.slice_frames_mut(0..4);
    assert_eq!(head.padded_frames(), 4);
    head.channel_padded_mut(0).fill(1.0);
    let tail = block.slice_frames_mut(4..10);
    assert_eq!(tail.padded_frames(), 12);
    let mut tail = block.slice_frames_mut(4..10);
    assert_eq!(tail.slice_frames_mut(1..3).padded_frames(), 2);
    assert_eq!(tail.slice_frames_mut(1..6).padded_frames(), 11);
    assert_eq!(data[..5], [1.0, 1.0, 1.0, 1.0, 0.0]);
}

#[test]
fn b1_split_channels_mut_yields_disjoint_slices() {
    let mut data = [0.0_f32; 12];
    let mut block = AudioBlockMut::new(&mut data, 3, 3, 4).unwrap();
    no_alloc(|| {
        let mut it: ChannelsMut<'_, f32> = block.split_channels_mut();
        assert_eq!(it.len(), 3);
        let a = it.next().unwrap();
        let b = it.next().unwrap();
        let c = it.next().unwrap();
        assert!(it.next().is_none());
        a.fill(1.0);
        b.copy_from_slice(a);
        c.iter_mut().zip(b.iter()).for_each(|(c, b)| *c = b * 3.0);
    });
    assert_eq!(data, [1., 1., 1., 0., 1., 1., 1., 0., 3., 3., 3., 0.]);
}

/// `iter` must not capture the borrow of the view itself (edition 2024 `impl Trait` capture).
fn channels_of(block: AudioBlock<'_, f32>) -> impl Iterator<Item = &[f32]> {
    block.iter()
}

#[test]
fn b1_iter_outlives_the_view_it_came_from() {
    let data = ramp::<8>();
    let block = AudioBlock::new(&data, 2, 3, 4).unwrap();
    let firsts: Vec<f32> = channels_of(block).map(|c| c[0]).collect();
    assert_eq!(firsts, [0.0, 4.0]);
}

#[test]
fn b1_views_are_generic_over_f32_and_f64() {
    fn sum<T: AudioSample + Into<f64>>(b: AudioBlock<'_, T>) -> f64 {
        b.iter().flatten().map(|&s| s.into()).sum()
    }
    assert_eq!(sum(AudioBlock::new(&[1.0_f32; 4], 2, 2, 2).unwrap()), 4.0);
    assert_eq!(sum(AudioBlock::new(&[0.5_f64; 4], 1, 3, 4).unwrap()), 1.5);
    assert_eq!(<f32 as AudioSample>::PRECISION, Precision::Single);
}

#[test]
fn b1_invalid_layouts_are_rejected_with_catalogued_codes() {
    let data = [0.0_f32; 8];
    let err = AudioBlock::new(&data, 1, 5, 4).unwrap_err();
    assert_eq!(
        err,
        AudioError::FramesExceedStride {
            frames: 5,
            stride: 4
        }
    );
    assert_eq!(err.code().as_str(), "SYRP-AUDIO-0001");
    let err = AudioBlock::new(&data, 3, 4, 4).unwrap_err();
    assert_eq!(err, AudioError::BufferTooShort { needed: 12, len: 8 });
    assert_eq!(err.code().as_str(), "SYRP-AUDIO-0002");
    let err = AudioBlock::new(&data, usize::MAX, 1, 2).unwrap_err();
    assert_eq!(
        err,
        AudioError::BufferTooShort {
            needed: usize::MAX,
            len: 8
        }
    );
}

#[test]
#[should_panic(expected = "channel 2 out of range")]
fn b1_channel_out_of_range_panics() {
    let data = [0.0_f32; 8];
    let _ = AudioBlock::new(&data, 2, 4, 4).unwrap().channel(2);
}

#[test]
fn b3_process_config_holds_rate_block_precision_and_mode() {
    let cfg = ProcessConfig::new(96_000.0, 256).unwrap();
    assert_eq!((cfg.sample_rate(), cfg.max_block_size()), (96_000.0, 256));
    assert_eq!(
        (cfg.precision(), cfg.mode()),
        (Precision::Single, ProcessMode::Realtime)
    );
    assert!(!cfg.is_offline());
    let off = cfg
        .with_precision(Precision::Double)
        .with_mode(ProcessMode::Offline);
    assert_eq!(off.precision(), Precision::Double);
    assert!(off.is_offline());
    assert_eq!(off.sample_rate(), 96_000.0);
}

#[test]
fn b3_invalid_configs_are_rejected_with_catalogued_codes() {
    for rate in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let err = ProcessConfig::new(rate, 64).unwrap_err();
        assert_eq!(err.code().as_str(), "SYRP-AUDIO-0003");
    }
    let err = ProcessConfig::new(48_000.0, 0).unwrap_err();
    assert_eq!(err, AudioError::InvalidBlockSize);
    assert_eq!(err.code().as_str(), "SYRP-AUDIO-0004");
}

#[test]
fn b5_whole_aligned_buffers_are_simd_aligned() {
    let mut f32s = Aligned([0.0_f32; 64]);
    let block = AudioBlockMut::new(&mut f32s.0, 4, 10, 16).unwrap();
    assert!(block.is_simd_aligned());
    assert!(block.as_block().slice_channels(1..3).is_simd_aligned());
    let f64s = Aligned([0.0_f64; 32]);
    assert!(AudioBlock::new(&f64s.0, 4, 8, 8).unwrap().is_simd_aligned());
    // SAFETY: four channels of stride 8 inside `f64s`, which outlives the view.
    let raw = unsafe { AudioBlock::from_raw_parts_aligned(f64s.0.as_ptr(), 4, 5, 8) };
    assert!(raw.is_simd_aligned());
}

#[test]
fn b5_unaligned_strides_and_sub_ranges_report_it() {
    let data = Aligned([0.0_f32; 64]);
    let whole = AudioBlock::new(&data.0, 4, 16, 16).unwrap();
    assert!(!whole.slice_frames(3..16).is_simd_aligned());
    assert!(whole.slice_frames(0..5).is_simd_aligned());
    assert!(
        !AudioBlock::new(&data.0, 4, 12, 12)
            .unwrap()
            .is_simd_aligned()
    );
    assert!(
        !AudioBlock::new(&data.0[1..], 2, 16, 16)
            .unwrap()
            .is_simd_aligned()
    );
    // SAFETY: two channels of stride 16 inside `data`, which outlives the view.
    let raw = unsafe { AudioBlock::from_raw_parts(data.0.as_ptr().add(1), 2, 16, 16) };
    assert!(!raw.is_simd_aligned());
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "alignment contract")]
fn b5_from_raw_parts_aligned_panics_on_unaligned_memory_in_debug() {
    let mut data = Aligned([0.0_f32; 64]);
    // SAFETY: two channels of stride 12 inside `data`, borrowed exclusively.
    let _ = unsafe { AudioBlockMut::from_raw_parts_aligned(data.0.as_mut_ptr(), 2, 12, 12) };
}

#[test]
fn b6_audio_thread_types_are_rt_safe() {
    fn rt<T: RtSafe>() {}
    rt::<AudioBlock<'static, f32>>();
    rt::<AudioBlockMut<'static, f64>>();
    rt::<ChannelsMut<'static, f32>>();
    rt::<ProcessConfig>();
    rt::<Precision>();
    rt::<ProcessMode>();
    fn send_sync<T: Send + Sync>() {}
    send_sync::<AudioBlock<'static, f32>>();
    send_sync::<AudioBlockMut<'static, f64>>();
}

#[test]
fn b7_contract_types_have_no_public_fields() {
    let src = [
        include_str!("../src/lib.rs"),
        include_str!("../src/block.rs"),
        include_str!("../src/config.rs"),
    ];
    let mut in_struct = false;
    for line in src.iter().flat_map(|s| s.lines()) {
        if line.starts_with("pub struct") {
            assert!(!line.contains("(pub"), "public tuple field: {line}");
            in_struct = line.ends_with('{');
        } else if line.starts_with('}') {
            in_struct = false;
        } else if in_struct {
            assert!(
                !line.trim_start().starts_with("pub"),
                "public field: {line}"
            );
        }
    }
}

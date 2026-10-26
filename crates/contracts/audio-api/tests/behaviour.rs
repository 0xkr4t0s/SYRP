// SPDX-License-Identifier: MIT OR Apache-2.0
//! One or more tests per behaviour in work/specs/WP-0.2-02.md.

use syrp_audio_api::{
    AudioBlock, AudioBlockMut, AudioError, AudioSample, Bus, BusLayout, ChannelSet, ChannelsMut,
    LayoutResponse, LoopPoints, PlayHead, PlayHeadInfo, Precision, ProcessConfig, ProcessMode,
    Speaker, TimeSignature,
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
fn b1_by_value_accessors_lend_data_for_the_full_lifetime() {
    let mut data = ramp::<8>();
    let (a, b) = {
        let block = AudioBlockMut::new(&mut data, 2, 3, 4).unwrap();
        let mut it = block.into_split_channels_mut();
        (it.next().unwrap(), it.next().unwrap())
    };
    a.swap_with_slice(b);
    let tail: &mut [f32] = AudioBlockMut::new(&mut data, 2, 3, 4)
        .unwrap()
        .into_slice_channels(1..2)
        .into_slice_frames(1..3)
        .into_channel_mut(0);
    tail.fill(-1.0);
    let ro = AudioBlockMut::new(&mut data, 2, 3, 4).unwrap().into_block();
    assert_eq!(
        (ro.channel(0), ro.channel(1)),
        (&[4.0, 5.0, 6.0][..], &[0.0, -1.0, -1.0][..])
    );
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
fn b2_named_layouts_have_the_documented_speakers() {
    let cases: [(ChannelSet, usize, &str); 12] = [
        (ChannelSet::disabled(), 0, "Disabled"),
        (ChannelSet::mono(), 1, "Mono"),
        (ChannelSet::stereo(), 2, "Stereo"),
        (ChannelSet::lcr(), 3, "LCR"),
        (ChannelSet::quadraphonic(), 4, "Quadraphonic"),
        (ChannelSet::surround_5_0(), 5, "5.0"),
        (ChannelSet::surround_5_1(), 6, "5.1"),
        (ChannelSet::surround_6_0(), 6, "6.0"),
        (ChannelSet::surround_6_1(), 7, "6.1"),
        (ChannelSet::surround_7_0(), 7, "7.0"),
        (ChannelSet::surround_7_1(), 8, "7.1"),
        (ChannelSet::surround_7_1_4(), 12, "7.1.4"),
    ];
    for (set, n, name) in cases {
        assert_eq!((set.channel_count(), set.name()), (n, name));
        assert_eq!(set.speakers().len(), n);
        assert!(!set.is_discrete());
        for (i, s) in set.speakers().enumerate() {
            assert_eq!(set.index_of(s), Some(i), "{name}: {s:?} appears once");
        }
    }
    let s71: Vec<Speaker> = ChannelSet::surround_7_1().speakers().collect();
    use Speaker::*;
    assert_eq!(
        s71,
        [
            Left,
            Right,
            Centre,
            Lfe,
            LeftSurround,
            RightSurround,
            LeftRearSurround,
            RightRearSurround
        ]
    );
    assert_eq!(ChannelSet::mono().speaker(0), Some(Centre));
    assert_eq!(ChannelSet::stereo().speaker(2), None);
    assert_eq!(ChannelSet::stereo().index_of(Centre), None);
    assert!(ChannelSet::disabled().is_disabled());
}

#[test]
fn b2_discrete_sets_number_their_channels() {
    let set = ChannelSet::discrete(5);
    assert!(set.is_discrete());
    assert_eq!(set.channel_count(), 5);
    assert_eq!(set.speaker(4), Some(Speaker::Discrete(4)));
    assert_eq!(set.speaker(5), None);
    assert_eq!(set.index_of(Speaker::Discrete(3)), Some(3));
    assert_eq!(set.index_of(Speaker::Discrete(5)), None);
    assert_eq!(set.index_of(Speaker::Left), None);
    assert_eq!(ChannelSet::discrete(0), ChannelSet::disabled());
    assert_ne!(ChannelSet::discrete(2), ChannelSet::stereo());
    assert_eq!(ChannelSet::canonical(2), ChannelSet::stereo());
    assert_eq!(ChannelSet::canonical(12), ChannelSet::surround_7_1_4());
    assert_eq!(ChannelSet::canonical(10), ChannelSet::discrete(10));
}

#[test]
fn b2_bus_layouts_keep_bus_order_names_and_sets() {
    let mut layout = BusLayout::new()
        .with_input("Main", ChannelSet::stereo())
        .with_input("Sidechain", ChannelSet::mono())
        .with_output("Main", ChannelSet::surround_5_1());
    let names: Vec<&str> = layout.inputs().iter().map(Bus::name).collect();
    assert_eq!(names, ["Main", "Sidechain"]);
    assert_eq!((layout.input_channels(), layout.output_channels()), (3, 6));
    layout.set_input(1, ChannelSet::disabled());
    layout.set_output(0, ChannelSet::stereo());
    assert_eq!(layout.inputs()[1].channel_set(), ChannelSet::disabled());
    assert_eq!(layout.output_channels(), 2);
    let renamed = BusLayout::new()
        .with_input("In", ChannelSet::stereo())
        .with_input("Key", ChannelSet::disabled())
        .with_output("Out", ChannelSet::stereo());
    assert!(layout.same_channel_sets(&renamed));
    assert_ne!(layout, renamed);
    assert!(!layout.same_channel_sets(&BusLayout::new()));
}

#[test]
fn b2_negotiation_results_carry_a_counter_proposal() {
    let stereo = BusLayout::new().with_output("Main", ChannelSet::stereo());
    let answer = |p: &BusLayout| match p.output_channels() {
        2 => LayoutResponse::Accepted,
        0 => LayoutResponse::Rejected,
        _ => LayoutResponse::Proposed(stereo.clone()),
    };
    assert_eq!(answer(&stereo), LayoutResponse::Accepted);
    assert_eq!(answer(&BusLayout::new()), LayoutResponse::Rejected);
    let big = BusLayout::new().with_output("Main", ChannelSet::surround_7_1());
    assert_eq!(answer(&big), LayoutResponse::Proposed(stereo.clone()));
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
fn b4_play_head_info_reports_only_what_was_set() {
    let empty = PlayHeadInfo::new();
    assert_eq!(empty, PlayHeadInfo::default());
    assert!(empty.bpm().is_none() && empty.time_signature().is_none());
    assert!(empty.ppq_position().is_none() && empty.bar_start_ppq().is_none());
    assert!(empty.loop_points().is_none() && empty.sample_position().is_none());
    assert!(empty.host_time_ns().is_none() && empty.time_in_seconds(48_000.0).is_none());
    assert!(!empty.is_playing() && !empty.is_recording() && !empty.is_looping());

    let ts = TimeSignature::new(6, 8).unwrap();
    let info = empty
        .with_bpm(140.0)
        .with_time_signature(ts)
        .with_ppq_position(9.5)
        .with_bar_start_ppq(9.0)
        .with_loop_points(LoopPoints::new(8.0, 16.0))
        .with_sample_position(-4_800)
        .with_host_time_ns(123)
        .with_playing(true)
        .with_recording(true)
        .with_looping(true);
    assert_eq!(info.bpm(), Some(140.0));
    assert_eq!(
        info.time_signature()
            .map(|t| (t.numerator(), t.denominator())),
        Some((6, 8))
    );
    assert_eq!(
        (info.ppq_position(), info.bar_start_ppq()),
        (Some(9.5), Some(9.0))
    );
    assert_eq!(
        info.loop_points().map(|l| (l.start_ppq(), l.end_ppq())),
        Some((8.0, 16.0))
    );
    assert_eq!(info.sample_position(), Some(-4_800));
    assert_eq!(info.time_in_seconds(48_000.0), Some(-0.1));
    assert_eq!(info.host_time_ns(), Some(123));
    assert!(info.is_playing() && info.is_recording() && info.is_looping());
    assert!(TimeSignature::new(0, 4).is_none());
}

#[test]
fn b4_a_play_head_reports_positions_without_allocating() {
    struct Fixed(PlayHeadInfo);
    impl PlayHead for Fixed {
        fn position(&self) -> Option<PlayHeadInfo> {
            Some(self.0)
        }
    }
    let head = Fixed(PlayHeadInfo::new().with_bpm(90.0));
    let bpm = no_alloc(|| head.position().and_then(|p| p.bpm()));
    assert_eq!(bpm, Some(90.0));
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
    rt::<ChannelSet>();
    rt::<Speaker>();
    rt::<PlayHeadInfo>();
    rt::<TimeSignature>();
    rt::<LoopPoints>();
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
        include_str!("../src/channels.rs"),
        include_str!("../src/playhead.rs"),
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

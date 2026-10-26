// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tests for B1, B2, B4 (change stream) and B7 of work/specs/WP-0.2-06.md.

use syrp_audio_api::{
    AudioBlockMut, BusLayout, ChannelSet, LayoutResponse, PlayHeadInfo, Precision, ProcessConfig,
};
use syrp_error::Diagnostic;
use syrp_midi::{MidiBuffer, UmpBuffer, UmpPacket};
use syrp_plugin_api::{
    ParamChange, ParamChanges, ParamIndex, ParamLayout, ParamRange, ParamSpec, Params, PluginError,
    PrepareCtx, ProcessIo, Processor, RtCtx, Tail, prepare_processor,
};
use syrp_rt::no_alloc;

#[global_allocator]
static ALLOC: syrp_rt::GuardedAllocator = syrp_rt::GuardedAllocator;

/// Mono in, stereo out: sample-accurate level changes, MIDI echoed an octave up, UMP echoed.
#[derive(Default)]
struct Echo {
    level: Option<ParamIndex>,
    seen_changes: Vec<(u32, u32)>,
    double: bool,
}

impl Processor for Echo {
    fn register_params(&mut self, layout: &mut ParamLayout) -> Result<(), PluginError> {
        layout.register(ParamSpec::new("level", "Level", ParamRange::new(0.0, 1.0)))?;
        Ok(())
    }
    fn default_layout(&self) -> BusLayout {
        let layout = BusLayout::new().with_input("In", ChannelSet::mono());
        layout.with_output("Out", ChannelSet::stereo())
    }
    fn supports_double_precision(&self) -> bool {
        self.double
    }
    fn prepare(&mut self, _: &ProcessConfig, cx: &mut PrepareCtx<'_>) -> Result<(), PluginError> {
        self.level = Some(cx.param_index("level").ok_or(PluginError::PrepareFailed)?);
        cx.reserve_param_changes(32);
        cx.reserve_param_changes(4); // requests only raise the capacity
        self.seen_changes = Vec::with_capacity(32);
        Ok(())
    }
    fn process(&mut self, io: &mut ProcessIo<'_>, params: &Params, rt: &mut RtCtx<'_>) {
        self.seen_changes.clear();
        let changes = io.param_changes();
        let seen = changes.iter().map(|c| (c.sample_offset(), c.index().get()));
        self.seen_changes.extend(seen);
        let (frames, level) = (io.frames(), self.level.unwrap());
        let mut value = params.get(level);
        if let Some(mut audio) = io.audio_f32() {
            let mut channels = audio.split_channels_mut();
            let left = channels.next().unwrap();
            for (t, s) in (0u32..).zip(left.iter_mut()) {
                value = changes.range(t..t + 1).last().map_or(value, |c| c.value());
                *s *= value;
            }
            channels.next().unwrap().copy_from_slice(&left[..frames]);
        }
        let bpm = rt.play_head().and_then(|p| p.bpm()).unwrap_or(0.0) as u8;
        let midi_in = io.midi_in();
        if let (Some(input), Some(out)) = (midi_in, io.midi_out()) {
            for e in input {
                let b = e.as_slice();
                out.try_add_event(&[b[0], b[1] + 12, bpm], e.sample_offset())
                    .unwrap();
            }
        }
        let ump_in = io.ump_in();
        if let (Some(input), Some(out)) = (ump_in, io.ump_out()) {
            for e in input {
                out.try_add(e.packet(), e.sample_offset()).unwrap();
            }
        }
    }
    fn latency_samples(&self) -> u32 {
        64
    }
    fn tail(&self) -> Tail {
        Tail::Samples(48_000)
    }
    fn bypass_param(&self) -> Option<ParamIndex> {
        self.level
    }
}

fn change(t: u32, index: u32, value: f32) -> ParamChange {
    ParamChange::new(t, ParamIndex::new(index), value)
}

fn block(data: &mut [f32], channels: usize) -> AudioBlockMut<'_, f32> {
    let frames = data.len() / channels;
    AudioBlockMut::new(data, channels, frames, frames).unwrap()
}

#[track_caller]
fn fails_with<T: std::fmt::Debug>(result: Result<T, PluginError>, err: PluginError, code: &str) {
    assert_eq!(result.unwrap_err(), err);
    assert_eq!(err.code().as_str(), code);
}

fn prepared() -> (Echo, ParamLayout, Params) {
    let mut echo = Echo::default();
    let mut layout = ParamLayout::new();
    echo.register_params(&mut layout).unwrap();
    let config = ProcessConfig::new(48_000.0, 8).unwrap();
    let prepared = prepare_processor(&mut echo, &config, &layout).unwrap();
    assert_eq!(prepared.param_change_capacity(), 32);
    let params = Params::new(&layout).unwrap();
    (echo, layout, params)
}

#[test]
fn b1_prepare_resolves_ids_and_allocates_then_process_runs_without_allocating() {
    let (mut echo, _, params) = prepared();
    assert_eq!(echo.level, Some(ParamIndex::new(0)));
    params.set(ParamIndex::new(0), 0.5);
    let mut data = [1.0f32; 16];
    no_alloc(|| {
        let mut io = ProcessIo::new(block(&mut data, 2), 1);
        echo.process(&mut io, &params, &mut RtCtx::new());
    });
    assert_eq!(data, [0.5; 16]);
    let config = ProcessConfig::new(44_100.0, 64).unwrap();
    let result = prepare_processor(&mut echo, &config, &ParamLayout::new());
    fails_with(result, PluginError::PrepareFailed, "SYRP-PLUGIN-0006");
}

#[test]
fn b1_compile_fail_prepare_ctx_is_unreachable_from_process() {
    trybuild::TestCases::new().compile_fail("tests/ui/*.rs");
}

#[test]
fn b2_layout_negotiation_latency_tail_and_bypass_parameter() {
    let (echo, ..) = prepared();
    let mic = BusLayout::new().with_input("Mic", ChannelSet::mono());
    let mic = mic.with_output("Main", ChannelSet::stereo());
    assert_eq!(echo.supports_layout(&mic), LayoutResponse::Accepted);
    let stereo = BusLayout::new().with_output("Main", ChannelSet::stereo());
    let proposed = LayoutResponse::Proposed(echo.default_layout());
    assert_eq!(echo.supports_layout(&stereo), proposed);
    assert_eq!(echo.latency_samples(), 64);
    assert_eq!(echo.tail(), Tail::Samples(48_000));
    assert_eq!(echo.bypass_param(), Some(ParamIndex::new(0)));
}

#[test]
fn b2_double_precision_needs_support() {
    let (mut echo, layout, params) = prepared();
    let config = ProcessConfig::new(48e3, 8).unwrap();
    let config = config.with_precision(Precision::Double);
    let result = prepare_processor(&mut echo, &config, &layout);
    let unsupported = PluginError::PrecisionUnsupported;
    fails_with(result, unsupported, "SYRP-PLUGIN-0005");
    echo.double = true;
    prepare_processor(&mut echo, &config, &layout).unwrap();
    let mut data = [3.0f64; 4];
    let mut io = ProcessIo::new_f64(AudioBlockMut::new(&mut data, 2, 2, 2).unwrap(), 1);
    assert!(io.audio_f32().is_none());
    no_alloc(|| echo.process_bypassed(&mut io, &params, &mut RtCtx::new()));
    assert_eq!(data, [3.0, 3.0, 0.0, 0.0]);
}

#[test]
fn b4_changes_are_sorted_by_offset_and_stable_for_equal_offsets() {
    let mut changes = ParamChanges::with_capacity(8).unwrap();
    let pushes = [(64, 0), (0, 1), (64, 2), (10, 0), (64, 1), (0, 2)];
    for (t, i) in pushes {
        changes.try_push(change(t, i, 0.0)).unwrap();
    }
    let got: Vec<_> = changes
        .iter()
        .map(|c| (c.sample_offset(), c.index().get()))
        .collect();
    assert_eq!(got, [(0, 1), (0, 2), (10, 0), (64, 0), (64, 2), (64, 1)]);
    assert_eq!(changes.range(1..64).len(), 1);
    assert_eq!(changes.range(64..u32::MAX).len(), 3);
}

#[test]
fn b4_filling_to_capacity_never_allocates() {
    let mut changes = ParamChanges::with_capacity(256).unwrap();
    no_alloc(|| {
        for block in 0..3 {
            changes.clear();
            for k in 0..256u32 {
                changes
                    .try_push(change((k * 7919 + block) % 512, k % 5, 0.0))
                    .unwrap();
            }
            let full = changes.try_push(change(0, 0, 1.0));
            assert_eq!(full, Err(PluginError::ParamChangesFull));
            assert!(changes.iter().is_sorted_by_key(|c| c.sample_offset()));
        }
    });
    assert_eq!((changes.len(), changes.capacity()), (256, 256)); // a failed push changes nothing
    let result = changes.try_push(change(0, 0, 1.0));
    fails_with(result, PluginError::ParamChangesFull, "SYRP-PLUGIN-0004");
    let result = ParamChanges::with_capacity(usize::MAX);
    fails_with(result, PluginError::AllocationFailed, "SYRP-PLUGIN-0003");
}

#[test]
fn b4_process_io_exposes_one_change_stream_applied_sample_accurately() {
    let (mut echo, _, params) = prepared();
    let mut changes = ParamChanges::with_capacity(32).unwrap();
    changes.try_push(change(4, 0, 0.25)).unwrap();
    changes.try_push(change(2, 0, 1.0)).unwrap();
    let mut data = [2.0f32; 16];
    no_alloc(|| {
        let mut io = ProcessIo::new(block(&mut data, 2), 1).with_param_changes(&changes);
        echo.process(&mut io, &params, &mut RtCtx::new());
    });
    assert_eq!(echo.seen_changes, [(2, 0), (4, 0)]);
    assert_eq!(data[..8], [0.0, 0.0, 2.0, 2.0, 0.5, 0.5, 0.5, 0.5]);
    assert_eq!(data[8..], data[..8]);
}

#[test]
fn b7_midi_and_ump_events_flow_in_and_out_without_allocating() {
    let (mut echo, _, params) = prepared();
    let mut midi_in = MidiBuffer::with_capacity(4, 16);
    midi_in.try_add_event(&[0x90, 60, 100], 3).unwrap();
    midi_in.try_add_event(&[0x80, 60, 0], 7).unwrap();
    let mut ump_in = UmpBuffer::with_capacity(4, 8);
    let packet = UmpPacket::midi1_channel_voice(0, 0x91, 64, 90).unwrap();
    ump_in.try_add(packet, 5).unwrap();
    let mut midi_out = MidiBuffer::with_capacity(4, 16);
    let mut ump_out = UmpBuffer::with_capacity(4, 8);
    let mut data = [0.0f32; 16];
    no_alloc(|| {
        let io = ProcessIo::new(block(&mut data, 2), 1).with_midi(&midi_in, &mut midi_out);
        let mut io = io.with_ump(&ump_in, &mut ump_out);
        let mut rt = RtCtx::new().with_play_head(PlayHeadInfo::new().with_bpm(120.0));
        echo.process(&mut io, &params, &mut rt);
    });
    let out: Vec<_> = midi_out
        .iter()
        .map(|e| (e.as_slice(), e.sample_offset()))
        .collect();
    assert_eq!(out, [(&[0x90, 72, 120][..], 3), (&[0x80, 72, 120][..], 7)]);
    let ump: Vec<_> = ump_out
        .iter()
        .map(|e| (e.packet(), e.sample_offset()))
        .collect();
    assert_eq!(ump, [(packet, 5)]);
}

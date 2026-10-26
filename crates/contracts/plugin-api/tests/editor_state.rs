// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tests for B5 (editor contract) and B6 (descriptor and state) of work/specs/WP-0.2-06.md.

use std::any::Any;
use std::sync::{Arc, Mutex};

use proptest::prelude::*;
use syrp_error::Diagnostic;
use syrp_plugin_api::*;

/// Stands in for the L5 runtime's `EditorHost` over `syrp-ui`'s `Ui`.
struct FakeUi {
    nodes: Vec<&'static str>,
}

impl EditorHost for FakeUi {
    fn root(&self) -> NodeHandle {
        NodeHandle::new(0, 1)
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// An editor that adds one knob per parameter through the typed accessor a runtime offers.
#[derive(Default)]
struct Knobs {
    size: Option<(f32, f32)>,
    destroyed: bool,
}

impl Editor for Knobs {
    fn create(
        &mut self,
        host: &mut dyn EditorHost,
        p: &ParamHandles,
    ) -> Result<NodeHandle, EditorError> {
        let root = host.root();
        let ui = host
            .as_any_mut()
            .downcast_mut::<FakeUi>()
            .ok_or(EditorError::Unsupported)?;
        ui.nodes.extend((0..p.len()).map(|_| "knob"));
        let gain = p.index_of("gain").ok_or(EditorError::CreationFailed)?;
        p.begin_gesture(gain);
        p.set(gain, 0.7);
        p.set_normalised(gain, 0.25);
        p.end_gesture(gain);
        Ok(root)
    }
    fn constraints(&self) -> SizeConstraints {
        SizeConstraints::resizable((400.0, 200.0), (200.0, 100.0), (800.0, 400.0))
            .with_aspect_ratio(2.0)
    }
    fn resized(&mut self, _: &mut dyn EditorHost, size: (f32, f32)) {
        self.size = Some(size);
    }
    fn destroy(&mut self, _: &mut dyn EditorHost) {
        self.destroyed = true;
    }
}

struct Recorder(Mutex<Vec<(u32, GesturePhase, f32)>>);

impl ParamGestures for Recorder {
    fn edited(&self, index: ParamIndex, phase: GesturePhase, value: f32) {
        self.0.lock().unwrap().push((index.get(), phase, value));
    }
}

#[track_caller]
fn coded<E: Diagnostic + PartialEq + std::fmt::Debug>(err: E, want: E, code: &str) {
    assert_eq!(err, want);
    assert_eq!(err.code().as_str(), code);
}

fn layout() -> ParamLayout {
    let mut layout = ParamLayout::new();
    let range = ParamRange::new(0.0, 2.0).with_step(0.1);
    layout
        .register(ParamSpec::new("gain", "Gain", range).with_default(1.0))
        .unwrap();
    layout
}

#[test]
fn b5_editor_lifecycle_builds_through_an_opaque_host_and_shares_values() {
    let layout = Arc::new(layout());
    let values = Arc::new(Params::new(&layout).unwrap());
    let recorder = Arc::new(Recorder(Mutex::new(Vec::new())));
    let handles = ParamHandles::shared(Arc::clone(&layout), Arc::clone(&values))
        .with_gestures(recorder.clone());
    let (mut ui, mut editor) = (FakeUi { nodes: Vec::new() }, Knobs::default());
    let (w, h) = editor.constraints().preferred();
    let root = editor.create(&mut ui, &handles).unwrap();
    assert_eq!(root, NodeHandle::new(0, 1));
    assert_eq!(ui.nodes, ["knob"]);
    // The processor's value array sees the editor's edits, snapped to the step.
    assert_eq!(values.get(ParamIndex::new(0)), 0.5);
    let log = recorder.0.lock().unwrap().clone();
    let tenths = log
        .iter()
        .map(|&(i, p, v)| (i, p, (v * 10.0).round() as u8));
    let tenths: Vec<_> = tenths.collect();
    use GesturePhase::*;
    let want = [(0, Begin, 10), (0, Change, 7), (0, Change, 5), (0, End, 5)];
    assert_eq!(tenths, want);
    let size = editor.constraints().clamp(w * 4.0, h);
    editor.resized(&mut ui, size);
    editor.destroy(&mut ui);
    assert_eq!(editor.size, Some((800.0, 400.0)));
    assert!(editor.destroyed);
    assert_eq!(handles.text(ParamIndex::new(0)).as_deref(), Some("0.5"));
}

#[test]
fn b5_editor_errors_carry_codes_and_handles_ignore_foreign_indices() {
    struct Other;
    impl EditorHost for Other {
        fn root(&self) -> NodeHandle {
            NodeHandle::new(3, 0)
        }
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
    }
    let handles = ParamHandles::new(&layout()).unwrap();
    let err = Knobs::default().create(&mut Other, &handles).unwrap_err();
    coded(err, EditorError::Unsupported, "SYRP-PLUGIN-0008");
    let failed = EditorError::CreationFailed;
    coded(failed, failed, "SYRP-PLUGIN-0007");
    let empty_values = Arc::new(Params::new(&ParamLayout::new()).unwrap());
    let mismatched = ParamHandles::shared(Arc::new(layout()), empty_values);
    mismatched.set(ParamIndex::new(0), 1.0); // a layout/value mismatch is ignored, not a panic
    assert!(mismatched.get(ParamIndex::new(0)).is_nan());
    fn object_safe(_: &mut dyn EditorHost, _: &dyn Editor) {}
    object_safe(&mut Other, &Knobs::default());
}

#[test]
fn b5_size_constraints_clamp_and_keep_the_aspect_ratio() {
    let c = Knobs::default().constraints();
    assert!(c.is_resizable());
    assert_eq!(c.clamp(100.0, 900.0), (200.0, 100.0));
    assert_eq!(c.clamp(500.0, 0.0), (500.0, 250.0));
    let fixed = SizeConstraints::fixed(300.0, 150.0);
    assert!(!fixed.is_resizable());
    assert_eq!(fixed.clamp(f32::NAN, f32::NAN), (300.0, 150.0));
}

proptest! {
    #[test]
    fn b5_node_handle_round_trips_any_index_and_generation(index: u32, generation: u32) {
        let node = NodeHandle::new(index, generation);
        prop_assert_eq!((node.index(), node.generation()), (index, generation));
        prop_assert_eq!(NodeHandle::from_bits(node.to_bits()), node);
        prop_assert_eq!(NodeHandle::from_bits(node.to_bits()).to_bits(), node.to_bits());
    }
}

#[test]
fn b6_descriptor_records_and_validates_ids_per_format() {
    let d = PluginDescriptor::new("Gain", "Example", (1, 4, 2))
        .with_id(PluginFormat::Clap, "com.example.gain")
        .with_id(PluginFormat::Vst3, "0123456789abcdef0123456789ABCDEF")
        .with_id(PluginFormat::AudioUnit, "Gain")
        .with_category(PluginCategory::Effect)
        .with_category(PluginCategory::Dynamics);
    assert_eq!([d.name(), d.vendor()], ["Gain", "Example"]);
    assert_eq!(d.version(), (1, 4, 2));
    assert_eq!(d.id(PluginFormat::AudioUnit), Some("Gain"));
    assert_eq!((d.id(PluginFormat::Lv2), d.categories().len()), (None, 2));
    assert_eq!(d.validate(), Ok(()));
    let base = || PluginDescriptor::new("Gain", "Example", (1, 0, 0));
    let bad = [
        base(),
        PluginDescriptor::new(" ", "Example", (1, 0, 0)).with_id(PluginFormat::Clap, "a.b"),
        base().with_id(PluginFormat::Clap, "has space"),
        base().with_id(PluginFormat::Vst3, "0123"),
        base().with_id(PluginFormat::AudioUnit, "Gain2"),
    ];
    for d in bad {
        let err = d.validate().unwrap_err();
        coded(err, PluginError::InvalidDescriptor, "SYRP-PLUGIN-0009");
    }
}

/// Version 2 stores `[gain, mode]`; version 1 stored only `[gain]`.
struct Synth {
    gain: u8,
    mode: u8,
}

impl PluginState for Synth {
    fn state_version(&self) -> u32 {
        2
    }
    fn save_state(&self, out: &mut Vec<u8>) -> Result<(), PluginError> {
        out.extend_from_slice(&[self.gain, self.mode]);
        Ok(())
    }
    fn load_state(&mut self, version: u32, data: &[u8]) -> Result<(), PluginError> {
        (self.gain, self.mode) = match (version, data) {
            (1, &[gain]) => (gain, 0),
            (2, &[gain, mode]) => (gain, mode),
            _ => return Err(PluginError::StateMalformed),
        };
        Ok(())
    }
}

#[test]
fn b6_state_round_trips_in_a_versioned_envelope_and_migrates_older_payloads() {
    let blob = save_state(&Synth { gain: 9, mode: 3 }).unwrap();
    assert_eq!(&blob[..8], b"SYST\x01\x00\x00\x00");
    assert_eq!(blob.len(), 22);
    assert_eq!(StateEnvelope::parse(&blob).unwrap().version(), 2);
    let mut synth = Synth { gain: 0, mode: 0 };
    load_state(&mut synth, &blob).unwrap();
    assert_eq!((synth.gain, synth.mode), (9, 3));
    let mut v1 = Vec::new();
    StateEnvelope::write(1, &[4], &mut v1);
    load_state(&mut synth, &v1).unwrap();
    assert_eq!((synth.gain, synth.mode), (4, 0));
}

#[test]
fn b6_state_rejects_newer_and_malformed_blobs_without_changing_the_plugin() {
    let mut synth = Synth { gain: 1, mode: 1 };
    let mut newer = Vec::new();
    StateEnvelope::write(3, &[0, 0, 0], &mut newer);
    let mut newer_format = save_state(&synth).unwrap();
    newer_format[4] = 2;
    for blob in [&newer, &newer_format] {
        let err = load_state(&mut synth, blob).unwrap_err();
        coded(err, PluginError::StateTooNew, "SYRP-PLUGIN-0011");
    }
    let good = save_state(&synth).unwrap();
    let mut reserved = good.clone();
    reserved[6] = 1;
    let mut magic = good.clone();
    magic[0] = b'J';
    let mut bad_payload = Vec::new();
    StateEnvelope::write(2, &[1], &mut bad_payload);
    let truncated = &good[..good.len() - 1];
    let mut extended = good.clone();
    extended.push(0);
    for blob in [
        &good[..10],
        truncated,
        &extended,
        &reserved,
        &magic,
        &bad_payload,
    ] {
        let err = load_state(&mut synth, blob).unwrap_err();
        coded(err, PluginError::StateMalformed, "SYRP-PLUGIN-0010");
    }
    assert_eq!((synth.gain, synth.mode), (1, 1));
}

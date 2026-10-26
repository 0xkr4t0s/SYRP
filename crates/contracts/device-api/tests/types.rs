// SPDX-License-Identifier: MIT OR Apache-2.0
//! Behaviour tests for WP-0.2-05: device types (B1), MIDI ports (B4) and change
//! notifications (B5), using the fakes (B6).

use std::sync::{Arc, Mutex};

use syrp_device_api::fake::{FakeAudioDeviceType, FakeAudioSpec, FakeMidiDeviceType};
use syrp_device_api::{
    AudioDeviceSetup, AudioDeviceType, ChannelMask, DeviceChange, DeviceChangeNotifier,
    DeviceError, DeviceId, MidiDeviceType,
};
use syrp_loop_api::{ManualLoop, MessageLoop};
use syrp_midi::MidiMessage;

type Log<T> = Arc<Mutex<Vec<T>>>;

/// A notifier on `lp` that records every change it is told about.
fn recorder(lp: &ManualLoop) -> (DeviceChangeNotifier, Log<DeviceChange>) {
    let log = Log::default();
    let l = Arc::clone(&log);
    let n = DeviceChangeNotifier::new(lp.task_sender(), move |_, c| {
        l.lock().unwrap().push(c.clone())
    });
    (n, log)
}

#[test]
fn b1_device_type_enumerates_and_creates_devices() {
    let ty: Box<dyn AudioDeviceType> = Box::new(FakeAudioDeviceType::new());
    assert_eq!(
        (ty.name(), ty.devices().len(), ty.default_device()),
        ("Fake", 0, None)
    );
    let fake = FakeAudioDeviceType::new();
    fake.add_device(FakeAudioSpec::new("Built-in", 2, 2));
    fake.add_device(FakeAudioSpec::new("Interface", 8, 8));
    let names: Vec<_> = fake.devices().iter().map(|d| d.name().to_owned()).collect();
    assert_eq!(names, ["Built-in", "Interface"]);
    assert_eq!(fake.default_device(), Some(DeviceId::new("fake:Built-in")));
    let mut dev = fake.create(&DeviceId::new("fake:Interface")).unwrap();
    assert_eq!(dev.input_channel_names().len(), 8);
    let setup = AudioDeviceSetup::new(48e3, 64).with_outputs(ChannelMask::first(8));
    dev.open(&setup).unwrap();
    assert_eq!(
        fake.create(&DeviceId::new("fake:Nope")).err(),
        Some(DeviceError::NotFound)
    );
}

#[test]
fn b1_removing_a_device_disconnects_devices_created_from_it() {
    let ty = FakeAudioDeviceType::new();
    ty.add_device(FakeAudioSpec::new("USB", 0, 2));
    let id = DeviceId::new("fake:USB");
    let mut dev = ty.create(&id).unwrap();
    assert!(ty.remove_device(&id));
    assert!(ty.devices().is_empty() && !ty.remove_device(&id));
    let setup = AudioDeviceSetup::new(48e3, 64).with_outputs(ChannelMask::first(2));
    assert_eq!(dev.open(&setup), Err(DeviceError::Disconnected));
}

#[test]
fn b4_inputs_deliver_timestamped_messages_only_while_started() {
    let ty = FakeMidiDeviceType::new(&["Keys"], &[]);
    let id = ty.inputs()[0].id().clone();
    let got: Log<MidiMessage> = Log::default();
    let g = Arc::clone(&got);
    let cb = Box::new(move |m: &MidiMessage| g.lock().unwrap().push(m.clone()));
    let mut input = ty.open_input(&id, cb).unwrap();
    let note = MidiMessage::note_on(1, 60, 100)
        .unwrap()
        .with_timestamp(1.5);
    assert_eq!(ty.deliver(&id, &note), 0, "stopped inputs discard");
    input.start().unwrap();
    assert_eq!(ty.deliver(&id, &note), 1);
    input.stop();
    assert_eq!(ty.deliver(&id, &note), 0);
    let got = got.lock().unwrap();
    assert_eq!((got.len(), got[0].timestamp()), (1, 1.5));
    assert_eq!(got[0], note);
}

#[test]
fn b4_outputs_send_and_ports_have_stable_ids() {
    let a = FakeMidiDeviceType::new(&["Keys"], &["Synth"]);
    let b = FakeMidiDeviceType::new(&["Pads", "Keys"], &["Synth"]);
    assert_eq!(a.inputs()[0].id(), b.inputs()[1].id(), "id is not an index");
    assert_eq!(a.outputs()[0].id().as_str(), "fake-out:Synth");
    let id = a.outputs()[0].id().clone();
    let mut out = a.open_output(&id).unwrap();
    let cc = MidiMessage::controller(2, 7, 99).unwrap();
    out.send(&cc).unwrap();
    assert_eq!(a.sent(&id), std::slice::from_ref(&cc));
    assert!(a.remove_port(&id));
    assert_eq!(out.send(&cc), Err(DeviceError::Disconnected));
    assert_eq!(a.open_output(&id).err(), Some(DeviceError::NotFound));
}

#[test]
fn b4_unplugged_input_cannot_start() {
    let ty = FakeMidiDeviceType::new(&["Keys"], &[]);
    let id = ty.inputs()[0].id().clone();
    let mut input = ty.open_input(&id, Box::new(|_: &MidiMessage| {})).unwrap();
    assert!(ty.remove_port(&id));
    assert_eq!(input.start(), Err(DeviceError::Disconnected));
    assert_eq!(ty.deliver(&id, &MidiMessage::note_on(1, 1, 1).unwrap()), 0);
}

#[test]
fn b5_audio_changes_are_posted_to_the_message_loop() {
    let lp = ManualLoop::new();
    let (n, log) = recorder(&lp);
    let ty = FakeAudioDeviceType::new();
    ty.set_change_notifier(Some(n));
    ty.add_device(FakeAudioSpec::new("A", 1, 1));
    ty.remove_device(&DeviceId::new("fake:A"));
    assert!(log.lock().unwrap().is_empty(), "never delivered inline");
    assert_eq!(lp.run_until_idle(), Ok(2));
    let a = DeviceId::new("fake:A");
    assert_eq!(
        *log.lock().unwrap(),
        [DeviceChange::Added(a.clone()), DeviceChange::Removed(a)]
    );
    ty.set_change_notifier(None);
    ty.add_device(FakeAudioSpec::new("B", 1, 1));
    assert_eq!(lp.run_until_idle(), Ok(0));
}

#[test]
fn b5_midi_changes_are_posted_from_any_thread() {
    let lp = ManualLoop::new();
    let (n, log) = recorder(&lp);
    let ty = Arc::new(FakeMidiDeviceType::new(&[], &["Synth"]));
    ty.set_change_notifier(Some(n));
    let t = Arc::clone(&ty);
    std::thread::spawn(move || t.add_input("Keys"))
        .join()
        .unwrap();
    ty.remove_port(&DeviceId::new("fake-out:Synth"));
    assert_eq!(lp.run_until_idle(), Ok(2));
    assert_eq!(
        *log.lock().unwrap(),
        [
            DeviceChange::Added(DeviceId::new("fake-in:Keys")),
            DeviceChange::Removed(DeviceId::new("fake-out:Synth")),
        ]
    );
}

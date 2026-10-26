// SPDX-License-Identifier: MIT OR Apache-2.0
//! Behaviour tests for WP-0.2-05 (audio devices: B2, B3, B6 and B7).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering::Relaxed};

use syrp_audio_api::{AudioBlock, AudioBlockMut};
use syrp_device_api::fake::{FakeAudioDevice, FakeAudioSpec};
use syrp_device_api::{
    AudioCallback, AudioDevice, AudioDeviceSetup, ChannelMask, DeviceError, DeviceRtCtx,
};
use syrp_error::Diagnostic;
use syrp_rt::{RtSafe, no_alloc};

type In<'a> = AudioBlock<'a, f32>;
type Out<'a> = AudioBlockMut<'a, f32>;
use DeviceError::{Disconnected, NotOpen, UnsupportedSetup as Unsupported};

#[global_allocator]
static ALLOC: syrp_rt::GuardedAllocator = syrp_rt::GuardedAllocator;

/// Counts `about_to_start`, `process` and `stopped` calls.
#[derive(Default, Clone)]
struct Probe(Arc<[AtomicUsize; 3]>);

impl AudioCallback for Probe {
    fn about_to_start(&mut self, _: &AudioDeviceSetup) {
        self.0[0].fetch_add(1, Relaxed);
    }
    fn process(&mut self, _: In<'_>, _: Out<'_>, _: &mut DeviceRtCtx<'_>) {
        self.0[1].fetch_add(1, Relaxed);
    }
    fn stopped(&mut self) {
        self.0[2].fetch_add(1, Relaxed);
    }
}

impl Probe {
    fn counts(&self) -> [usize; 3] {
        [0, 1, 2].map(|i| self.0[i].load(Relaxed))
    }
}

fn stereo(rate: f64, size: u32) -> AudioDeviceSetup {
    AudioDeviceSetup::new(rate, size).with_outputs(ChannelMask::first(2))
}

#[test]
fn b2_open_accepts_only_supported_rates_sizes_and_channels() {
    let mut dev = FakeAudioDevice::new(FakeAudioSpec::new("Duo", 2, 2));
    assert_eq!(dev.open(&stereo(12_345.0, 64)), Err(Unsupported));
    assert_eq!(dev.open(&stereo(48e3, 63)), Err(Unsupported));
    let three = AudioDeviceSetup::new(48e3, 64).with_inputs(ChannelMask::default().with(2));
    assert_eq!(dev.open(&three), Err(Unsupported));
    assert_eq!(dev.current_setup(), None);
    dev.open(&stereo(96e3, 128)).unwrap();
    assert_eq!(dev.current_setup(), Some(stereo(96e3, 128)));
    assert_eq!(Unsupported.code().as_str(), "SYRP-DEVICE-0002");
}

#[test]
fn b2_start_and_stop_run_the_callback_lifecycle() {
    let mut dev: Box<dyn AudioDevice> =
        Box::new(FakeAudioDevice::new(FakeAudioSpec::new("D", 0, 2)));
    let probe = Probe::default();
    assert_eq!(dev.start(Box::new(probe.clone())).err(), Some(NotOpen));
    dev.open(&stereo(48e3, 64)).unwrap();
    dev.start(Box::new(probe.clone())).unwrap();
    assert!(dev.is_running());
    assert_eq!(probe.counts(), [1, 0, 0]);
    let again = dev.start(Box::new(Probe::default())).err();
    assert_eq!(again, Some(DeviceError::AlreadyRunning));
    assert!(dev.stop().is_some(), "stop hands the callback back");
    assert_eq!((dev.is_running(), probe.counts()), (false, [1, 0, 1]));
    assert!(dev.stop().is_none());
    dev.start(Box::new(probe.clone())).unwrap();
    dev.close();
    assert_eq!((dev.current_setup(), probe.counts()), (None, [2, 0, 2]));
}

#[test]
fn b2_devices_wider_than_64_channels_open_and_run() {
    let mut dev = FakeAudioDevice::new(FakeAudioSpec::new("MADI", 0, 192));
    let driver = dev.driver();
    let all = AudioDeviceSetup::new(48e3, 32).with_outputs(ChannelMask::first(192));
    dev.open(&all).unwrap();
    let wide = AudioDeviceSetup::new(48e3, 32).with_outputs(ChannelMask::first(193));
    assert_eq!(
        FakeAudioDevice::new(FakeAudioSpec::new("M", 0, 192)).open(&wide),
        Err(Unsupported)
    );
    let cb = |_: In<'_>, mut out: Out<'_>, _: &mut DeviceRtCtx<'_>| out.channel_mut(150).fill(1.0);
    dev.start(Box::new(cb)).unwrap();
    let mut output = vec![0.0_f32; 192 * 32];
    driver.run_block(&[], &mut output).unwrap();
    assert!(output[150 * 32..151 * 32].iter().all(|&s| s == 1.0));
    assert_eq!(output.iter().filter(|&&s| s != 0.0).count(), 32);
    let sparse = ChannelMask::default().with(3).with(70).with(255);
    assert_eq!((sparse.count(), sparse.span()), (3, 256));
    assert_eq!(sparse.iter().collect::<Vec<_>>(), [3, 70, 255]);
}

#[test]
fn b2_about_to_start_may_drive_the_device() {
    let mut dev = FakeAudioDevice::new(FakeAudioSpec::new("P", 0, 2));
    dev.open(&stereo(48e3, 32)).unwrap();
    struct Prerun(syrp_device_api::fake::FakeAudioDriver);
    impl AudioCallback for Prerun {
        fn about_to_start(&mut self, _: &AudioDeviceSetup) {
            assert_eq!(self.0.run_block(&[], &mut [0.0; 64]), Err(NotOpen));
        }
        fn process(&mut self, _: In<'_>, _: Out<'_>, _: &mut DeviceRtCtx<'_>) {}
    }
    dev.start(Box::new(Prerun(dev.driver()))).unwrap();
    assert!(dev.is_running());
}

#[test]
fn b2_reports_latency_xruns_rates_and_sizes() {
    let dev = FakeAudioDevice::new(FakeAudioSpec::new("L", 1, 2).with_latency(10, 20));
    assert_eq!((dev.input_latency(), dev.output_latency()), (10, 20));
    assert_eq!(dev.available_sample_rates(), [44_100.0, 48_000.0, 96_000.0]);
    assert_eq!(dev.available_buffer_sizes(), [32, 64, 128, 256, 512]);
    assert_eq!(dev.input_channel_names(), ["Input 1"]);
    assert_eq!(dev.output_channel_names().len(), 2);
    assert_eq!(dev.xrun_count(), Some(0));
    let driver = dev.driver();
    driver.add_xrun();
    driver.add_xrun();
    assert_eq!(dev.xrun_count(), Some(2));
}

#[test]
fn b2_a_disconnected_device_fails_every_call() {
    let mut dev = FakeAudioDevice::new(FakeAudioSpec::new("U", 0, 2));
    let driver = dev.driver();
    dev.open(&stereo(48e3, 32)).unwrap();
    dev.start(Box::new(Probe::default())).unwrap();
    driver.disconnect();
    assert_eq!(driver.run_block(&[], &mut [0.0; 64]), Err(Disconnected));
    assert_eq!(dev.open(&stereo(48e3, 32)), Err(Disconnected));
}

#[test]
fn b3_callback_gets_masked_blocks_and_positions_without_allocating() {
    let mut dev = FakeAudioDevice::new(FakeAudioSpec::new("IO", 3, 2));
    let driver = dev.driver();
    let setup = stereo(48e3, 32).with_inputs(ChannelMask::from_low_bits(0b101));
    dev.open(&setup).unwrap();
    let last_pos = Arc::new(AtomicU64::new(u64::MAX));
    let seen = Arc::clone(&last_pos);
    let cb = move |inp: In<'_>, mut out: Out<'_>, rt: &mut DeviceRtCtx<'_>| {
        let shape = (inp.channels(), out.channels(), inp.frames(), out.frames());
        assert_eq!((shape, rt.sample_rate()), ((2, 2, 32, 32), 48e3));
        assert!(out.channel(0).iter().all(|&s| s == 0.0), "zeroed first");
        out.channel_mut(0).copy_from_slice(inp.channel(1));
        seen.store(rt.sample_position(), Relaxed);
    };
    dev.start(Box::new(cb)).unwrap();
    let mut input = [0.0_f32; 64];
    input[32..].fill(0.25);
    let mut output = [9.0_f32; 64];
    no_alloc(|| {
        driver.run_block(&input, &mut output).unwrap();
        driver.run_block(&input, &mut output).unwrap();
    });
    assert_eq!(last_pos.load(Relaxed), 32);
    assert!(output[..32].iter().all(|&s| s == 0.25));
    assert!(output[32..].iter().all(|&s| s == 0.0));
}

#[test]
fn b6_fake_ids_are_stable_across_instances() {
    let a = FakeAudioDevice::new(FakeAudioSpec::new("Studio", 2, 2));
    let b = FakeAudioDevice::new(FakeAudioSpec::new("Studio", 8, 8));
    assert_eq!(a.info().id(), b.info().id());
    assert_eq!(
        (a.info().id().as_str(), a.info().name()),
        ("fake:Studio", "Studio")
    );
}

#[test]
fn b6_driver_rejects_blocks_of_the_wrong_shape() {
    let mut dev = FakeAudioDevice::new(FakeAudioSpec::new("S", 0, 2));
    let driver = dev.driver();
    let run = |i: &[f32], o: &mut [f32]| driver.run_block(i, o);
    assert_eq!(run(&[], &mut [0.0; 64]), Err(NotOpen));
    dev.open(&stereo(48e3, 32)).unwrap();
    assert_eq!(run(&[], &mut [0.0; 64]), Err(NotOpen));
    dev.start(Box::new(Probe::default())).unwrap();
    assert_eq!(run(&[], &mut [0.0; 63]), Err(Unsupported));
    assert_eq!(run(&[0.0; 32], &mut [0.0; 64]), Err(Unsupported));
    assert_eq!(run(&[], &mut [0.0; 64]), Ok(()));
}

#[test]
fn b7_hot_types_are_rt_safe_and_devices_are_send() {
    fn rt<T: RtSafe>() {}
    rt::<ChannelMask>();
    rt::<syrp_device_api::ChannelIter>();
    rt::<AudioDeviceSetup>();
    rt::<DeviceRtCtx<'static>>();
    fn send<T: Send>() {}
    send::<Box<dyn AudioDevice>>();
    send::<Box<dyn AudioCallback>>();
    send::<Box<dyn syrp_device_api::MidiInput>>();
    send::<Box<dyn syrp_device_api::MidiOutput>>();
    send::<syrp_device_api::DeviceChangeNotifier>();
}

/// B7 freezes the public API (the snapshot is committed by the PR manager after review);
/// ADR 0003 D3 requires frozen contract types to have no public fields.
#[test]
fn b7_contract_types_have_no_public_fields() {
    let src = [
        include_str!("../src/lib.rs"),
        include_str!("../src/audio.rs"),
        include_str!("../src/fake.rs"),
        include_str!("../src/midi.rs"),
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

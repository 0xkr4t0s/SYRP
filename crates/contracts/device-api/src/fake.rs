// SPDX-License-Identifier: MIT OR Apache-2.0
//! In-memory audio and MIDI devices for tests: no hardware, no threads, fully deterministic.
//!
//! Tests drive the "audio thread" by calling [`FakeAudioDriver::run_block`] and the MIDI
//! thread with [`FakeMidiDeviceType::deliver`]. Ids are derived from names, so two fake
//! backends with the same names give the same ids, like a real device across runs.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use syrp_audio_api::{AudioBlock, AudioBlockMut};
use syrp_midi::MidiMessage;

use crate::{
    AudioCallback, AudioDevice, AudioDeviceSetup, AudioDeviceType, ChannelMask, DeviceChange,
    DeviceChangeNotifier, DeviceError, DeviceId, DeviceInfo, DeviceRtCtx, MidiDeviceType,
    MidiInput, MidiInputCallback, MidiOutput,
};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What a [`FakeAudioDevice`] supports: channels, rates, buffer sizes and latency.
/// ```
/// let spec = syrp_device_api::fake::FakeAudioSpec::new("Duo", 2, 2);
/// assert_eq!(spec.id().as_str(), "fake:Duo");
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct FakeAudioSpec {
    name: String,
    inputs: u32,
    outputs: u32,
    rates: Vec<f64>,
    sizes: Vec<u32>,
    latency: (u32, u32),
}

impl FakeAudioSpec {
    /// A device with these channel counts, rates 44.1, 48 and 96 kHz, buffer sizes 32 to 512
    /// (powers of two) and no latency.
    /// ```
    /// let _spec = syrp_device_api::fake::FakeAudioSpec::new("Mic", 1, 0);
    /// ```
    pub fn new(name: impl Into<String>, inputs: u32, outputs: u32) -> Self {
        FakeAudioSpec {
            name: name.into(),
            inputs: inputs.min(ChannelMask::MAX_CHANNELS),
            outputs: outputs.min(ChannelMask::MAX_CHANNELS),
            rates: vec![44_100.0, 48_000.0, 96_000.0],
            sizes: vec![32, 64, 128, 256, 512],
            latency: (0, 0),
        }
    }

    /// Sets the reported input and output latency in frames.
    /// ```
    /// let _spec = syrp_device_api::fake::FakeAudioSpec::new("A", 1, 1).with_latency(64, 64);
    /// ```
    #[must_use]
    pub fn with_latency(mut self, input: u32, output: u32) -> Self {
        self.latency = (input, output);
        self
    }

    /// The device id: `fake:` followed by the name.
    /// ```
    /// assert_eq!(syrp_device_api::fake::FakeAudioSpec::new("A", 1, 1).id().as_str(), "fake:A");
    /// ```
    #[must_use]
    pub fn id(&self) -> DeviceId {
        DeviceId::new(format!("fake:{}", self.name))
    }
}

#[derive(Default)]
struct AudioState {
    setup: Option<AudioDeviceSetup>,
    callback: Option<Box<dyn AudioCallback>>,
    position: u64,
    xruns: u64,
    gone: bool,
}

/// An in-memory [`AudioDevice`]. Get its [`FakeAudioDriver`] before boxing it.
/// ```
/// use syrp_device_api::{AudioDevice, AudioDeviceSetup, ChannelMask, fake::{FakeAudioDevice, FakeAudioSpec}};
/// let mut dev = FakeAudioDevice::new(FakeAudioSpec::new("A", 0, 1));
/// let driver = dev.driver();
/// dev.open(&AudioDeviceSetup::new(48e3, 32).with_outputs(ChannelMask::first(1)))?;
/// dev.start(Box::new(|_: syrp_audio_api::AudioBlock<'_, f32>, mut o: syrp_audio_api::AudioBlockMut<'_, f32>, _: &mut syrp_device_api::DeviceRtCtx<'_>| o.fill(0.5)))?;
/// let mut out = [0.0_f32; 32];
/// driver.run_block(&[], &mut out)?;
/// assert_eq!(out, [0.5; 32]);
/// # Ok::<(), syrp_device_api::DeviceError>(())
/// ```
pub struct FakeAudioDevice {
    info: DeviceInfo,
    spec: FakeAudioSpec,
    state: Arc<Mutex<AudioState>>,
}

impl FakeAudioDevice {
    /// A closed device built from `spec`.
    /// ```
    /// let _d = syrp_device_api::fake::FakeAudioDevice::new(syrp_device_api::fake::FakeAudioSpec::new("A", 1, 1));
    /// ```
    #[must_use]
    pub fn new(spec: FakeAudioSpec) -> Self {
        let info = DeviceInfo::new(spec.id(), spec.name.clone());
        FakeAudioDevice {
            info,
            spec,
            state: Arc::default(),
        }
    }

    /// A handle that plays the audio thread and the hardware for this device.
    /// ```
    /// # use syrp_device_api::fake::{FakeAudioDevice, FakeAudioSpec};
    /// let _driver = FakeAudioDevice::new(FakeAudioSpec::new("A", 1, 1)).driver();
    /// ```
    #[must_use]
    pub fn driver(&self) -> FakeAudioDriver {
        FakeAudioDriver {
            state: Arc::clone(&self.state),
        }
    }
}

impl AudioDevice for FakeAudioDevice {
    fn info(&self) -> &DeviceInfo {
        &self.info
    }

    fn input_channel_names(&self) -> Vec<String> {
        (1..=self.spec.inputs)
            .map(|c| format!("Input {c}"))
            .collect()
    }

    fn output_channel_names(&self) -> Vec<String> {
        (1..=self.spec.outputs)
            .map(|c| format!("Output {c}"))
            .collect()
    }

    fn available_sample_rates(&self) -> Vec<f64> {
        self.spec.rates.clone()
    }

    fn available_buffer_sizes(&self) -> Vec<u32> {
        self.spec.sizes.clone()
    }

    fn open(&mut self, setup: &AudioDeviceSetup) -> Result<(), DeviceError> {
        self.close();
        let mut st = lock(&self.state);
        if st.gone {
            return Err(DeviceError::Disconnected);
        }
        let ok = self.spec.rates.contains(&setup.sample_rate())
            && self.spec.sizes.contains(&setup.buffer_size())
            && setup.inputs().span() <= self.spec.inputs
            && setup.outputs().span() <= self.spec.outputs;
        if !ok {
            return Err(DeviceError::UnsupportedSetup);
        }
        *st = AudioState {
            setup: Some(*setup),
            ..AudioState::default()
        };
        Ok(())
    }

    fn close(&mut self) {
        drop(self.stop());
        lock(&self.state).setup = None;
    }

    fn current_setup(&self) -> Option<AudioDeviceSetup> {
        lock(&self.state).setup
    }

    fn start(&mut self, mut callback: Box<dyn AudioCallback>) -> Result<(), DeviceError> {
        let check = |st: &AudioState| match st.setup {
            _ if st.gone => Err(DeviceError::Disconnected),
            None => Err(DeviceError::NotOpen),
            _ if st.callback.is_some() => Err(DeviceError::AlreadyRunning),
            Some(setup) => Ok(setup),
        };
        let setup = check(&lock(&self.state))?;
        // Outside the lock, so the callback may use the driver while it prepares.
        callback.about_to_start(&setup);
        let mut st = lock(&self.state);
        let now =
            check(&st).and_then(|now| (now == setup).then_some(()).ok_or(DeviceError::NotOpen));
        if let Err(e) = now {
            drop(st);
            callback.stopped();
            return Err(e);
        }
        st.position = 0;
        st.callback = Some(callback);
        Ok(())
    }

    fn stop(&mut self) -> Option<Box<dyn AudioCallback>> {
        let mut callback = lock(&self.state).callback.take()?;
        callback.stopped();
        Some(callback)
    }

    fn is_running(&self) -> bool {
        lock(&self.state).callback.is_some()
    }

    fn input_latency(&self) -> u32 {
        self.spec.latency.0
    }

    fn output_latency(&self) -> u32 {
        self.spec.latency.1
    }

    fn xrun_count(&self) -> Option<u64> {
        Some(lock(&self.state).xruns)
    }
}

impl Drop for FakeAudioDevice {
    fn drop(&mut self) {
        self.close();
    }
}

/// Plays the audio thread and the hardware for one [`FakeAudioDevice`]. Cloneable.
/// ```
/// use syrp_device_api::{AudioDevice, fake::{FakeAudioDevice, FakeAudioSpec}};
/// let dev = FakeAudioDevice::new(FakeAudioSpec::new("A", 1, 1));
/// dev.driver().add_xrun();
/// assert_eq!(dev.xrun_count(), Some(1));
/// ```
#[derive(Clone)]
pub struct FakeAudioDriver {
    state: Arc<Mutex<AudioState>>,
}

impl FakeAudioDriver {
    /// Runs one block of `buffer_size` frames through the running callback. `input` holds the
    /// active input channels and `output` the active outputs, planar, each channel
    /// `buffer_size` samples long. `output` is zeroed first. Allocation-free.
    ///
    /// # Errors
    /// [`DeviceError::NotOpen`] if no callback is running, [`DeviceError::Disconnected`]
    /// after [`disconnect`](Self::disconnect), [`DeviceError::UnsupportedSetup`] if a slice
    /// has the wrong length.
    /// ```
    /// # use syrp_device_api::{DeviceError, fake::{FakeAudioDevice, FakeAudioSpec}};
    /// let dev = FakeAudioDevice::new(FakeAudioSpec::new("A", 1, 1));
    /// assert_eq!(dev.driver().run_block(&[], &mut []), Err(DeviceError::NotOpen));
    /// ```
    pub fn run_block(&self, input: &[f32], output: &mut [f32]) -> Result<(), DeviceError> {
        let mut guard = lock(&self.state);
        let st = &mut *guard;
        if st.gone {
            return Err(DeviceError::Disconnected);
        }
        let (Some(setup), Some(callback)) = (st.setup, st.callback.as_mut()) else {
            return Err(DeviceError::NotOpen);
        };
        let frames = setup.buffer_size() as usize;
        let (ins, outs) = (
            setup.inputs().count() as usize,
            setup.outputs().count() as usize,
        );
        if input.len() != ins * frames || output.len() != outs * frames {
            return Err(DeviceError::UnsupportedSetup);
        }
        output.fill(0.0);
        let bad = |_| DeviceError::UnsupportedSetup;
        let input = AudioBlock::new(input, ins, frames, frames).map_err(bad)?;
        let output = AudioBlockMut::new(output, outs, frames, frames).map_err(bad)?;
        let mut rt = DeviceRtCtx::new(st.position, setup.sample_rate());
        callback.process(input, output, &mut rt);
        st.position += frames as u64;
        Ok(())
    }

    /// Records one buffer overrun or underrun.
    /// ```
    /// # use syrp_device_api::fake::{FakeAudioDevice, FakeAudioSpec};
    /// FakeAudioDevice::new(FakeAudioSpec::new("A", 1, 1)).driver().add_xrun();
    /// ```
    pub fn add_xrun(&self) {
        lock(&self.state).xruns += 1;
    }

    /// Simulates unplugging: every later call fails with [`DeviceError::Disconnected`].
    /// ```
    /// # use syrp_device_api::{AudioDevice, AudioDeviceSetup, DeviceError, fake::{FakeAudioDevice, FakeAudioSpec}};
    /// let mut dev = FakeAudioDevice::new(FakeAudioSpec::new("A", 1, 1));
    /// dev.driver().disconnect();
    /// assert_eq!(dev.open(&AudioDeviceSetup::new(48e3, 64)), Err(DeviceError::Disconnected));
    /// ```
    pub fn disconnect(&self) {
        lock(&self.state).gone = true;
    }
}

#[derive(Default)]
struct AudioRegistry {
    specs: Vec<FakeAudioSpec>,
    created: Vec<(DeviceId, FakeAudioDriver)>,
    notifier: Option<DeviceChangeNotifier>,
}

/// An in-memory [`AudioDeviceType`] whose devices tests plug in and unplug.
/// ```
/// use syrp_device_api::{AudioDeviceType, fake::{FakeAudioDeviceType, FakeAudioSpec}};
/// let ty = FakeAudioDeviceType::new();
/// ty.add_device(FakeAudioSpec::new("A", 1, 1));
/// assert_eq!(ty.devices().len(), 1);
/// ```
#[derive(Default)]
pub struct FakeAudioDeviceType {
    reg: Mutex<AudioRegistry>,
}

impl FakeAudioDeviceType {
    /// A backend with no devices.
    /// ```
    /// let _ty = syrp_device_api::fake::FakeAudioDeviceType::new();
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Plugs in a device (replacing one with the same id) and notifies [`DeviceChange::Added`].
    /// ```
    /// # use syrp_device_api::fake::{FakeAudioDeviceType, FakeAudioSpec};
    /// FakeAudioDeviceType::new().add_device(FakeAudioSpec::new("A", 1, 1));
    /// ```
    pub fn add_device(&self, spec: FakeAudioSpec) {
        let mut reg = lock(&self.reg);
        let id = spec.id();
        reg.specs.retain(|s| s.id() != id);
        reg.specs.push(spec);
        if let Some(n) = &reg.notifier {
            n.notify(DeviceChange::Added(id));
        }
    }

    /// Unplugs a device: devices created from it fail with [`DeviceError::Disconnected`] and
    /// [`DeviceChange::Removed`] is notified. Returns whether it was present.
    /// ```
    /// # use syrp_device_api::{DeviceId, fake::FakeAudioDeviceType};
    /// assert!(!FakeAudioDeviceType::new().remove_device(&DeviceId::new("fake:A")));
    /// ```
    pub fn remove_device(&self, id: &DeviceId) -> bool {
        let mut reg = lock(&self.reg);
        let before = reg.specs.len();
        reg.specs.retain(|s| s.id() != *id);
        reg.created
            .iter()
            .filter(|(c, _)| c == id)
            .for_each(|(_, d)| d.disconnect());
        reg.created.retain(|(c, _)| c != id);
        let removed = reg.specs.len() != before;
        if let (true, Some(n)) = (removed, &reg.notifier) {
            n.notify(DeviceChange::Removed(id.clone()));
        }
        removed
    }

    /// Like [`AudioDeviceType::create`], but returns the concrete fake so tests can take its
    /// driver.
    ///
    /// # Errors
    /// [`DeviceError::NotFound`] if no device has `id`.
    /// ```
    /// # use syrp_device_api::fake::{FakeAudioDeviceType, FakeAudioSpec};
    /// let ty = FakeAudioDeviceType::new();
    /// ty.add_device(FakeAudioSpec::new("A", 1, 1));
    /// assert!(ty.create_fake(&FakeAudioSpec::new("A", 1, 1).id()).is_ok());
    /// ```
    pub fn create_fake(&self, id: &DeviceId) -> Result<FakeAudioDevice, DeviceError> {
        let mut reg = lock(&self.reg);
        let spec = reg
            .specs
            .iter()
            .find(|s| s.id() == *id)
            .ok_or(DeviceError::NotFound)?;
        let dev = FakeAudioDevice::new(spec.clone());
        reg.created.push((id.clone(), dev.driver()));
        Ok(dev)
    }
}

impl AudioDeviceType for FakeAudioDeviceType {
    fn name(&self) -> &str {
        "Fake"
    }

    fn devices(&self) -> Vec<DeviceInfo> {
        let reg = lock(&self.reg);
        reg.specs
            .iter()
            .map(|s| DeviceInfo::new(s.id(), s.name.clone()))
            .collect()
    }

    fn default_device(&self) -> Option<DeviceId> {
        lock(&self.reg).specs.first().map(FakeAudioSpec::id)
    }

    fn create(&self, id: &DeviceId) -> Result<Box<dyn AudioDevice>, DeviceError> {
        Ok(Box::new(self.create_fake(id)?))
    }

    fn set_change_notifier(&self, notifier: Option<DeviceChangeNotifier>) {
        lock(&self.reg).notifier = notifier;
    }
}

struct InputState {
    callback: Box<dyn MidiInputCallback>,
    running: bool,
    gone: bool,
}

#[derive(Default)]
struct MidiRegistry {
    inputs: Vec<DeviceInfo>,
    outputs: Vec<DeviceInfo>,
    open: Vec<(DeviceId, Arc<Mutex<InputState>>)>,
    sent: Vec<(DeviceId, MidiMessage)>,
    notifier: Option<DeviceChangeNotifier>,
}

/// An in-memory [`MidiDeviceType`]. Input ids are `fake-in:<name>`, outputs `fake-out:<name>`.
/// ```
/// use syrp_device_api::{MidiDeviceType, fake::FakeMidiDeviceType};
/// assert_eq!(FakeMidiDeviceType::new(&["K"], &[]).inputs()[0].id().as_str(), "fake-in:K");
/// ```
#[derive(Default)]
pub struct FakeMidiDeviceType {
    reg: Arc<Mutex<MidiRegistry>>,
}

impl FakeMidiDeviceType {
    /// A backend with these input and output ports.
    /// ```
    /// let _ty = syrp_device_api::fake::FakeMidiDeviceType::new(&["In"], &["Out"]);
    /// ```
    #[must_use]
    pub fn new(inputs: &[&str], outputs: &[&str]) -> Self {
        let port = |kind: &str, n: &&str| DeviceInfo::new(DeviceId::new(format!("{kind}:{n}")), *n);
        let reg = MidiRegistry {
            inputs: inputs.iter().map(|n| port("fake-in", n)).collect(),
            outputs: outputs.iter().map(|n| port("fake-out", n)).collect(),
            ..MidiRegistry::default()
        };
        FakeMidiDeviceType {
            reg: Arc::new(Mutex::new(reg)),
        }
    }

    /// Plugs in an input port and notifies [`DeviceChange::Added`]; returns its id.
    /// ```
    /// # use syrp_device_api::fake::FakeMidiDeviceType;
    /// assert_eq!(FakeMidiDeviceType::new(&[], &[]).add_input("K").as_str(), "fake-in:K");
    /// ```
    pub fn add_input(&self, name: &str) -> DeviceId {
        let mut reg = lock(&self.reg);
        let id = DeviceId::new(format!("fake-in:{name}"));
        reg.inputs.retain(|p| *p.id() != id);
        reg.inputs.push(DeviceInfo::new(id.clone(), name));
        if let Some(n) = &reg.notifier {
            n.notify(DeviceChange::Added(id.clone()));
        }
        id
    }

    /// Unplugs a port: open handles fail with [`DeviceError::Disconnected`] and
    /// [`DeviceChange::Removed`] is notified. Returns whether it was present.
    /// ```
    /// # use syrp_device_api::fake::FakeMidiDeviceType;
    /// let ty = FakeMidiDeviceType::new(&["K"], &[]);
    /// assert!(ty.remove_port(&ty.add_input("K")));
    /// ```
    pub fn remove_port(&self, id: &DeviceId) -> bool {
        let mut reg = lock(&self.reg);
        let before = reg.inputs.len() + reg.outputs.len();
        reg.inputs.retain(|p| p.id() != id);
        reg.outputs.retain(|p| p.id() != id);
        reg.open
            .iter()
            .filter(|(c, _)| c == id)
            .for_each(|(_, s)| lock(s).gone = true);
        reg.open.retain(|(c, _)| c != id);
        let removed = reg.inputs.len() + reg.outputs.len() != before;
        if let (true, Some(n)) = (removed, &reg.notifier) {
            n.notify(DeviceChange::Removed(id.clone()));
        }
        removed
    }

    /// Plays the MIDI thread: hands `message` (with its timestamp) to every started input
    /// open on `id`. Returns how many callbacks received it.
    /// ```
    /// # use syrp_device_api::fake::FakeMidiDeviceType;
    /// let ty = FakeMidiDeviceType::new(&["K"], &[]);
    /// let m = syrp_midi::MidiMessage::note_on(1, 60, 1).unwrap();
    /// assert_eq!(ty.deliver(&syrp_device_api::DeviceId::new("fake-in:K"), &m), 0);
    /// ```
    pub fn deliver(&self, id: &DeviceId, message: &MidiMessage) -> usize {
        let open: Vec<_> = lock(&self.reg)
            .open
            .iter()
            .filter(|(c, _)| c == id)
            .map(|(_, s)| Arc::clone(s))
            .collect();
        let mut delivered = 0;
        for state in open {
            let mut st = lock(&state);
            if st.running && !st.gone {
                st.callback.message(message);
                delivered += 1;
            }
        }
        delivered
    }

    /// The messages sent so far to output `id`, oldest first.
    /// ```
    /// # use syrp_device_api::{DeviceId, fake::FakeMidiDeviceType};
    /// assert!(FakeMidiDeviceType::new(&[], &["S"]).sent(&DeviceId::new("fake-out:S")).is_empty());
    /// ```
    #[must_use]
    pub fn sent(&self, id: &DeviceId) -> Vec<MidiMessage> {
        let reg = lock(&self.reg);
        reg.sent
            .iter()
            .filter(|(c, _)| c == id)
            .map(|(_, m)| m.clone())
            .collect()
    }
}

impl MidiDeviceType for FakeMidiDeviceType {
    fn name(&self) -> &str {
        "Fake"
    }

    fn inputs(&self) -> Vec<DeviceInfo> {
        lock(&self.reg).inputs.clone()
    }

    fn outputs(&self) -> Vec<DeviceInfo> {
        lock(&self.reg).outputs.clone()
    }

    fn open_input(
        &self,
        id: &DeviceId,
        callback: Box<dyn MidiInputCallback>,
    ) -> Result<Box<dyn MidiInput>, DeviceError> {
        let mut reg = lock(&self.reg);
        let info = reg
            .inputs
            .iter()
            .find(|p| p.id() == id)
            .ok_or(DeviceError::NotFound)?
            .clone();
        let state = Arc::new(Mutex::new(InputState {
            callback,
            running: false,
            gone: false,
        }));
        reg.open.push((id.clone(), Arc::clone(&state)));
        Ok(Box::new(FakeMidiInput { info, state }))
    }

    fn open_output(&self, id: &DeviceId) -> Result<Box<dyn MidiOutput>, DeviceError> {
        let reg = lock(&self.reg);
        let info = reg
            .outputs
            .iter()
            .find(|p| p.id() == id)
            .ok_or(DeviceError::NotFound)?
            .clone();
        Ok(Box::new(FakeMidiOutput {
            info,
            reg: Arc::clone(&self.reg),
        }))
    }

    fn set_change_notifier(&self, notifier: Option<DeviceChangeNotifier>) {
        lock(&self.reg).notifier = notifier;
    }
}

struct FakeMidiInput {
    info: DeviceInfo,
    state: Arc<Mutex<InputState>>,
}

impl MidiInput for FakeMidiInput {
    fn info(&self) -> &DeviceInfo {
        &self.info
    }

    fn start(&mut self) -> Result<(), DeviceError> {
        let mut st = lock(&self.state);
        if st.gone {
            return Err(DeviceError::Disconnected);
        }
        st.running = true;
        Ok(())
    }

    fn stop(&mut self) {
        lock(&self.state).running = false;
    }
}

impl Drop for FakeMidiInput {
    fn drop(&mut self) {
        lock(&self.state).gone = true;
    }
}

struct FakeMidiOutput {
    info: DeviceInfo,
    reg: Arc<Mutex<MidiRegistry>>,
}

impl MidiOutput for FakeMidiOutput {
    fn info(&self) -> &DeviceInfo {
        &self.info
    }

    fn send(&mut self, message: &MidiMessage) -> Result<(), DeviceError> {
        let mut reg = lock(&self.reg);
        if !reg.outputs.iter().any(|p| p == &self.info) {
            return Err(DeviceError::Disconnected);
        }
        reg.sent.push((self.info.id().clone(), message.clone()));
        Ok(())
    }
}

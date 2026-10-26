// SPDX-License-Identifier: MIT OR Apache-2.0
//! In-memory audio and MIDI devices for tests: no hardware, no threads, fully deterministic.
//!
//! Tests drive the "audio thread" by calling [`FakeAudioDriver::run_block`]. Ids are derived
//! from names, so two fakes with the same name share an id, like a real device across runs.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use syrp_audio_api::{AudioBlock, AudioBlockMut};

use crate::{
    AudioCallback, AudioDevice, AudioDeviceSetup, ChannelMask, DeviceError, DeviceId, DeviceInfo,
    DeviceRtCtx,
};

fn fits(mask: ChannelMask, count: u32) -> bool {
    mask.bits().checked_shr(count).unwrap_or(0) == 0
}

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
            inputs: inputs.min(64),
            outputs: outputs.min(64),
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
            && fits(setup.inputs(), self.spec.inputs)
            && fits(setup.outputs(), self.spec.outputs);
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
        let mut st = lock(&self.state);
        let setup = match (&st, st.setup) {
            (s, _) if s.gone => return Err(DeviceError::Disconnected),
            (_, None) => return Err(DeviceError::NotOpen),
            (s, _) if s.callback.is_some() => return Err(DeviceError::AlreadyRunning),
            (_, Some(setup)) => setup,
        };
        callback.about_to_start(&setup);
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

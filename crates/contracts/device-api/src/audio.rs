// SPDX-License-Identifier: MIT OR Apache-2.0
//! Audio device traits and the plain data they exchange.

use std::marker::PhantomData;

use syrp_audio_api::{AudioBlock, AudioBlockMut};
use syrp_rt::RtSafe;

use crate::{DeviceError, DeviceInfo};

const WORDS: usize = 4;

/// Which of a device's channels (0 to [`MAX_CHANNELS`](Self::MAX_CHANNELS) - 1) are active.
///
/// Fixed-size storage keeps it `Copy` and real-time safe.
/// ```
/// use syrp_device_api::ChannelMask;
/// let m = ChannelMask::default().with(0).with(130);
/// assert_eq!((m.count(), m.contains(130), m.contains(1)), (2, true, false));
/// ```
#[doc(alias = "BigInteger")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ChannelMask {
    words: [u64; WORDS],
}

impl ChannelMask {
    /// The widest device a mask can describe, in channels.
    /// ```
    /// assert_eq!(syrp_device_api::ChannelMask::MAX_CHANNELS, 256);
    /// ```
    pub const MAX_CHANNELS: u32 = 64 * WORDS as u32;

    /// The first `n` channels (all of them when `n >= MAX_CHANNELS`).
    /// ```
    /// assert_eq!(syrp_device_api::ChannelMask::first(100).count(), 100);
    /// ```
    #[must_use]
    pub const fn first(n: u32) -> Self {
        let mut words = [0; WORDS];
        let mut i = 0;
        while i < WORDS {
            let lo = 64 * i as u32;
            words[i] = if n >= lo + 64 {
                u64::MAX
            } else if n > lo {
                (1 << (n - lo)) - 1
            } else {
                0
            };
            i += 1;
        }
        ChannelMask { words }
    }

    /// Channels 0 to 63 from `bits` (bit `c` is channel `c`); higher channels are inactive.
    /// ```
    /// assert_eq!(syrp_device_api::ChannelMask::from_low_bits(0b110).count(), 2);
    /// ```
    #[must_use]
    pub const fn from_low_bits(bits: u64) -> Self {
        let mut words = [0; WORDS];
        words[0] = bits;
        ChannelMask { words }
    }

    /// This mask with `channel` active; channels at or past `MAX_CHANNELS` are ignored.
    /// ```
    /// assert!(syrp_device_api::ChannelMask::default().with(200).contains(200));
    /// ```
    #[must_use]
    pub const fn with(mut self, channel: u32) -> Self {
        if channel < Self::MAX_CHANNELS {
            self.words[(channel / 64) as usize] |= 1 << (channel % 64);
        }
        self
    }

    /// This mask with `channel` inactive.
    /// ```
    /// assert!(!syrp_device_api::ChannelMask::first(2).without(1).contains(1));
    /// ```
    #[must_use]
    pub const fn without(mut self, channel: u32) -> Self {
        if channel < Self::MAX_CHANNELS {
            self.words[(channel / 64) as usize] &= !(1 << (channel % 64));
        }
        self
    }

    /// How many channels are active; this is the channel count of the callback's block.
    /// ```
    /// assert_eq!(syrp_device_api::ChannelMask::first(2).count(), 2);
    /// ```
    #[must_use]
    pub const fn count(self) -> u32 {
        let mut n = 0;
        let mut i = 0;
        while i < WORDS {
            n += self.words[i].count_ones();
            i += 1;
        }
        n
    }

    /// Whether channel `channel` is active.
    /// ```
    /// assert!(!syrp_device_api::ChannelMask::first(2).contains(256));
    /// ```
    #[must_use]
    pub const fn contains(self, channel: u32) -> bool {
        channel < Self::MAX_CHANNELS
            && self.words[(channel / 64) as usize] & (1 << (channel % 64)) != 0
    }

    /// The highest active channel plus one (0 when empty): a device needs at least this many.
    /// ```
    /// assert_eq!(syrp_device_api::ChannelMask::default().with(70).span(), 71);
    /// ```
    #[must_use]
    pub const fn span(self) -> u32 {
        let mut i = WORDS;
        while i > 0 {
            i -= 1;
            if self.words[i] != 0 {
                return 64 * i as u32 + 64 - self.words[i].leading_zeros();
            }
        }
        0
    }

    /// The active channels in ascending order; block channel `k` is the `k`th of these.
    /// ```
    /// let m = syrp_device_api::ChannelMask::from_low_bits(0b101).with(64);
    /// assert_eq!(m.iter().collect::<Vec<_>>(), [0, 2, 64]);
    /// ```
    #[must_use]
    pub const fn iter(self) -> ChannelIter {
        ChannelIter {
            mask: self,
            next: 0,
        }
    }
}

impl IntoIterator for ChannelMask {
    type Item = u32;
    type IntoIter = ChannelIter;
    fn into_iter(self) -> ChannelIter {
        self.iter()
    }
}

/// Iterates a [`ChannelMask`]'s active channels in ascending order. Allocation-free.
/// ```
/// let mut it = syrp_device_api::ChannelMask::first(2).iter();
/// assert_eq!((it.next(), it.next(), it.next()), (Some(0), Some(1), None));
/// ```
#[derive(Debug, Clone, Copy)]
pub struct ChannelIter {
    mask: ChannelMask,
    next: u32,
}

impl Iterator for ChannelIter {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        while self.next < ChannelMask::MAX_CHANNELS {
            let word = self.mask.words[(self.next / 64) as usize] >> (self.next % 64);
            if word == 0 {
                self.next = (self.next / 64 + 1) * 64;
                continue;
            }
            let channel = self.next + word.trailing_zeros();
            self.next = channel + 1;
            return Some(channel);
        }
        None
    }
}

/// How to open an [`AudioDevice`]: sample rate, buffer size and active channels.
/// ```
/// use syrp_device_api::{AudioDeviceSetup, ChannelMask};
/// let s = AudioDeviceSetup::new(44_100.0, 256).with_inputs(ChannelMask::first(1));
/// assert_eq!((s.sample_rate(), s.buffer_size(), s.inputs().count(), s.outputs().count()), (44_100.0, 256, 1, 0));
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioDeviceSetup {
    sample_rate: f64,
    buffer_size: u32,
    inputs: ChannelMask,
    outputs: ChannelMask,
}

impl AudioDeviceSetup {
    /// A setup at `sample_rate` Hz with `buffer_size` frames per callback and no channels.
    /// ```
    /// let _s = syrp_device_api::AudioDeviceSetup::new(48_000.0, 128);
    /// ```
    #[must_use]
    pub const fn new(sample_rate: f64, buffer_size: u32) -> Self {
        let none = ChannelMask { words: [0; WORDS] };
        AudioDeviceSetup {
            sample_rate,
            buffer_size,
            inputs: none,
            outputs: none,
        }
    }

    /// Returns the setup with these input channels active.
    /// ```
    /// # use syrp_device_api::{AudioDeviceSetup, ChannelMask};
    /// let s = AudioDeviceSetup::new(48e3, 64).with_inputs(ChannelMask::first(2));
    /// assert_eq!(s.inputs().count(), 2);
    /// ```
    #[must_use]
    pub const fn with_inputs(mut self, inputs: ChannelMask) -> Self {
        self.inputs = inputs;
        self
    }

    /// Returns the setup with these output channels active.
    /// ```
    /// # use syrp_device_api::{AudioDeviceSetup, ChannelMask};
    /// let s = AudioDeviceSetup::new(48e3, 64).with_outputs(ChannelMask::first(2));
    /// assert_eq!(s.outputs().count(), 2);
    /// ```
    #[must_use]
    pub const fn with_outputs(mut self, outputs: ChannelMask) -> Self {
        self.outputs = outputs;
        self
    }

    /// The sample rate in Hz.
    /// ```
    /// assert_eq!(syrp_device_api::AudioDeviceSetup::new(96e3, 64).sample_rate(), 96e3);
    /// ```
    #[must_use]
    pub const fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    /// The buffer size in frames: the most frames one callback receives.
    /// ```
    /// assert_eq!(syrp_device_api::AudioDeviceSetup::new(96e3, 64).buffer_size(), 64);
    /// ```
    #[doc(alias = "bufferSize")]
    #[must_use]
    pub const fn buffer_size(&self) -> u32 {
        self.buffer_size
    }

    /// The active input channels.
    /// ```
    /// assert_eq!(syrp_device_api::AudioDeviceSetup::new(48e3, 64).inputs().count(), 0);
    /// ```
    #[doc(alias = "inputChannels")]
    #[must_use]
    pub const fn inputs(&self) -> ChannelMask {
        self.inputs
    }

    /// The active output channels.
    /// ```
    /// assert_eq!(syrp_device_api::AudioDeviceSetup::new(48e3, 64).outputs().count(), 0);
    /// ```
    #[doc(alias = "outputChannels")]
    #[must_use]
    pub const fn outputs(&self) -> ChannelMask {
        self.outputs
    }
}

/// The real-time context a device passes to every [`AudioCallback::process`] call.
///
/// Built by backends around each callback; it borrows nothing yet, but its lifetime lets later
/// versions lend RT services without a contract change.
/// ```
/// let rt = syrp_device_api::DeviceRtCtx::new(512, 48_000.0);
/// assert_eq!((rt.sample_position(), rt.sample_rate()), (512, 48_000.0));
/// ```
#[doc(alias = "AudioIODeviceCallbackContext")]
#[derive(Debug)]
pub struct DeviceRtCtx<'a> {
    sample_position: u64,
    sample_rate: f64,
    _p: PhantomData<&'a mut ()>,
}

impl DeviceRtCtx<'_> {
    /// A context for a callback that starts `sample_position` frames after the device started.
    /// For backends.
    /// ```
    /// let _rt = syrp_device_api::DeviceRtCtx::new(0, 44_100.0);
    /// ```
    #[must_use]
    pub const fn new(sample_position: u64, sample_rate: f64) -> Self {
        DeviceRtCtx {
            sample_position,
            sample_rate,
            _p: PhantomData,
        }
    }

    /// Frames delivered since `start`, counting dropped blocks: the first callback sees 0.
    /// ```
    /// assert_eq!(syrp_device_api::DeviceRtCtx::new(64, 48e3).sample_position(), 64);
    /// ```
    #[must_use]
    pub const fn sample_position(&self) -> u64 {
        self.sample_position
    }

    /// The device's current sample rate in Hz.
    /// ```
    /// assert_eq!(syrp_device_api::DeviceRtCtx::new(0, 48e3).sample_rate(), 48e3);
    /// ```
    #[must_use]
    pub const fn sample_rate(&self) -> f64 {
        self.sample_rate
    }
}

// SAFETY: plain `Copy` bits with no destructor and no RT-unsafe methods.
unsafe impl RtSafe for ChannelMask {}
// SAFETY: a mask and a counter; `next` only reads and shifts integers.
unsafe impl RtSafe for ChannelIter {}
// SAFETY: plain `Copy` data with no destructor and no RT-unsafe methods.
unsafe impl RtSafe for AudioDeviceSetup {}
// SAFETY: two numbers and a marker; no destructor, every method is a field read.
unsafe impl RtSafe for DeviceRtCtx<'_> {}

/// The code a running [`AudioDevice`] calls on its audio thread.
///
/// Closures with the [`process`](Self::process) signature implement it.
/// ```
/// use syrp_device_api::{AudioCallback, DeviceRtCtx};
/// use syrp_audio_api::{AudioBlock as In, AudioBlockMut as Out};
/// let mut cb = |i: In<'_, f32>, mut o: Out<'_, f32>, _: &mut DeviceRtCtx<'_>| o.channel_mut(0).copy_from_slice(i.channel(0));
/// let mut out = [0.0_f32; 4];
/// cb.process(In::new(&[0.5; 4], 1, 4, 4).unwrap(), Out::new(&mut out, 1, 4, 4).unwrap(), &mut DeviceRtCtx::new(0, 48e3));
/// assert_eq!(out, [0.5; 4]);
/// ```
#[doc(alias = "AudioIODeviceCallback")]
pub trait AudioCallback: Send + 'static {
    /// Called off the audio thread before the first `process`; may allocate.
    /// ```
    /// # use syrp_device_api::{AudioCallback, AudioDeviceSetup};
    /// # let mut cb = |_: syrp_audio_api::AudioBlock<'_, f32>, _: syrp_audio_api::AudioBlockMut<'_, f32>, _: &mut syrp_device_api::DeviceRtCtx<'_>| {};
    /// cb.about_to_start(&AudioDeviceSetup::new(48e3, 64));
    /// ```
    #[doc(alias = "audioDeviceAboutToStart")]
    fn about_to_start(&mut self, setup: &AudioDeviceSetup) {
        let _ = setup;
    }

    /// Renders one block on the audio thread. Must not allocate, lock, block or do I/O.
    ///
    /// `input` has one channel per active input and `output` one per active output, in
    /// ascending channel order, with the same `frames()` (at most the buffer size). `output`
    /// is zeroed before the call.
    /// ```
    /// # use syrp_device_api::{AudioCallback, DeviceRtCtx};
    /// # use syrp_audio_api::{AudioBlock, AudioBlockMut};
    /// let mut cb = |_: AudioBlock<'_, f32>, mut o: AudioBlockMut<'_, f32>, _: &mut DeviceRtCtx<'_>| o.fill(1.0);
    /// let mut out = [0.0_f32; 2];
    /// cb.process(AudioBlock::new(&[], 0, 2, 2).unwrap(), AudioBlockMut::new(&mut out, 1, 2, 2).unwrap(), &mut DeviceRtCtx::new(0, 1.0));
    /// assert_eq!(out, [1.0; 2]);
    /// ```
    #[doc(alias = "audioDeviceIOCallbackWithContext")]
    fn process(
        &mut self,
        input: AudioBlock<'_, f32>,
        output: AudioBlockMut<'_, f32>,
        rt: &mut DeviceRtCtx<'_>,
    );

    /// Called off the audio thread after the last `process`.
    /// ```
    /// # use syrp_device_api::AudioCallback;
    /// # let mut cb = |_: syrp_audio_api::AudioBlock<'_, f32>, _: syrp_audio_api::AudioBlockMut<'_, f32>, _: &mut syrp_device_api::DeviceRtCtx<'_>| {};
    /// cb.stopped();
    /// ```
    #[doc(alias = "audioDeviceStopped")]
    fn stopped(&mut self) {}
}

impl<F> AudioCallback for F
where
    F: FnMut(AudioBlock<'_, f32>, AudioBlockMut<'_, f32>, &mut DeviceRtCtx<'_>) + Send + 'static,
{
    fn process(
        &mut self,
        input: AudioBlock<'_, f32>,
        output: AudioBlockMut<'_, f32>,
        rt: &mut DeviceRtCtx<'_>,
    ) {
        self(input, output, rt);
    }
}

/// One audio device: open it with a setup, start it with a callback, stop and close it.
///
/// Call these from any thread except the device's audio thread. `close` stops the device first;
/// dropping a device closes it.
/// ```
/// use syrp_device_api::{AudioDevice, AudioDeviceSetup, ChannelMask};
/// use syrp_device_api::fake::{FakeAudioDevice, FakeAudioSpec};
/// let mut dev = FakeAudioDevice::new(FakeAudioSpec::new("Out", 0, 2));
/// dev.open(&AudioDeviceSetup::new(48e3, 64).with_outputs(ChannelMask::first(2)))?;
/// dev.start(Box::new(|_: syrp_audio_api::AudioBlock<'_, f32>, _: syrp_audio_api::AudioBlockMut<'_, f32>, _: &mut syrp_device_api::DeviceRtCtx<'_>| {}))?;
/// assert!(dev.is_running());
/// assert!(dev.stop().is_some());
/// # Ok::<(), syrp_device_api::DeviceError>(())
/// ```
#[doc(alias = "AudioIODevice")]
pub trait AudioDevice: Send {
    /// The device's id and name.
    /// ```
    /// # use syrp_device_api::{AudioDevice, fake::{FakeAudioDevice, FakeAudioSpec}};
    /// assert_eq!(FakeAudioDevice::new(FakeAudioSpec::new("A", 1, 1)).info().name(), "A");
    /// ```
    fn info(&self) -> &DeviceInfo;

    /// One name per hardware input channel.
    /// ```
    /// # use syrp_device_api::{AudioDevice, fake::{FakeAudioDevice, FakeAudioSpec}};
    /// assert_eq!(FakeAudioDevice::new(FakeAudioSpec::new("A", 2, 0)).input_channel_names().len(), 2);
    /// ```
    #[doc(alias = "getInputChannelNames")]
    fn input_channel_names(&self) -> Vec<String>;

    /// One name per hardware output channel.
    /// ```
    /// # use syrp_device_api::{AudioDevice, fake::{FakeAudioDevice, FakeAudioSpec}};
    /// assert_eq!(FakeAudioDevice::new(FakeAudioSpec::new("A", 2, 0)).output_channel_names().len(), 0);
    /// ```
    #[doc(alias = "getOutputChannelNames")]
    fn output_channel_names(&self) -> Vec<String>;

    /// The sample rates `open` accepts, in Hz, ascending.
    /// ```
    /// # use syrp_device_api::{AudioDevice, fake::{FakeAudioDevice, FakeAudioSpec}};
    /// assert!(FakeAudioDevice::new(FakeAudioSpec::new("A", 1, 1)).available_sample_rates().contains(&48e3));
    /// ```
    #[doc(alias = "getAvailableSampleRates")]
    fn available_sample_rates(&self) -> Vec<f64>;

    /// The buffer sizes `open` accepts, in frames, ascending.
    /// ```
    /// # use syrp_device_api::{AudioDevice, fake::{FakeAudioDevice, FakeAudioSpec}};
    /// assert!(FakeAudioDevice::new(FakeAudioSpec::new("A", 1, 1)).available_buffer_sizes().contains(&64));
    /// ```
    #[doc(alias = "getAvailableBufferSizes")]
    fn available_buffer_sizes(&self) -> Vec<u32>;

    /// Opens (or reopens) the device with `setup`, stopping it first if it is running.
    ///
    /// # Errors
    /// [`DeviceError::UnsupportedSetup`] for a rate, size or channel the device lacks (the
    /// device is then closed); [`DeviceError::Disconnected`] if it has gone.
    /// ```
    /// # use syrp_device_api::{AudioDevice, AudioDeviceSetup, DeviceError, fake::{FakeAudioDevice, FakeAudioSpec}};
    /// let mut dev = FakeAudioDevice::new(FakeAudioSpec::new("A", 1, 1));
    /// assert_eq!(dev.open(&AudioDeviceSetup::new(1.0, 64)), Err(DeviceError::UnsupportedSetup));
    /// ```
    fn open(&mut self, setup: &AudioDeviceSetup) -> Result<(), DeviceError>;

    /// Stops and closes the device. Does nothing if it is closed.
    /// ```
    /// # use syrp_device_api::{AudioDevice, fake::{FakeAudioDevice, FakeAudioSpec}};
    /// let mut dev = FakeAudioDevice::new(FakeAudioSpec::new("A", 1, 1));
    /// dev.close();
    /// assert!(dev.current_setup().is_none());
    /// ```
    fn close(&mut self);

    /// The setup the device is open with, or `None` when closed.
    /// ```
    /// # use syrp_device_api::{AudioDevice, AudioDeviceSetup, fake::{FakeAudioDevice, FakeAudioSpec}};
    /// let mut dev = FakeAudioDevice::new(FakeAudioSpec::new("A", 1, 1));
    /// dev.open(&AudioDeviceSetup::new(48e3, 64)).unwrap();
    /// assert_eq!(dev.current_setup().unwrap().sample_rate(), 48e3);
    /// ```
    #[doc(alias = "getCurrentSampleRate")]
    fn current_setup(&self) -> Option<AudioDeviceSetup>;

    /// Calls `callback.about_to_start`, then runs `process` on the audio thread for every
    /// block until `stop`.
    ///
    /// # Errors
    /// [`DeviceError::NotOpen`], [`DeviceError::AlreadyRunning`] or
    /// [`DeviceError::Disconnected`]; the callback is dropped unstarted.
    /// ```
    /// # use syrp_device_api::{AudioDevice, DeviceError, fake::{FakeAudioDevice, FakeAudioSpec}};
    /// let mut dev = FakeAudioDevice::new(FakeAudioSpec::new("A", 1, 1));
    /// let cb = |_: syrp_audio_api::AudioBlock<'_, f32>, _: syrp_audio_api::AudioBlockMut<'_, f32>, _: &mut syrp_device_api::DeviceRtCtx<'_>| {};
    /// assert_eq!(dev.start(Box::new(cb)).err(), Some(DeviceError::NotOpen));
    /// ```
    fn start(&mut self, callback: Box<dyn AudioCallback>) -> Result<(), DeviceError>;

    /// Stops the callback, calls its `stopped`, and hands it back so it is dropped off the
    /// audio thread. `None` if nothing was running.
    /// ```
    /// # use syrp_device_api::{AudioDevice, fake::{FakeAudioDevice, FakeAudioSpec}};
    /// assert!(FakeAudioDevice::new(FakeAudioSpec::new("A", 1, 1)).stop().is_none());
    /// ```
    fn stop(&mut self) -> Option<Box<dyn AudioCallback>>;

    /// Whether a callback is running.
    /// ```
    /// # use syrp_device_api::{AudioDevice, fake::{FakeAudioDevice, FakeAudioSpec}};
    /// assert!(!FakeAudioDevice::new(FakeAudioSpec::new("A", 1, 1)).is_running());
    /// ```
    #[doc(alias = "isPlaying")]
    fn is_running(&self) -> bool;

    /// Input latency in frames, as the driver reports it.
    /// ```
    /// # use syrp_device_api::{AudioDevice, fake::{FakeAudioDevice, FakeAudioSpec}};
    /// let dev = FakeAudioDevice::new(FakeAudioSpec::new("A", 1, 1).with_latency(32, 48));
    /// assert_eq!(dev.input_latency(), 32);
    /// ```
    #[doc(alias = "getInputLatencyInSamples")]
    fn input_latency(&self) -> u32;

    /// Output latency in frames, as the driver reports it.
    /// ```
    /// # use syrp_device_api::{AudioDevice, fake::{FakeAudioDevice, FakeAudioSpec}};
    /// let dev = FakeAudioDevice::new(FakeAudioSpec::new("A", 1, 1).with_latency(32, 48));
    /// assert_eq!(dev.output_latency(), 48);
    /// ```
    #[doc(alias = "getOutputLatencyInSamples")]
    fn output_latency(&self) -> u32;

    /// Buffer overruns and underruns since the device was opened, or `None` if the backend
    /// cannot detect them.
    /// ```
    /// # use syrp_device_api::{AudioDevice, fake::{FakeAudioDevice, FakeAudioSpec}};
    /// assert_eq!(FakeAudioDevice::new(FakeAudioSpec::new("A", 1, 1)).xrun_count(), Some(0));
    /// ```
    #[doc(alias = "getXRunCount")]
    fn xrun_count(&self) -> Option<u64>;
}

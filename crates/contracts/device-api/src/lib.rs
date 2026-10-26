// SPDX-License-Identifier: MIT OR Apache-2.0
//! The Syrp device backend contract (master plan §3, `juce_audio_devices`).
//!
//! Every platform backend (CoreAudio, WASAPI, ASIO, ALSA, JACK, CoreMIDI...) lives in its own
//! crate and implements these traits; this crate holds no platform code.
//!
//! - [`AudioDevice`]: open with an [`AudioDeviceSetup`], run an [`AudioCallback`] with a
//!   [`DeviceRtCtx`], stop, and report latency, xruns and the rates and sizes it supports.
//! - [`DeviceId`]: names a device stably across runs.
//! - [`fake`]: in-memory devices for tests, with no hardware.
//!
//! Device types (enumeration), MIDI ports and device-change notifications arrive in the next
//! increment of WP-0.2-05.
//!
//! ```
//! use syrp_device_api::fake::{FakeAudioDevice, FakeAudioSpec};
//! use syrp_device_api::{AudioDevice, AudioDeviceSetup, ChannelMask};
//! let mut dev: Box<dyn AudioDevice> = Box::new(FakeAudioDevice::new(FakeAudioSpec::new("Duo", 2, 2)));
//! dev.open(&AudioDeviceSetup::new(48_000.0, 64).with_outputs(ChannelMask::first(2)))?;
//! assert_eq!(dev.current_setup().map(|s| s.buffer_size()), Some(64));
//! # Ok::<(), syrp_device_api::DeviceError>(())
//! ```

use std::fmt;

use syrp_error::{Diagnostic, ErrorCode};

mod audio;
pub mod fake;

pub use audio::{
    AudioCallback, AudioDevice, AudioDeviceSetup, ChannelIter, ChannelMask, DeviceRtCtx,
};

/// A device or port identifier that is stable across runs and reboots.
///
/// Backends derive it from a persistent hardware or driver identifier (a UID, an endpoint id,
/// a card name), never from an enumeration index, so saved settings find the same device again.
/// ```
/// let id = syrp_device_api::DeviceId::new("alsa:hw:USB");
/// assert_eq!(id.as_str(), "alsa:hw:USB");
/// ```
#[doc(alias = "identifier")]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceId(Box<str>);

impl DeviceId {
    /// Wraps a backend's persistent identifier.
    /// ```
    /// let _id = syrp_device_api::DeviceId::new("coreaudio:BuiltInSpeakerDevice");
    /// ```
    pub fn new(id: impl Into<String>) -> Self {
        DeviceId(id.into().into_boxed_str())
    }

    /// The identifier text.
    /// ```
    /// assert_eq!(syrp_device_api::DeviceId::new("x").as_str(), "x");
    /// ```
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Errors from device backends, catalogued in `errors/device.toml`.
/// ```
/// use syrp_error::Diagnostic;
/// assert_eq!(syrp_device_api::DeviceError::NotFound.code().as_str(), "SYRP-DEVICE-0001");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DeviceError {
    /// No device or port has that id (`SYRP-DEVICE-0001`).
    NotFound,
    /// The device cannot run the requested rate, buffer size or channels (`SYRP-DEVICE-0002`).
    UnsupportedSetup,
    /// The device must be opened before this call (`SYRP-DEVICE-0003`).
    NotOpen,
    /// The device is already running a callback; stop it first (`SYRP-DEVICE-0004`).
    AlreadyRunning,
    /// The device was unplugged or disconnected (`SYRP-DEVICE-0005`).
    Disconnected,
}

impl DeviceError {
    const fn parts(self) -> (&'static str, &'static str) {
        match self {
            DeviceError::NotFound => ("SYRP-DEVICE-0001", "no device has that id"),
            DeviceError::UnsupportedSetup => {
                ("SYRP-DEVICE-0002", "the device does not support that setup")
            }
            DeviceError::NotOpen => ("SYRP-DEVICE-0003", "the device is not open"),
            DeviceError::AlreadyRunning => ("SYRP-DEVICE-0004", "the device is already running"),
            DeviceError::Disconnected => ("SYRP-DEVICE-0005", "the device was disconnected"),
        }
    }
}

impl fmt::Display for DeviceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.parts().1)
    }
}

impl std::error::Error for DeviceError {}

impl Diagnostic for DeviceError {
    fn code(&self) -> ErrorCode {
        ErrorCode::new(self.parts().0)
    }
}

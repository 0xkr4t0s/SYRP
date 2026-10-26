// SPDX-License-Identifier: MIT OR Apache-2.0
//! The Syrp device backend contract (master plan §3, `juce_audio_devices`).
//!
//! Every platform backend (CoreAudio, WASAPI, ASIO, ALSA, JACK, CoreMIDI...) lives in its own
//! crate and implements these traits; this crate holds no platform code.
//!
//! - Audio: [`AudioDeviceType`] enumerates devices and creates [`AudioDevice`]s, which open
//!   with an [`AudioDeviceSetup`] and run an [`AudioCallback`] with a [`DeviceRtCtx`].
//! - MIDI: [`MidiDeviceType`] enumerates ports and opens [`MidiInput`]s (delivering
//!   timestamped [`syrp_midi::MidiMessage`]s to a [`MidiInputCallback`]) and [`MidiOutput`]s.
//! - Devices are named by a [`DeviceId`] that is stable across runs. Hot-plug changes reach
//!   the message loop through a [`DeviceChangeNotifier`].
//! - [`fake`]: in-memory devices for tests, with no hardware.
//!
//! ```
//! use syrp_device_api::fake::{FakeAudioDeviceType, FakeAudioSpec};
//! use syrp_device_api::{AudioDeviceSetup, AudioDeviceType, ChannelMask};
//! let ty = FakeAudioDeviceType::new();
//! ty.add_device(FakeAudioSpec::new("Interface", 2, 2));
//! let id = ty.devices()[0].id().clone();
//! let mut dev = ty.create(&id)?;
//! dev.open(&AudioDeviceSetup::new(48_000.0, 64).with_outputs(ChannelMask::first(2)))?;
//! assert_eq!(dev.current_setup().map(|s| s.buffer_size()), Some(64));
//! # Ok::<(), syrp_device_api::DeviceError>(())
//! ```

use std::fmt;
use std::sync::Arc;

use syrp_error::{Diagnostic, ErrorCode};
use syrp_loop_api::{MessageThread, Task, TaskSender};

mod audio;
pub mod fake;
mod midi;

pub use audio::{
    AudioCallback, AudioDevice, AudioDeviceSetup, AudioDeviceType, ChannelIter, ChannelMask,
    DeviceRtCtx,
};
pub use midi::{MidiDeviceType, MidiInput, MidiInputCallback, MidiOutput};

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

/// One device or port as enumerated by a backend: its stable id and a display name.
/// ```
/// use syrp_device_api::{DeviceId, DeviceInfo};
/// let info = DeviceInfo::new(DeviceId::new("fake:A"), "A");
/// assert_eq!((info.id().as_str(), info.name()), ("fake:A", "A"));
/// ```
#[doc(alias = "MidiDeviceInfo")]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DeviceInfo {
    id: DeviceId,
    name: String,
}

impl DeviceInfo {
    /// Describes a device.
    /// ```
    /// let _info = syrp_device_api::DeviceInfo::new(syrp_device_api::DeviceId::new("a"), "A");
    /// ```
    pub fn new(id: DeviceId, name: impl Into<String>) -> Self {
        DeviceInfo {
            id,
            name: name.into(),
        }
    }

    /// The stable identifier; pass it back to the backend to create or open the device.
    /// ```
    /// # use syrp_device_api::{DeviceId, DeviceInfo};
    /// assert_eq!(DeviceInfo::new(DeviceId::new("a"), "A").id(), &DeviceId::new("a"));
    /// ```
    #[must_use]
    pub fn id(&self) -> &DeviceId {
        &self.id
    }

    /// The name to show to users. Not unique and not stable: never store it as a key.
    /// ```
    /// # use syrp_device_api::{DeviceId, DeviceInfo};
    /// assert_eq!(DeviceInfo::new(DeviceId::new("a"), "A").name(), "A");
    /// ```
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// A change in a backend's set of devices, delivered on the message thread.
/// ```
/// use syrp_device_api::{DeviceChange, DeviceId};
/// let c = DeviceChange::Removed(DeviceId::new("a"));
/// assert!(matches!(c, DeviceChange::Removed(_)));
/// ```
#[doc(alias = "audioDeviceListChanged")]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DeviceChange {
    /// A device or port appeared.
    Added(DeviceId),
    /// A device or port disappeared (unplugged or disconnected); open handles to it fail with
    /// [`DeviceError::Disconnected`].
    Removed(DeviceId),
    /// The system default device changed.
    DefaultChanged,
}

/// The listener a [`DeviceChangeNotifier`] runs on the message thread.
/// ```
/// let _f: Box<syrp_device_api::DeviceChangeFn> = Box::new(|_mt, change| println!("{change:?}"));
/// ```
pub type DeviceChangeFn = dyn Fn(&MessageThread<'_>, &DeviceChange) + Send + Sync;

/// Posts [`DeviceChange`]s to the message loop (`syrp-loop-api`) from any thread.
///
/// The application installs one on a backend; the backend calls [`notify`](Self::notify)
/// from whatever thread its OS reports hot-plug events on, and the listener runs later on the
/// message thread, never inline.
/// ```
/// use std::sync::{Arc, Mutex};
/// use syrp_device_api::{DeviceChange, DeviceChangeNotifier};
/// use syrp_loop_api::{ManualLoop, MessageLoop};
/// let lp = ManualLoop::new();
/// let seen = Arc::new(Mutex::new(Vec::new()));
/// let s = seen.clone();
/// let n = DeviceChangeNotifier::new(lp.task_sender(), move |_mt, c| s.lock().unwrap().push(c.clone()));
/// n.notify(DeviceChange::DefaultChanged);
/// assert!(seen.lock().unwrap().is_empty());
/// lp.run_until_idle().unwrap();
/// assert_eq!(*seen.lock().unwrap(), [DeviceChange::DefaultChanged]);
/// ```
#[doc(alias = "ChangeListener")]
#[derive(Clone)]
pub struct DeviceChangeNotifier {
    sender: TaskSender,
    listener: Arc<DeviceChangeFn>,
}

impl DeviceChangeNotifier {
    /// A notifier that runs `listener` on the message thread of `sender`'s loop.
    /// ```
    /// # use syrp_loop_api::{ManualLoop, MessageLoop};
    /// let _n = syrp_device_api::DeviceChangeNotifier::new(ManualLoop::new().task_sender(), |_, _| {});
    /// ```
    pub fn new(
        sender: TaskSender,
        listener: impl Fn(&MessageThread<'_>, &DeviceChange) + Send + Sync + 'static,
    ) -> Self {
        DeviceChangeNotifier {
            sender,
            listener: Arc::new(listener),
        }
    }

    /// Queues `change` for the listener on the message thread. Callable from any thread.
    /// ```
    /// # use syrp_loop_api::{ManualLoop, MessageLoop};
    /// # use syrp_device_api::{DeviceChange, DeviceChangeNotifier};
    /// let lp = ManualLoop::new();
    /// DeviceChangeNotifier::new(lp.task_sender(), |_, _| {}).notify(DeviceChange::DefaultChanged);
    /// assert_eq!(lp.run_until_idle(), Ok(1));
    /// ```
    pub fn notify(&self, change: DeviceChange) {
        let listener = Arc::clone(&self.listener);
        self.sender.post(Task::new(move |mt| listener(mt, &change)));
    }
}

impl fmt::Debug for DeviceChangeNotifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceChangeNotifier")
            .finish_non_exhaustive()
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

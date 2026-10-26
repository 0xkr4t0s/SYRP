// SPDX-License-Identifier: MIT OR Apache-2.0
//! MIDI device traits.

use syrp_midi::MidiMessage;

use crate::{DeviceChangeNotifier, DeviceError, DeviceId, DeviceInfo};

/// Receives messages from a started [`MidiInput`], on the backend's MIDI thread.
///
/// Each message's [`timestamp`](MidiMessage::timestamp) is its arrival time in seconds on a
/// monotonic clock shared by every port of one [`MidiDeviceType`]. Closures taking
/// `&MidiMessage` implement it. Keep it short: forward to a queue rather than blocking.
/// ```
/// use syrp_device_api::MidiInputCallback;
/// let mut n = 0;
/// let mut cb = move |_m: &syrp_midi::MidiMessage| n += 1;
/// cb.message(&syrp_midi::MidiMessage::note_on(1, 60, 100).unwrap());
/// ```
pub trait MidiInputCallback: Send + 'static {
    /// Handles one complete message (SysEx arrives whole).
    /// ```
    /// # use syrp_device_api::MidiInputCallback;
    /// let mut cb = |m: &syrp_midi::MidiMessage| assert_eq!(m.timestamp(), 0.0);
    /// cb.message(&syrp_midi::MidiMessage::controller(1, 7, 0).unwrap());
    /// ```
    #[doc(alias = "handleIncomingMidiMessage")]
    fn message(&mut self, message: &MidiMessage);
}

impl<F: FnMut(&MidiMessage) + Send + 'static> MidiInputCallback for F {
    fn message(&mut self, message: &MidiMessage) {
        self(message);
    }
}

/// An open MIDI input port. Dropping it stops and closes the port.
/// ```
/// use syrp_device_api::{MidiDeviceType, fake::FakeMidiDeviceType};
/// let ty = FakeMidiDeviceType::new(&["Keys"], &[]);
/// let mut input = ty.open_input(ty.inputs()[0].id(), Box::new(|_m: &syrp_midi::MidiMessage| {}))?;
/// input.start()?;
/// # Ok::<(), syrp_device_api::DeviceError>(())
/// ```
pub trait MidiInput: Send {
    /// The port's id and name.
    /// ```
    /// # use syrp_device_api::{MidiDeviceType, fake::FakeMidiDeviceType};
    /// let ty = FakeMidiDeviceType::new(&["Keys"], &[]);
    /// let input = ty.open_input(ty.inputs()[0].id(), Box::new(|_: &syrp_midi::MidiMessage| {})).unwrap();
    /// assert_eq!(input.info().name(), "Keys");
    /// ```
    #[doc(alias = "getDeviceInfo")]
    fn info(&self) -> &DeviceInfo;

    /// Starts delivering messages to the callback.
    ///
    /// # Errors
    /// [`DeviceError::Disconnected`] if the port has gone.
    /// ```
    /// # use syrp_device_api::{MidiDeviceType, fake::FakeMidiDeviceType};
    /// let ty = FakeMidiDeviceType::new(&["Keys"], &[]);
    /// let mut input = ty.open_input(ty.inputs()[0].id(), Box::new(|_: &syrp_midi::MidiMessage| {})).unwrap();
    /// assert!(input.start().is_ok());
    /// ```
    fn start(&mut self) -> Result<(), DeviceError>;

    /// Stops delivery; messages arriving while stopped are discarded.
    /// ```
    /// # use syrp_device_api::{MidiDeviceType, fake::FakeMidiDeviceType};
    /// let ty = FakeMidiDeviceType::new(&["Keys"], &[]);
    /// ty.open_input(ty.inputs()[0].id(), Box::new(|_: &syrp_midi::MidiMessage| {})).unwrap().stop();
    /// ```
    fn stop(&mut self);
}

/// An open MIDI output port. Dropping it closes the port.
/// ```
/// use syrp_device_api::{MidiDeviceType, fake::FakeMidiDeviceType};
/// let ty = FakeMidiDeviceType::new(&[], &["Synth"]);
/// let mut out = ty.open_output(ty.outputs()[0].id())?;
/// out.send(&syrp_midi::MidiMessage::note_on(1, 60, 100).unwrap())?;
/// # Ok::<(), syrp_device_api::DeviceError>(())
/// ```
pub trait MidiOutput: Send {
    /// The port's id and name.
    /// ```
    /// # use syrp_device_api::{MidiDeviceType, fake::FakeMidiDeviceType};
    /// let ty = FakeMidiDeviceType::new(&[], &["Synth"]);
    /// assert_eq!(ty.open_output(ty.outputs()[0].id()).unwrap().info().name(), "Synth");
    /// ```
    #[doc(alias = "getDeviceInfo")]
    fn info(&self) -> &DeviceInfo;

    /// Sends `message` now; its timestamp is ignored.
    ///
    /// # Errors
    /// [`DeviceError::Disconnected`] if the port has gone.
    /// ```
    /// # use syrp_device_api::{MidiDeviceType, fake::FakeMidiDeviceType};
    /// let ty = FakeMidiDeviceType::new(&[], &["Synth"]);
    /// let mut out = ty.open_output(ty.outputs()[0].id()).unwrap();
    /// assert!(out.send(&syrp_midi::MidiMessage::controller(1, 7, 0).unwrap()).is_ok());
    /// ```
    #[doc(alias = "sendMessageNow")]
    fn send(&mut self, message: &MidiMessage) -> Result<(), DeviceError>;
}

/// A MIDI backend (CoreMIDI, Windows MIDI Services, ALSA...): enumerates and opens ports.
/// ```
/// use syrp_device_api::{MidiDeviceType, fake::FakeMidiDeviceType};
/// let ty = FakeMidiDeviceType::new(&["In"], &["Out"]);
/// assert_eq!((ty.inputs().len(), ty.outputs().len()), (1, 1));
/// ```
pub trait MidiDeviceType {
    /// The backend's name, such as `"CoreMIDI"`.
    /// ```
    /// # use syrp_device_api::{MidiDeviceType, fake::FakeMidiDeviceType};
    /// assert_eq!(FakeMidiDeviceType::new(&[], &[]).name(), "Fake");
    /// ```
    fn name(&self) -> &str;

    /// The input ports present now.
    /// ```
    /// # use syrp_device_api::{MidiDeviceType, fake::FakeMidiDeviceType};
    /// assert_eq!(FakeMidiDeviceType::new(&["A"], &[]).inputs()[0].name(), "A");
    /// ```
    #[doc(alias = "getAvailableDevices")]
    fn inputs(&self) -> Vec<DeviceInfo>;

    /// The output ports present now.
    /// ```
    /// # use syrp_device_api::{MidiDeviceType, fake::FakeMidiDeviceType};
    /// assert!(FakeMidiDeviceType::new(&["A"], &[]).outputs().is_empty());
    /// ```
    #[doc(alias = "getAvailableDevices")]
    fn outputs(&self) -> Vec<DeviceInfo>;

    /// Opens an input port, stopped, that will deliver to `callback`.
    ///
    /// # Errors
    /// [`DeviceError::NotFound`] if no input has `id`.
    /// ```
    /// # use syrp_device_api::{DeviceError, DeviceId, MidiDeviceType, fake::FakeMidiDeviceType};
    /// let r = FakeMidiDeviceType::new(&[], &[]).open_input(&DeviceId::new("x"), Box::new(|_: &syrp_midi::MidiMessage| {}));
    /// assert_eq!(r.err(), Some(DeviceError::NotFound));
    /// ```
    #[doc(alias = "openDevice")]
    fn open_input(
        &self,
        id: &DeviceId,
        callback: Box<dyn MidiInputCallback>,
    ) -> Result<Box<dyn MidiInput>, DeviceError>;

    /// Opens an output port.
    ///
    /// # Errors
    /// [`DeviceError::NotFound`] if no output has `id`.
    /// ```
    /// # use syrp_device_api::{DeviceError, DeviceId, MidiDeviceType, fake::FakeMidiDeviceType};
    /// assert_eq!(FakeMidiDeviceType::new(&[], &[]).open_output(&DeviceId::new("x")).err(), Some(DeviceError::NotFound));
    /// ```
    #[doc(alias = "openDevice")]
    fn open_output(&self, id: &DeviceId) -> Result<Box<dyn MidiOutput>, DeviceError>;

    /// Installs (or with `None` removes) the notifier told about hot-plug changes.
    /// ```
    /// # use syrp_device_api::{MidiDeviceType, fake::FakeMidiDeviceType};
    /// FakeMidiDeviceType::new(&[], &[]).set_change_notifier(None);
    /// ```
    #[doc(alias = "MidiDeviceListConnection")]
    fn set_change_notifier(&self, notifier: Option<DeviceChangeNotifier>);
}

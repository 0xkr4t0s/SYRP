// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plugin state: the `PluginState` trait and its versioned envelope.

use crate::PluginError;

const MAGIC: [u8; 4] = *b"SYST";
const ENVELOPE_FORMAT: u16 = 1;
const HEADER: usize = 20;

/// A plugin whose state hosts save into projects and presets. Message thread only.
///
/// The plugin writes its own payload; [`save_state`] and [`load_state`] wrap it in a
/// [`StateEnvelope`] stamped with [`state_version`](Self::state_version), so a newer plugin
/// can migrate an older payload and an older plugin refuses a newer one.
/// ```
/// use syrp_plugin_api::{PluginError, PluginState, load_state, save_state};
/// struct Gain(u8);
/// impl PluginState for Gain {
///     fn state_version(&self) -> u32 { 2 }
///     fn save_state(&self, out: &mut Vec<u8>) -> Result<(), PluginError> { out.push(self.0); Ok(()) }
///     fn load_state(&mut self, version: u32, data: &[u8]) -> Result<(), PluginError> {
///         // Version 1 stored the gain doubled.
///         let raw = *data.first().ok_or(PluginError::StateMalformed)?;
///         self.0 = if version == 1 { raw / 2 } else { raw };
///         Ok(())
///     }
/// }
/// let blob = save_state(&Gain(7)).unwrap();
/// let mut g = Gain(0);
/// load_state(&mut g, &blob).unwrap();
/// assert_eq!(g.0, 7);
/// ```
#[doc(alias = "getStateInformation")]
#[doc(alias = "setStateInformation")]
pub trait PluginState {
    /// The version of the payload [`save_state`](Self::save_state) writes. Raise it whenever
    /// the payload changes shape.
    fn state_version(&self) -> u32;

    /// Appends the current state's payload to `out`.
    /// ```
    /// # struct S; impl syrp_plugin_api::PluginState for S { fn state_version(&self) -> u32 { 3 } fn save_state(&self, o: &mut Vec<u8>) -> Result<(), syrp_plugin_api::PluginError> { o.push(1); Ok(()) } fn load_state(&mut self, _: u32, _: &[u8]) -> Result<(), syrp_plugin_api::PluginError> { Ok(()) } }
    /// let mut out = Vec::new();
    /// syrp_plugin_api::PluginState::save_state(&S, &mut out).unwrap();
    /// assert_eq!(out, [1]);
    /// ```
    fn save_state(&self, out: &mut Vec<u8>) -> Result<(), PluginError>;

    /// Restores from a payload written at `version`, which is never above
    /// [`state_version`](Self::state_version). Migrate older versions here.
    fn load_state(&mut self, version: u32, data: &[u8]) -> Result<(), PluginError>;
}

/// A parsed state blob: the payload and the version it was written at.
///
/// Layout, little-endian: `b"SYST"`, envelope format `u16` (1), reserved `u16` (0), state
/// version `u32`, payload length `u64`, then exactly that many payload bytes.
/// ```
/// use syrp_plugin_api::StateEnvelope;
/// let mut blob = Vec::new();
/// StateEnvelope::write(4, b"abc", &mut blob);
/// let env = StateEnvelope::parse(&blob).unwrap();
/// assert_eq!((env.version(), env.payload()), (4, &b"abc"[..]));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateEnvelope<'a> {
    version: u32,
    payload: &'a [u8],
}

impl<'a> StateEnvelope<'a> {
    /// Appends an envelope holding `payload` at `version` to `out`.
    /// ```
    /// let mut out = Vec::new();
    /// syrp_plugin_api::StateEnvelope::write(1, &[], &mut out);
    /// assert_eq!(out.len(), 20);
    /// ```
    pub fn write(version: u32, payload: &[u8], out: &mut Vec<u8>) {
        out.reserve(HEADER + payload.len());
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&ENVELOPE_FORMAT.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&version.to_le_bytes());
        out.extend_from_slice(&(payload.len() as u64).to_le_bytes());
        out.extend_from_slice(payload);
    }

    /// Reads an envelope. Fails with [`PluginError::StateMalformed`] on a wrong magic, a
    /// truncated or over-long blob or a nonzero reserved field, and with
    /// [`PluginError::StateTooNew`] on an envelope format above 1.
    /// ```
    /// use syrp_plugin_api::{PluginError, StateEnvelope};
    /// assert_eq!(StateEnvelope::parse(b"JUCE"), Err(PluginError::StateMalformed));
    /// ```
    pub fn parse(bytes: &'a [u8]) -> Result<Self, PluginError> {
        let (header, payload) = bytes
            .split_first_chunk::<HEADER>()
            .ok_or(PluginError::StateMalformed)?;
        let field = |at: usize, n: usize| &header[at..at + n];
        let u16_at = |at| u16::from_le_bytes([header[at], header[at + 1]]);
        if field(0, 4) != MAGIC || u16_at(6) != 0 {
            return Err(PluginError::StateMalformed);
        }
        if u16_at(4) > ENVELOPE_FORMAT {
            return Err(PluginError::StateTooNew);
        }
        let version = u32::from_le_bytes(field(8, 4).try_into().unwrap_or_default());
        let len = u64::from_le_bytes(field(12, 8).try_into().unwrap_or_default());
        if u64::try_from(payload.len()) != Ok(len) {
            return Err(PluginError::StateMalformed);
        }
        Ok(Self { version, payload })
    }

    /// The version the payload was written at.
    /// ```
    /// # let mut b = Vec::new(); syrp_plugin_api::StateEnvelope::write(9, &[], &mut b);
    /// assert_eq!(syrp_plugin_api::StateEnvelope::parse(&b).unwrap().version(), 9);
    /// ```
    #[must_use]
    pub fn version(&self) -> u32 {
        self.version
    }

    /// The plugin's payload bytes.
    /// ```
    /// # let mut b = Vec::new(); syrp_plugin_api::StateEnvelope::write(9, &[5], &mut b);
    /// assert_eq!(syrp_plugin_api::StateEnvelope::parse(&b).unwrap().payload(), [5]);
    /// ```
    #[must_use]
    pub fn payload(&self) -> &'a [u8] {
        self.payload
    }
}

/// Saves `plugin`'s state as an enveloped blob stamped with its state version.
/// ```
/// # struct S; impl syrp_plugin_api::PluginState for S { fn state_version(&self) -> u32 { 3 } fn save_state(&self, _: &mut Vec<u8>) -> Result<(), syrp_plugin_api::PluginError> { Ok(()) } fn load_state(&mut self, _: u32, _: &[u8]) -> Result<(), syrp_plugin_api::PluginError> { Ok(()) } }
/// let blob = syrp_plugin_api::save_state(&S).unwrap();
/// assert_eq!(syrp_plugin_api::StateEnvelope::parse(&blob).unwrap().version(), 3);
/// ```
pub fn save_state<S: PluginState + ?Sized>(plugin: &S) -> Result<Vec<u8>, PluginError> {
    let mut payload = Vec::new();
    plugin.save_state(&mut payload)?;
    let mut out = Vec::new();
    StateEnvelope::write(plugin.state_version(), &payload, &mut out);
    Ok(out)
}

/// Restores `plugin` from a blob made by [`save_state`]. Fails with
/// [`PluginError::StateTooNew`] when the blob was written by a newer state version, without
/// calling the plugin, or with whatever parsing or `load_state` returns.
/// ```
/// # struct S; impl syrp_plugin_api::PluginState for S { fn state_version(&self) -> u32 { 3 } fn save_state(&self, _: &mut Vec<u8>) -> Result<(), syrp_plugin_api::PluginError> { Ok(()) } fn load_state(&mut self, _: u32, _: &[u8]) -> Result<(), syrp_plugin_api::PluginError> { Ok(()) } }
/// let mut newer = Vec::new();
/// syrp_plugin_api::StateEnvelope::write(4, &[], &mut newer);
/// assert_eq!(syrp_plugin_api::load_state(&mut S, &newer), Err(syrp_plugin_api::PluginError::StateTooNew));
/// ```
pub fn load_state<S: PluginState + ?Sized>(plugin: &mut S, blob: &[u8]) -> Result<(), PluginError> {
    let envelope = StateEnvelope::parse(blob)?;
    if envelope.version() > plugin.state_version() {
        return Err(PluginError::StateTooNew);
    }
    plugin.load_state(envelope.version(), envelope.payload())
}

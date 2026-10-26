// SPDX-License-Identifier: MIT OR Apache-2.0
//! `PluginDescriptor`: what hosts show and store about a plugin.

use crate::PluginError;

/// A plugin format, for format-specific unique ids.
/// ```
/// assert_ne!(syrp_plugin_api::PluginFormat::Clap, syrp_plugin_api::PluginFormat::Vst3);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum PluginFormat {
    /// CLAP: a reverse-DNS id such as `com.example.gain`.
    Clap,
    /// VST3: a 16-byte class id written as 32 hex digits.
    Vst3,
    /// Audio Unit: a four-character subtype code.
    AudioUnit,
    /// LV2: a URI.
    Lv2,
    /// AAX: a plugin id string.
    Aax,
}

/// What kind of plugin this is, for host browsers.
/// ```
/// assert_ne!(syrp_plugin_api::PluginCategory::Effect, syrp_plugin_api::PluginCategory::Instrument);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PluginCategory {
    /// Makes sound from notes.
    Instrument,
    /// Processes audio.
    Effect,
    /// Processes MIDI only.
    MidiEffect,
    /// Measures without changing the audio.
    Analyzer,
    /// Delay or echo.
    Delay,
    /// Reverb.
    Reverb,
    /// Compressor, limiter, gate.
    Dynamics,
    /// Equaliser or filter.
    Filter,
    /// Distortion or saturation.
    Distortion,
    /// Chorus, flanger, phaser and similar.
    Modulation,
    /// Gain, routing, metering helpers.
    Utility,
}

/// A plugin's name, vendor, version, unique id per format and categories.
/// ```
/// use syrp_plugin_api::{PluginCategory, PluginDescriptor, PluginFormat};
/// let d = PluginDescriptor::new("Gain", "Example", (1, 2, 0))
///     .with_id(PluginFormat::Clap, "com.example.gain")
///     .with_category(PluginCategory::Effect);
/// assert_eq!(d.id(PluginFormat::Clap), Some("com.example.gain"));
/// assert!(d.validate().is_ok());
/// ```
#[doc(alias = "PluginDescription")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginDescriptor {
    name: String,
    vendor: String,
    version: (u32, u32, u32),
    ids: Vec<(PluginFormat, String)>,
    categories: Vec<PluginCategory>,
}

impl PluginDescriptor {
    /// A descriptor with no ids or categories yet; `version` is major, minor, patch.
    /// ```
    /// let d = syrp_plugin_api::PluginDescriptor::new("Gain", "Example", (1, 0, 3));
    /// assert_eq!((d.name(), d.vendor(), d.version()), ("Gain", "Example", (1, 0, 3)));
    /// ```
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        vendor: impl Into<String>,
        version: (u32, u32, u32),
    ) -> Self {
        let (name, vendor) = (name.into(), vendor.into());
        Self {
            name,
            vendor,
            version,
            ids: Vec::new(),
            categories: Vec::new(),
        }
    }

    /// Sets the unique id for `format`, replacing any earlier one.
    /// ```
    /// use syrp_plugin_api::{PluginDescriptor, PluginFormat::Clap};
    /// let d = PluginDescriptor::new("G", "E", (1, 0, 0)).with_id(Clap, "a.b").with_id(Clap, "a.c");
    /// assert_eq!(d.id(Clap), Some("a.c"));
    /// ```
    #[must_use]
    pub fn with_id(mut self, format: PluginFormat, id: impl Into<String>) -> Self {
        self.ids.retain(|(f, _)| *f != format);
        self.ids.push((format, id.into()));
        self
    }

    /// Adds a category, keeping the first occurrence of each.
    /// ```
    /// use syrp_plugin_api::{PluginCategory::Reverb, PluginDescriptor};
    /// let d = PluginDescriptor::new("R", "E", (1, 0, 0)).with_category(Reverb).with_category(Reverb);
    /// assert_eq!(d.categories(), [Reverb]);
    /// ```
    #[must_use]
    pub fn with_category(mut self, category: PluginCategory) -> Self {
        if !self.categories.contains(&category) {
            self.categories.push(category);
        }
        self
    }

    /// The display name.
    /// ```
    /// assert_eq!(syrp_plugin_api::PluginDescriptor::new("G", "E", (1, 0, 0)).name(), "G");
    /// ```
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The vendor (manufacturer) name.
    /// ```
    /// assert_eq!(syrp_plugin_api::PluginDescriptor::new("G", "E", (1, 0, 0)).vendor(), "E");
    /// ```
    #[must_use]
    pub fn vendor(&self) -> &str {
        &self.vendor
    }

    /// The version as major, minor, patch.
    /// ```
    /// assert_eq!(syrp_plugin_api::PluginDescriptor::new("G", "E", (2, 1, 0)).version(), (2, 1, 0));
    /// ```
    #[must_use]
    pub fn version(&self) -> (u32, u32, u32) {
        self.version
    }

    /// The unique id for `format`, if the plugin ships in it.
    /// ```
    /// use syrp_plugin_api::{PluginDescriptor, PluginFormat};
    /// assert_eq!(PluginDescriptor::new("G", "E", (1, 0, 0)).id(PluginFormat::Vst3), None);
    /// ```
    #[must_use]
    pub fn id(&self, format: PluginFormat) -> Option<&str> {
        self.ids
            .iter()
            .find(|(f, _)| *f == format)
            .map(|(_, id)| id.as_str())
    }

    /// The categories, in the order added.
    /// ```
    /// assert!(syrp_plugin_api::PluginDescriptor::new("G", "E", (1, 0, 0)).categories().is_empty());
    /// ```
    #[must_use]
    pub fn categories(&self) -> &[PluginCategory] {
        &self.categories
    }

    /// Checks that the name and vendor are set, at least one id is, and each id has its
    /// format's shape: VST3 32 hex digits, Audio Unit four printable ASCII characters, the
    /// others non-empty without whitespace. Fails with [`PluginError::InvalidDescriptor`].
    /// ```
    /// use syrp_plugin_api::{PluginDescriptor, PluginError, PluginFormat};
    /// let d = PluginDescriptor::new("G", "E", (1, 0, 0)).with_id(PluginFormat::Vst3, "not-hex");
    /// assert_eq!(d.validate(), Err(PluginError::InvalidDescriptor));
    /// ```
    pub fn validate(&self) -> Result<(), PluginError> {
        let id_ok = |(format, id): &(PluginFormat, String)| match format {
            PluginFormat::Vst3 => id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()),
            PluginFormat::AudioUnit => id.len() == 4 && id.bytes().all(|b| b.is_ascii_graphic()),
            _ => !id.is_empty() && !id.contains(char::is_whitespace),
        };
        let named = !self.name.trim().is_empty() && !self.vendor.trim().is_empty();
        let ok = named && !self.ids.is_empty() && self.ids.iter().all(id_ok);
        ok.then_some(()).ok_or(PluginError::InvalidDescriptor)
    }
}

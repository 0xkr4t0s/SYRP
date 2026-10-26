// SPDX-License-Identifier: MIT OR Apache-2.0
//! Parameter declaration: `ParamIndex`, `Smoothing` and `ParamSpec`.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use syrp_rt::RtSafe;

use crate::ParamRange;

/// The dense index of a registered parameter: registration order, starting at 0. The audio
/// thread uses only indices; [`ParamLayout::index_of`](crate::ParamLayout::index_of) resolves
/// string ids before processing starts.
/// ```
/// use syrp_plugin_api::ParamIndex;
/// assert_eq!(ParamIndex::new(3).get(), 3);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ParamIndex(u32);

impl ParamIndex {
    /// The index with value `index`.
    /// ```
    /// assert_eq!(syrp_plugin_api::ParamIndex::new(7).get(), 7);
    /// ```
    #[must_use]
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    /// The index as `u32`.
    /// ```
    /// assert_eq!(syrp_plugin_api::ParamIndex::new(1).get(), 1u32);
    /// ```
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    /// The index as `usize`, for indexing slices.
    /// ```
    /// assert_eq!(syrp_plugin_api::ParamIndex::new(2).as_usize(), 2usize);
    /// ```
    #[must_use]
    pub const fn as_usize(self) -> usize {
        self.0 as usize
    }
}

// SAFETY: a plain `u32` newtype with no destructor and no RT-unsafe methods.
unsafe impl RtSafe for ParamIndex {}

/// How a processor should smooth changes to a parameter's value (the processor smooths).
/// ```
/// use syrp_plugin_api::Smoothing;
/// assert_ne!(Smoothing::Linear(std::time::Duration::from_millis(20)), Smoothing::None);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum Smoothing {
    /// Jump straight to the new value.
    #[default]
    None,
    /// Ramp linearly to the new value over the given time.
    Linear(Duration),
    /// Ramp by a constant ratio per sample over the given time (for gains and frequencies).
    Multiplicative(Duration),
}

type ToText = Arc<dyn Fn(f64) -> String + Send + Sync>;
type FromText = Arc<dyn Fn(&str) -> Option<f64> + Send + Sync>;

/// The declaration of one parameter, built on the message thread.
///
/// Required: a string id (unique within a processor and stable across versions), a display
/// name and a [`ParamRange`]. Optional, with defaults: default value (the range start), unit
/// (empty), text conversion (decimal plus unit), [`Smoothing`] (none), automatable (true) and
/// group (none).
/// ```
/// use syrp_plugin_api::{ParamRange, ParamSpec};
/// let spec = ParamSpec::new("cutoff", "Cutoff", ParamRange::new(20.0, 20_000.0).with_centre(1_000.0))
///     .with_default(1_000.0)
///     .with_unit("Hz")
///     .with_group("filter");
/// assert_eq!(spec.value_to_text(440.0), "440.00 Hz");
/// assert_eq!(spec.text_to_value("440 Hz"), Some(440.0));
/// ```
#[derive(Clone)]
#[doc(alias = "AudioParameterFloat")]
#[doc(alias = "RangedAudioParameter")]
pub struct ParamSpec {
    id: String,
    name: String,
    range: ParamRange,
    default: Option<f64>,
    unit: String,
    to_text: Option<ToText>,
    from_text: Option<FromText>,
    smoothing: Smoothing,
    automatable: bool,
    group: Option<String>,
}

impl ParamSpec {
    /// A parameter with the given string id, display name and range.
    /// ```
    /// # use syrp_plugin_api::{ParamRange, ParamSpec};
    /// let p = ParamSpec::new("mix", "Mix", ParamRange::new(0.0, 1.0));
    /// assert_eq!((p.id(), p.name(), p.range().end()), ("mix", "Mix", 1.0));
    /// ```
    #[doc(alias = "getParameterID")]
    #[must_use]
    pub fn new(id: impl Into<String>, name: impl Into<String>, range: ParamRange) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            range,
            default: None,
            unit: String::new(),
            to_text: None,
            from_text: None,
            smoothing: Smoothing::None,
            automatable: true,
            group: None,
        }
    }

    /// Sets the default plain value, which must lie inside the range.
    /// ```
    /// # use syrp_plugin_api::{ParamRange, ParamSpec};
    /// let p = ParamSpec::new("q", "Q", ParamRange::new(0.0, 10.0)).with_default(2.5);
    /// assert_eq!(p.range().normalise(p.default_value()), 0.25);
    /// ```
    #[must_use]
    pub fn with_default(mut self, value: f64) -> Self {
        self.default = Some(value);
        self
    }

    /// Sets the unit label, such as `"dB"` or `"Hz"`.
    /// ```
    /// # use syrp_plugin_api::{ParamRange, ParamSpec};
    /// assert_eq!(ParamSpec::new("g", "G", ParamRange::new(-60.0, 6.0)).with_unit("dB").unit(), "dB");
    /// ```
    #[doc(alias = "getLabel")]
    #[must_use]
    pub fn with_unit(mut self, unit: impl Into<String>) -> Self {
        self.unit = unit.into();
        self
    }

    /// Replaces the value-to-text conversion.
    /// ```
    /// # use syrp_plugin_api::{ParamRange, ParamSpec};
    /// let p = ParamSpec::new("on", "On", ParamRange::new(0.0, 1.0).with_step(1.0))
    ///     .with_value_to_text(|v| if v >= 0.5 { "On".into() } else { "Off".into() });
    /// assert_eq!(p.value_to_text(1.0), "On");
    /// ```
    #[doc(alias = "getText")]
    #[must_use]
    pub fn with_value_to_text(mut self, f: impl Fn(f64) -> String + Send + Sync + 'static) -> Self {
        self.to_text = Some(Arc::new(f));
        self
    }

    /// Replaces the text-to-value conversion. Its result is still clamped and snapped.
    /// ```
    /// # use syrp_plugin_api::{ParamRange, ParamSpec};
    /// let p = ParamSpec::new("on", "On", ParamRange::new(0.0, 1.0).with_step(1.0))
    ///     .with_text_to_value(|t| Some(if t == "On" { 1.0 } else { 0.0 }));
    /// assert_eq!(p.text_to_value("On"), Some(1.0));
    /// ```
    #[doc(alias = "getValueForText")]
    #[must_use]
    pub fn with_text_to_value(
        mut self,
        f: impl Fn(&str) -> Option<f64> + Send + Sync + 'static,
    ) -> Self {
        self.from_text = Some(Arc::new(f));
        self
    }

    /// Sets the requested smoothing.
    /// ```
    /// # use syrp_plugin_api::{ParamRange, ParamSpec, Smoothing};
    /// let s = Smoothing::Linear(std::time::Duration::from_millis(5));
    /// assert_eq!(ParamSpec::new("g", "G", ParamRange::new(0.0, 1.0)).with_smoothing(s).smoothing(), s);
    /// ```
    #[must_use]
    pub fn with_smoothing(mut self, smoothing: Smoothing) -> Self {
        self.smoothing = smoothing;
        self
    }

    /// Sets whether hosts may automate the parameter.
    /// ```
    /// # use syrp_plugin_api::{ParamRange, ParamSpec};
    /// let p = ParamSpec::new("m", "M", ParamRange::new(0.0, 1.0));
    /// assert!(p.is_automatable() && !p.with_automatable(false).is_automatable());
    /// ```
    #[doc(alias = "isAutomatable")]
    #[must_use]
    pub fn with_automatable(mut self, automatable: bool) -> Self {
        self.automatable = automatable;
        self
    }

    /// Puts the parameter in a group, given as a path of group ids separated by `/`.
    /// ```
    /// # use syrp_plugin_api::{ParamRange, ParamSpec};
    /// let p = ParamSpec::new("lo", "Low", ParamRange::new(0.0, 1.0));
    /// assert_eq!((p.group(), p.clone().with_group("eq/low").group()), (None, Some("eq/low")));
    /// ```
    #[doc(alias = "AudioProcessorParameterGroup")]
    #[must_use]
    pub fn with_group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
    }

    /// The string id. Hosts store automation and state by it.
    /// ```
    /// # use syrp_plugin_api::{ParamRange, ParamSpec};
    /// assert_eq!(ParamSpec::new("drive", "Drive", ParamRange::new(0.0, 1.0)).id(), "drive");
    /// ```
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The display name.
    /// ```
    /// # use syrp_plugin_api::{ParamRange, ParamSpec};
    /// assert_eq!(ParamSpec::new("drive", "Drive", ParamRange::new(0.0, 1.0)).name(), "Drive");
    /// ```
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The range.
    /// ```
    /// # use syrp_plugin_api::{ParamRange, ParamSpec};
    /// assert_eq!(ParamSpec::new("x", "X", ParamRange::new(0.0, 4.0)).range().end(), 4.0);
    /// ```
    #[must_use]
    pub fn range(&self) -> &ParamRange {
        &self.range
    }

    /// The default plain value: the one set, or else the range start.
    /// ```
    /// # use syrp_plugin_api::{ParamRange, ParamSpec};
    /// assert_eq!(ParamSpec::new("x", "X", ParamRange::new(-3.0, 4.0)).default_value(), -3.0);
    /// ```
    #[doc(alias = "getDefaultValue")]
    #[must_use]
    pub fn default_value(&self) -> f64 {
        self.default.unwrap_or(self.range.start())
    }

    /// The unit label, empty when none was set.
    /// ```
    /// # use syrp_plugin_api::{ParamRange, ParamSpec};
    /// assert_eq!(ParamSpec::new("x", "X", ParamRange::new(0.0, 1.0)).unit(), "");
    /// ```
    #[must_use]
    pub fn unit(&self) -> &str {
        &self.unit
    }

    /// The requested smoothing.
    /// ```
    /// # use syrp_plugin_api::{ParamRange, ParamSpec, Smoothing};
    /// assert_eq!(ParamSpec::new("x", "X", ParamRange::new(0.0, 1.0)).smoothing(), Smoothing::None);
    /// ```
    #[must_use]
    pub fn smoothing(&self) -> Smoothing {
        self.smoothing
    }

    /// Whether hosts may automate the parameter (true unless turned off).
    /// ```
    /// # use syrp_plugin_api::{ParamRange, ParamSpec};
    /// assert!(ParamSpec::new("x", "X", ParamRange::new(0.0, 1.0)).is_automatable());
    /// ```
    #[must_use]
    pub fn is_automatable(&self) -> bool {
        self.automatable
    }

    /// The group path, if any.
    /// ```
    /// # use syrp_plugin_api::{ParamRange, ParamSpec};
    /// assert_eq!(ParamSpec::new("x", "X", ParamRange::new(0.0, 1.0)).group(), None);
    /// ```
    #[must_use]
    pub fn group(&self) -> Option<&str> {
        self.group.as_deref()
    }

    /// Formats a plain value. The default prints the fewest decimals (at most six) that show
    /// every step from the range start exactly, two when continuous, then a space and the
    /// unit when there is one.
    /// ```
    /// # use syrp_plugin_api::{ParamRange, ParamSpec};
    /// let p = ParamSpec::new("n", "N", ParamRange::new(0.0, 8.0).with_step(1.0));
    /// assert_eq!(p.value_to_text(3.0), "3");
    /// ```
    #[must_use]
    pub fn value_to_text(&self, value: f64) -> String {
        if let Some(f) = &self.to_text {
            return f(value);
        }
        let (step, start) = (self.range.step(), self.range.start());
        let whole = |x: f64| (x - x.round()).abs() <= 1e-6 * x.abs().max(1.0);
        let decimals = if step > 0.0 {
            let scaled = |d: i32| (step * 10f64.powi(d), start * 10f64.powi(d));
            (0..6)
                .find(|&d| matches!(scaled(d), (a, b) if whole(a) && whole(b)))
                .unwrap_or(6) as usize
        } else {
            2
        };
        match self.unit.as_str() {
            "" => format!("{value:.decimals$}"),
            unit => format!("{value:.decimals$} {unit}"),
        }
    }

    /// Parses text into a plain value, clamped into the range and snapped to the step.
    /// The default accepts a decimal number, optionally followed by the unit.
    /// ```
    /// # use syrp_plugin_api::{ParamRange, ParamSpec};
    /// let p = ParamSpec::new("g", "G", ParamRange::new(-60.0, 6.0)).with_unit("dB");
    /// assert_eq!((p.text_to_value(" -3 dB "), p.text_to_value("99"), p.text_to_value("x")), (Some(-3.0), Some(6.0), None));
    /// ```
    #[must_use]
    pub fn text_to_value(&self, text: &str) -> Option<f64> {
        let parsed = match &self.from_text {
            Some(f) => f(text),
            None => {
                let t = text.trim();
                let t = t.strip_suffix(self.unit.as_str()).unwrap_or(t);
                t.trim_end().parse::<f64>().ok()
            }
        }?;
        (!parsed.is_nan()).then(|| self.range.snap(parsed))
    }

    /// Whether the id is non-empty, the range valid and the default finite and in range.
    pub(crate) fn is_valid(&self) -> bool {
        let d = self.default_value();
        !self.id.is_empty()
            && self.range.is_valid()
            && (self.range.start()..=self.range.end()).contains(&d)
    }
}

impl fmt::Debug for ParamSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParamSpec")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("range", &self.range)
            .field("default", &self.default_value())
            .field("unit", &self.unit)
            .field("smoothing", &self.smoothing)
            .field("automatable", &self.automatable)
            .field("group", &self.group)
            .finish_non_exhaustive()
    }
}

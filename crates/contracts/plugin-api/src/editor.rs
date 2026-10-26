// SPDX-License-Identifier: MIT OR Apache-2.0
//! The editor contract: `Editor`, `EditorHost`, `NodeHandle`, `SizeConstraints`, `ParamHandles`.
//!
//! This crate is L1 and never names `syrp-ui` (L4) types. An editor builds its UI through the
//! opaque [`EditorHost`] and returns the root of what it built as a [`NodeHandle`].

use std::any::Any;
use std::fmt;
use std::sync::Arc;

use syrp_error::{Diagnostic, ErrorCode};

use crate::{ParamIndex, ParamLayout, ParamSpec, Params};

/// A UI node: a 64-bit generational key, a 32-bit slot index plus a 32-bit generation.
///
/// `syrp-ui`'s `ComponentId` converts to and from it losslessly through
/// [`to_bits`](Self::to_bits) and [`from_bits`](Self::from_bits).
/// ```
/// use syrp_plugin_api::NodeHandle;
/// let node = NodeHandle::new(7, 2);
/// assert_eq!((node.index(), node.generation()), (7, 2));
/// assert_eq!(NodeHandle::from_bits(node.to_bits()), node);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeHandle(u64);

impl NodeHandle {
    /// The handle for slot `index` at `generation`.
    /// ```
    /// assert_eq!(syrp_plugin_api::NodeHandle::new(1, 0).index(), 1);
    /// ```
    #[must_use]
    pub const fn new(index: u32, generation: u32) -> Self {
        Self(((generation as u64) << 32) | index as u64)
    }

    /// The slot index (low 32 bits).
    /// ```
    /// assert_eq!(syrp_plugin_api::NodeHandle::new(u32::MAX, 3).index(), u32::MAX);
    /// ```
    #[must_use]
    pub const fn index(self) -> u32 {
        self.0 as u32
    }

    /// The generation (high 32 bits), bumped each time the slot is reused.
    /// ```
    /// assert_eq!(syrp_plugin_api::NodeHandle::new(0, 9).generation(), 9);
    /// ```
    #[must_use]
    pub const fn generation(self) -> u32 {
        (self.0 >> 32) as u32
    }

    /// The key as one `u64`: generation in the high half, index in the low half.
    /// ```
    /// assert_eq!(syrp_plugin_api::NodeHandle::new(1, 1).to_bits(), 0x1_0000_0001);
    /// ```
    #[must_use]
    pub const fn to_bits(self) -> u64 {
        self.0
    }

    /// The handle with the given [`to_bits`](Self::to_bits) value. Every `u64` is valid.
    /// ```
    /// assert_eq!(syrp_plugin_api::NodeHandle::from_bits(5).index(), 5);
    /// ```
    #[must_use]
    pub const fn from_bits(bits: u64) -> Self {
        Self(bits)
    }
}

/// What an editor builds into, implemented by the L5 `syrp-plugin` runtime over `syrp-ui`.
///
/// Object safe. Plugins reach the concrete UI through the runtime's typed accessor, which
/// downcasts [`as_any_mut`](Self::as_any_mut); this crate never sees the UI type.
/// ```
/// use syrp_plugin_api::{EditorHost, NodeHandle};
/// struct Headless;
/// impl EditorHost for Headless {
///     fn root(&self) -> NodeHandle { NodeHandle::new(0, 0) }
///     fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
/// }
/// let host: &mut dyn EditorHost = &mut Headless;
/// assert!(host.as_any_mut().downcast_mut::<Headless>().is_some());
/// ```
pub trait EditorHost {
    /// The node the editor's UI is attached under.
    /// ```
    /// # struct H; impl syrp_plugin_api::EditorHost for H { fn root(&self) -> syrp_plugin_api::NodeHandle { syrp_plugin_api::NodeHandle::new(4, 0) } fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self } }
    /// assert_eq!(syrp_plugin_api::EditorHost::root(&H).index(), 4);
    /// ```
    fn root(&self) -> NodeHandle;

    /// The host as `Any`, for the runtime's typed accessor.
    /// ```
    /// # struct H; impl syrp_plugin_api::EditorHost for H { fn root(&self) -> syrp_plugin_api::NodeHandle { syrp_plugin_api::NodeHandle::new(4, 0) } fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self } }
    /// assert!(syrp_plugin_api::EditorHost::as_any_mut(&mut H).is::<H>());
    /// ```
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// The sizes an editor accepts, in logical pixels.
/// ```
/// use syrp_plugin_api::SizeConstraints;
/// let c = SizeConstraints::fixed(400.0, 300.0);
/// assert_eq!(c.clamp(1000.0, 10.0), (400.0, 300.0));
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SizeConstraints {
    preferred: (f32, f32),
    min: (f32, f32),
    max: (f32, f32),
    aspect: Option<f32>,
}

impl SizeConstraints {
    /// Exactly `width` by `height`, not resizable.
    /// ```
    /// assert!(!syrp_plugin_api::SizeConstraints::fixed(1.0, 1.0).is_resizable());
    /// ```
    #[must_use]
    pub const fn fixed(width: f32, height: f32) -> Self {
        let size = (width, height);
        Self::resizable(size, size, size)
    }

    /// Opens at `width` by `height` and resizes freely between `min` and `max`.
    /// ```
    /// use syrp_plugin_api::SizeConstraints;
    /// let c = SizeConstraints::resizable((600.0, 400.0), (300.0, 200.0), (1200.0, 800.0));
    /// assert_eq!((c.preferred(), c.clamp(100.0, 900.0)), ((600.0, 400.0), (300.0, 800.0)));
    /// ```
    #[must_use]
    pub const fn resizable(preferred: (f32, f32), min: (f32, f32), max: (f32, f32)) -> Self {
        Self {
            preferred,
            min,
            max,
            aspect: None,
        }
    }

    /// Also keeps width / height at `ratio` (the height follows the width).
    /// ```
    /// use syrp_plugin_api::SizeConstraints;
    /// let c = SizeConstraints::resizable((400.0, 200.0), (100.0, 50.0), (800.0, 400.0)).with_aspect_ratio(2.0);
    /// assert_eq!((c.aspect_ratio(), c.clamp(500.0, 100.0)), (Some(2.0), (500.0, 250.0)));
    /// ```
    #[must_use]
    pub const fn with_aspect_ratio(mut self, ratio: f32) -> Self {
        self.aspect = Some(ratio);
        self
    }

    /// The size the editor opens at.
    /// ```
    /// assert_eq!(syrp_plugin_api::SizeConstraints::fixed(2.0, 3.0).preferred(), (2.0, 3.0));
    /// ```
    #[must_use]
    pub const fn preferred(&self) -> (f32, f32) {
        self.preferred
    }

    /// The width-to-height ratio kept while resizing, if any.
    /// ```
    /// assert_eq!(syrp_plugin_api::SizeConstraints::fixed(2.0, 3.0).aspect_ratio(), None);
    /// ```
    #[must_use]
    pub const fn aspect_ratio(&self) -> Option<f32> {
        self.aspect
    }

    /// Whether the host may offer more than one size.
    /// ```
    /// let c = syrp_plugin_api::SizeConstraints::resizable((2.0, 2.0), (1.0, 1.0), (3.0, 3.0));
    /// assert!(c.is_resizable());
    /// ```
    #[must_use]
    pub fn is_resizable(&self) -> bool {
        self.min != self.max
    }

    /// The accepted size nearest to a proposed one: width clamped first, then the height
    /// from the aspect ratio if there is one, then the height clamped.
    /// ```
    /// assert_eq!(syrp_plugin_api::SizeConstraints::fixed(2.0, 3.0).clamp(f32::NAN, 9.0), (2.0, 3.0));
    /// ```
    #[must_use]
    pub fn clamp(&self, width: f32, height: f32) -> (f32, f32) {
        let fit = |v: f32, lo: f32, hi: f32| if v.is_nan() { lo } else { v.max(lo).min(hi) };
        let w = fit(width, self.min.0, self.max.0);
        let h = self.aspect.map_or(height, |ratio| w / ratio);
        (w, fit(h, self.min.1, self.max.1))
    }
}

/// Errors from creating an editor.
/// ```
/// use syrp_error::Diagnostic;
/// assert_eq!(syrp_plugin_api::EditorError::Unsupported.code().as_str(), "SYRP-PLUGIN-0008");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EditorError {
    /// The editor could not build its UI (`SYRP-PLUGIN-0007`).
    CreationFailed,
    /// The host cannot show this editor, for example a headless host (`SYRP-PLUGIN-0008`).
    Unsupported,
}

impl fmt::Display for EditorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            EditorError::CreationFailed => "editor could not build its UI",
            EditorError::Unsupported => "host cannot show this editor",
        })
    }
}

impl std::error::Error for EditorError {}

impl Diagnostic for EditorError {
    fn code(&self) -> ErrorCode {
        ErrorCode::new(match self {
            EditorError::CreationFailed => "SYRP-PLUGIN-0007",
            EditorError::Unsupported => "SYRP-PLUGIN-0008",
        })
    }
}

/// A plugin's editor. Every method runs on the message thread; the editor reaches parameter
/// values only through [`ParamHandles`], never the audio state.
///
/// Lifecycle: [`constraints`](Self::constraints) sizes the window, [`create`](Self::create)
/// builds the UI once, [`resized`](Self::resized) follows host resizes, and
/// [`destroy`](Self::destroy) runs before the host drops the UI.
/// ```
/// use syrp_plugin_api::*;
/// struct Blank;
/// impl Editor for Blank {
///     fn create(&mut self, host: &mut dyn EditorHost, _: &ParamHandles) -> Result<NodeHandle, EditorError> {
///         Ok(host.root())
///     }
///     fn constraints(&self) -> SizeConstraints { SizeConstraints::fixed(320.0, 200.0) }
/// }
/// ```
#[doc(alias = "AudioProcessorEditor")]
pub trait Editor: Send {
    /// Builds the UI under `host` and returns its root node, which must be a core node handle.
    /// ```
    /// # use syrp_plugin_api::*;
    /// # struct H; impl EditorHost for H { fn root(&self) -> NodeHandle { NodeHandle::new(0, 1) } fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self } }
    /// # struct E; impl Editor for E { fn create(&mut self, h: &mut dyn EditorHost, _: &ParamHandles) -> Result<NodeHandle, EditorError> { Ok(h.root()) } fn constraints(&self) -> SizeConstraints { SizeConstraints::fixed(1.0, 1.0) } }
    /// let params = ParamHandles::new(&ParamLayout::new()).unwrap();
    /// assert_eq!(E.create(&mut H, &params), Ok(NodeHandle::new(0, 1)));
    /// ```
    fn create(
        &mut self,
        host: &mut dyn EditorHost,
        params: &ParamHandles,
    ) -> Result<NodeHandle, EditorError>;

    /// The sizes the editor accepts.
    /// ```
    /// # use syrp_plugin_api::*;
    /// # struct E; impl Editor for E { fn create(&mut self, h: &mut dyn EditorHost, _: &ParamHandles) -> Result<NodeHandle, EditorError> { Ok(h.root()) } fn constraints(&self) -> SizeConstraints { SizeConstraints::fixed(9.0, 9.0) } }
    /// assert_eq!(E.constraints().preferred(), (9.0, 9.0));
    /// ```
    fn constraints(&self) -> SizeConstraints;

    /// The host resized the editor to `size`, already clamped by the constraints.
    /// Default: does nothing.
    /// ```
    /// # use syrp_plugin_api::*;
    /// # struct H; impl EditorHost for H { fn root(&self) -> NodeHandle { NodeHandle::new(0, 1) } fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self } }
    /// # struct E; impl Editor for E { fn create(&mut self, h: &mut dyn EditorHost, _: &ParamHandles) -> Result<NodeHandle, EditorError> { Ok(h.root()) } fn constraints(&self) -> SizeConstraints { SizeConstraints::fixed(1.0, 1.0) } }
    /// E.resized(&mut H, (1.0, 1.0));
    /// ```
    fn resized(&mut self, host: &mut dyn EditorHost, size: (f32, f32)) {
        let _ = (host, size);
    }

    /// Releases what `create` built, before the host drops it. Default: does nothing.
    /// ```
    /// # use syrp_plugin_api::*;
    /// # struct H; impl EditorHost for H { fn root(&self) -> NodeHandle { NodeHandle::new(0, 1) } fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self } }
    /// # struct E; impl Editor for E { fn create(&mut self, h: &mut dyn EditorHost, _: &ParamHandles) -> Result<NodeHandle, EditorError> { Ok(h.root()) } fn constraints(&self) -> SizeConstraints { SizeConstraints::fixed(1.0, 1.0) } }
    /// E.destroy(&mut H);
    /// ```
    fn destroy(&mut self, host: &mut dyn EditorHost) {
        let _ = host;
    }
}

/// Receives the edits an editor makes, so the runtime can tell the host (automation gestures).
/// ```
/// struct Log;
/// impl syrp_plugin_api::ParamGestures for Log {
///     fn edited(&self, _: syrp_plugin_api::ParamIndex, _: syrp_plugin_api::GesturePhase, _: f32) {}
/// }
/// ```
pub trait ParamGestures: Send + Sync {
    /// Parameter `index` is in gesture `phase`, now holding the plain `value`.
    /// ```
    /// # struct Log; impl syrp_plugin_api::ParamGestures for Log { fn edited(&self, _: syrp_plugin_api::ParamIndex, _: syrp_plugin_api::GesturePhase, _: f32) {} }
    /// syrp_plugin_api::ParamGestures::edited(&Log, syrp_plugin_api::ParamIndex::new(0), syrp_plugin_api::GesturePhase::Begin, 0.0);
    /// ```
    fn edited(&self, index: ParamIndex, phase: GesturePhase, value: f32);
}

/// Where an edit sits in a user gesture (a drag is `Begin`, any number of `Change`, `End`).
/// ```
/// assert_ne!(syrp_plugin_api::GesturePhase::Begin, syrp_plugin_api::GesturePhase::End);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GesturePhase {
    /// The user grabbed the control.
    Begin,
    /// The value changed.
    Change,
    /// The user let go.
    End,
}

/// The editor's view of the parameters: specs, values and gesture-reporting edits.
///
/// Cheap to clone: it shares the layout and the value array with the processor.
/// ```
/// use syrp_plugin_api::*;
/// let mut layout = ParamLayout::new();
/// layout.register(ParamSpec::new("mix", "Mix", ParamRange::new(0.0, 100.0)).with_unit("%")).unwrap();
/// let params = ParamHandles::new(&layout).unwrap();
/// let mix = params.index_of("mix").unwrap();
/// params.set_normalised(mix, 0.5);
/// assert_eq!((params.get(mix), params.text(mix).as_deref()), (50.0, Some("50.00 %")));
/// ```
#[derive(Clone)]
pub struct ParamHandles {
    layout: Arc<ParamLayout>,
    values: Arc<Params>,
    gestures: Option<Arc<dyn ParamGestures>>,
}

impl ParamHandles {
    /// Handles over a fresh value array for `layout`, holding the defaults.
    /// ```
    /// assert!(syrp_plugin_api::ParamHandles::new(&Default::default()).unwrap().is_empty());
    /// ```
    pub fn new(layout: &ParamLayout) -> Result<Self, crate::PluginError> {
        let values = Arc::new(Params::new(layout)?);
        Ok(Self::shared(Arc::new(layout.clone()), values))
    }

    /// Handles sharing `values`, which the processor reads, built for `layout`.
    /// ```
    /// # use std::sync::Arc; use syrp_plugin_api::*;
    /// let layout = Arc::new(ParamLayout::new());
    /// let values = Arc::new(Params::new(&layout).unwrap());
    /// assert_eq!(ParamHandles::shared(layout, values).len(), 0);
    /// ```
    #[must_use]
    pub fn shared(layout: Arc<ParamLayout>, values: Arc<Params>) -> Self {
        Self {
            layout,
            values,
            gestures: None,
        }
    }

    /// Reports every edit to `gestures`.
    /// ```
    /// # struct Log; impl syrp_plugin_api::ParamGestures for Log { fn edited(&self, _: syrp_plugin_api::ParamIndex, _: syrp_plugin_api::GesturePhase, _: f32) {} }
    /// let p = syrp_plugin_api::ParamHandles::new(&Default::default()).unwrap();
    /// let p = p.with_gestures(std::sync::Arc::new(Log));
    /// ```
    #[must_use]
    pub fn with_gestures(mut self, gestures: Arc<dyn ParamGestures>) -> Self {
        self.gestures = Some(gestures);
        self
    }

    /// The number of parameters.
    /// ```
    /// assert_eq!(syrp_plugin_api::ParamHandles::new(&Default::default()).unwrap().len(), 0);
    /// ```
    #[must_use]
    pub fn len(&self) -> usize {
        self.layout.len()
    }

    /// Whether there are no parameters.
    /// ```
    /// assert!(syrp_plugin_api::ParamHandles::new(&Default::default()).unwrap().is_empty());
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.layout.is_empty()
    }

    /// The index registered for `id`.
    /// ```
    /// assert_eq!(syrp_plugin_api::ParamHandles::new(&Default::default()).unwrap().index_of("x"), None);
    /// ```
    #[must_use]
    pub fn index_of(&self, id: &str) -> Option<ParamIndex> {
        self.layout.index_of(id)
    }

    /// The spec at `index`.
    /// ```
    /// # use syrp_plugin_api::ParamIndex;
    /// assert!(syrp_plugin_api::ParamHandles::new(&Default::default()).unwrap().spec(ParamIndex::new(0)).is_none());
    /// ```
    #[must_use]
    pub fn spec(&self, index: ParamIndex) -> Option<&ParamSpec> {
        self.layout.spec(index)
    }

    /// The current plain value, or `NaN` for an index out of range.
    /// ```
    /// # use syrp_plugin_api::ParamIndex;
    /// assert!(syrp_plugin_api::ParamHandles::new(&Default::default()).unwrap().get(ParamIndex::new(0)).is_nan());
    /// ```
    #[must_use]
    pub fn get(&self, index: ParamIndex) -> f32 {
        self.values.try_get(index).unwrap_or(f32::NAN)
    }

    /// The current value as display text, through the spec's conversion.
    /// ```
    /// # use syrp_plugin_api::ParamIndex;
    /// assert_eq!(syrp_plugin_api::ParamHandles::new(&Default::default()).unwrap().text(ParamIndex::new(0)), None);
    /// ```
    #[must_use]
    pub fn text(&self, index: ParamIndex) -> Option<String> {
        Some(self.spec(index)?.value_to_text(f64::from(self.get(index))))
    }

    /// Starts a gesture on `index` (the user grabbed its control).
    /// ```
    /// # use syrp_plugin_api::ParamIndex;
    /// syrp_plugin_api::ParamHandles::new(&Default::default()).unwrap().begin_gesture(ParamIndex::new(0));
    /// ```
    pub fn begin_gesture(&self, index: ParamIndex) {
        self.report(index, GesturePhase::Begin);
    }

    /// Sets the plain value, snapped and clamped to the range, and reports the change.
    /// Does nothing for an index outside the layout or the value array.
    /// ```
    /// # use syrp_plugin_api::ParamIndex;
    /// syrp_plugin_api::ParamHandles::new(&Default::default()).unwrap().set(ParamIndex::new(0), 1.0);
    /// ```
    pub fn set(&self, index: ParamIndex, value: f64) {
        if let (Some(spec), Some(_)) = (self.spec(index), self.values.try_get(index)) {
            self.values.set(index, spec.range().snap(value) as f32);
            self.report(index, GesturePhase::Change);
        }
    }

    /// Sets the value from a normalised position in `0.0..=1.0`; see [`set`](Self::set).
    /// ```
    /// # use syrp_plugin_api::ParamIndex;
    /// syrp_plugin_api::ParamHandles::new(&Default::default()).unwrap().set_normalised(ParamIndex::new(0), 0.5);
    /// ```
    pub fn set_normalised(&self, index: ParamIndex, normalised: f64) {
        if let Some(spec) = self.spec(index) {
            self.set(index, spec.range().denormalise(normalised));
        }
    }

    /// Ends the gesture on `index`.
    /// ```
    /// # use syrp_plugin_api::ParamIndex;
    /// syrp_plugin_api::ParamHandles::new(&Default::default()).unwrap().end_gesture(ParamIndex::new(0));
    /// ```
    pub fn end_gesture(&self, index: ParamIndex) {
        self.report(index, GesturePhase::End);
    }

    fn report(&self, index: ParamIndex, phase: GesturePhase) {
        if let (Some(g), Some(value)) = (&self.gestures, self.values.try_get(index)) {
            g.edited(index, phase, value);
        }
    }
}

impl fmt::Debug for ParamHandles {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParamHandles")
            .field("layout", &self.layout)
            .field("values", &self.values)
            .field("gestures", &self.gestures.is_some())
            .finish()
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0
//! Parameter registration (`ParamLayout`) and runtime values (`Params`).

use std::sync::atomic::Ordering;

use syrp_rt::AtomicF32;

use crate::{ParamIndex, ParamSpec, PluginError};

/// The registered parameters of one processor, in registration order.
///
/// [`register`](Self::register) assigns each spec the next dense [`ParamIndex`]. String ids
/// are resolved here, on the message thread or at prepare time, never in `process`.
/// ```
/// use syrp_plugin_api::{ParamIndex, ParamLayout, ParamRange, ParamSpec};
/// let mut layout = ParamLayout::new();
/// let a = layout.register(ParamSpec::new("a", "A", ParamRange::new(0.0, 1.0))).unwrap();
/// let b = layout.register(ParamSpec::new("b", "B", ParamRange::new(0.0, 1.0))).unwrap();
/// assert_eq!((a, b, layout.index_of("b")), (ParamIndex::new(0), ParamIndex::new(1), Some(b)));
/// ```
#[derive(Debug, Clone, Default)]
pub struct ParamLayout {
    specs: Vec<ParamSpec>,
    /// Indices into `specs`, sorted by id, for binary-search lookup.
    by_id: Vec<u32>,
}

impl ParamLayout {
    /// An empty layout.
    /// ```
    /// assert!(syrp_plugin_api::ParamLayout::new().is_empty());
    /// ```
    #[must_use]
    pub const fn new() -> Self {
        Self {
            specs: Vec::new(),
            by_id: Vec::new(),
        }
    }

    /// Registers `spec` and returns its index.
    ///
    /// Fails, leaving the layout unchanged, with [`PluginError::DuplicateParamId`] if the id
    /// is taken, or [`PluginError::InvalidParamSpec`] if the id is empty, the range invalid
    /// ([`ParamRange::is_valid`](crate::ParamRange::is_valid)) or the default out of range.
    /// ```
    /// use syrp_plugin_api::{ParamLayout, ParamRange, ParamSpec, PluginError};
    /// let mut layout = ParamLayout::new();
    /// let spec = ParamSpec::new("a", "A", ParamRange::new(0.0, 1.0));
    /// layout.register(spec.clone()).unwrap();
    /// assert_eq!(layout.register(spec), Err(PluginError::DuplicateParamId));
    /// ```
    #[doc(alias = "addParameter")]
    pub fn register(&mut self, spec: ParamSpec) -> Result<ParamIndex, PluginError> {
        if !spec.is_valid() {
            return Err(PluginError::InvalidParamSpec);
        }
        let Err(at) = self.search(spec.id()) else {
            return Err(PluginError::DuplicateParamId);
        };
        let index = u32::try_from(self.specs.len()).map_err(|_| PluginError::AllocationFailed)?;
        self.specs.push(spec);
        self.by_id.insert(at, index);
        Ok(ParamIndex::new(index))
    }

    /// The index registered for `id`. Message thread or prepare time only.
    /// ```
    /// assert_eq!(syrp_plugin_api::ParamLayout::new().index_of("gain"), None);
    /// ```
    #[must_use]
    pub fn index_of(&self, id: &str) -> Option<ParamIndex> {
        let at = self.search(id).ok()?;
        Some(ParamIndex::new(self.by_id[at]))
    }

    /// The spec at `index`.
    /// ```
    /// # use syrp_plugin_api::{ParamLayout, ParamRange, ParamSpec};
    /// let mut layout = ParamLayout::new();
    /// let i = layout.register(ParamSpec::new("a", "A", ParamRange::new(0.0, 1.0))).unwrap();
    /// assert_eq!(layout.spec(i).map(ParamSpec::id), Some("a"));
    /// ```
    #[must_use]
    pub fn spec(&self, index: ParamIndex) -> Option<&ParamSpec> {
        self.specs.get(index.as_usize())
    }

    /// Every spec in index order: position `i` holds the spec with `ParamIndex::new(i)`.
    /// ```
    /// assert!(syrp_plugin_api::ParamLayout::new().specs().is_empty());
    /// ```
    #[must_use]
    pub fn specs(&self) -> &[ParamSpec] {
        &self.specs
    }

    /// The number of registered parameters.
    /// ```
    /// assert_eq!(syrp_plugin_api::ParamLayout::new().len(), 0);
    /// ```
    #[must_use]
    pub fn len(&self) -> usize {
        self.specs.len()
    }

    /// Whether no parameter is registered.
    /// ```
    /// assert!(syrp_plugin_api::ParamLayout::default().is_empty());
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.specs.is_empty()
    }

    fn search(&self, id: &str) -> Result<usize, usize> {
        self.by_id
            .binary_search_by(|&i| self.specs[i as usize].id().cmp(id))
    }
}

/// Runtime parameter values: one contiguous array of atomics indexed by [`ParamIndex`].
///
/// Values are plain (not normalised) `f32`. Any thread may read or write them with one
/// relaxed atomic operation each, which is RT-safe. There is no lookup by string id.
/// ```
/// use syrp_plugin_api::{ParamLayout, ParamRange, ParamSpec, Params};
/// let mut layout = ParamLayout::new();
/// let g = layout.register(ParamSpec::new("g", "G", ParamRange::new(0.0, 1.0))).unwrap();
/// let params = Params::new(&layout).unwrap();
/// params.set(g, 0.5);
/// assert_eq!(params.get(g), 0.5);
/// ```
#[derive(Debug)]
pub struct Params {
    values: Box<[AtomicF32]>,
}

impl Params {
    /// Allocates one atomic per parameter in `layout`, each holding its default value.
    /// Fails with [`PluginError::AllocationFailed`] if the memory cannot be reserved.
    /// ```
    /// let p = syrp_plugin_api::Params::new(&syrp_plugin_api::ParamLayout::new()).unwrap();
    /// assert!(p.is_empty());
    /// ```
    pub fn new(layout: &ParamLayout) -> Result<Self, PluginError> {
        let mut values = Vec::new();
        values
            .try_reserve_exact(layout.len())
            .map_err(|_| PluginError::AllocationFailed)?;
        let defaults = layout.specs().iter().map(ParamSpec::default_value);
        values.extend(defaults.map(|d| AtomicF32::new(d as f32)));
        Ok(Self {
            values: values.into_boxed_slice(),
        })
    }

    /// The value at `index`. RT-safe.
    ///
    /// # Panics
    /// If `index` is not below [`len`](Self::len), which means it came from another layout.
    /// ```
    /// # use syrp_plugin_api::{ParamLayout, ParamRange, ParamSpec, Params};
    /// # let mut layout = ParamLayout::new();
    /// let i = layout.register(ParamSpec::new("x", "X", ParamRange::new(0.0, 1.0)).with_default(0.25)).unwrap();
    /// assert_eq!(Params::new(&layout).unwrap().get(i), 0.25);
    /// ```
    #[must_use]
    pub fn get(&self, index: ParamIndex) -> f32 {
        self.values[index.as_usize()].load(Ordering::Relaxed)
    }

    /// The value at `index`, or `None` when it is out of range. RT-safe.
    /// ```
    /// # use syrp_plugin_api::{ParamIndex, ParamLayout, Params};
    /// assert_eq!(Params::new(&ParamLayout::new()).unwrap().try_get(ParamIndex::new(0)), None);
    /// ```
    #[must_use]
    pub fn try_get(&self, index: ParamIndex) -> Option<f32> {
        let value = self.values.get(index.as_usize())?;
        Some(value.load(Ordering::Relaxed))
    }

    /// Stores `value` at `index`. RT-safe. The caller keeps values inside the range.
    ///
    /// # Panics
    /// If `index` is not below [`len`](Self::len).
    /// ```
    /// # use syrp_plugin_api::{ParamLayout, ParamRange, ParamSpec, Params};
    /// # let mut layout = ParamLayout::new();
    /// let i = layout.register(ParamSpec::new("x", "X", ParamRange::new(0.0, 1.0))).unwrap();
    /// let p = Params::new(&layout).unwrap();
    /// p.set(i, 1.0);
    /// assert_eq!(p.get(i), 1.0);
    /// ```
    pub fn set(&self, index: ParamIndex, value: f32) {
        self.values[index.as_usize()].store(value, Ordering::Relaxed);
    }

    /// The number of parameters.
    /// ```
    /// assert_eq!(syrp_plugin_api::Params::new(&Default::default()).unwrap().len(), 0);
    /// ```
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Whether there are no parameters.
    /// ```
    /// assert!(syrp_plugin_api::Params::new(&Default::default()).unwrap().is_empty());
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

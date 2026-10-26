// SPDX-License-Identifier: MIT OR Apache-2.0
//! The per-block parameter change stream.

use std::ops::Range;

use syrp_rt::RtSafe;

use crate::{ParamIndex, PluginError};

/// One parameter change inside a block: sample offset, parameter index and new plain value.
/// ```
/// use syrp_plugin_api::{ParamChange, ParamIndex};
/// let c = ParamChange::new(16, ParamIndex::new(2), 0.5);
/// assert_eq!((c.sample_offset(), c.index(), c.value()), (16, ParamIndex::new(2), 0.5));
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParamChange {
    sample_offset: u32,
    index: ParamIndex,
    value: f32,
}

impl ParamChange {
    /// A change of parameter `index` to `value` at `sample_offset` frames into the block.
    /// ```
    /// let c = syrp_plugin_api::ParamChange::new(0, syrp_plugin_api::ParamIndex::new(0), 1.0);
    /// assert_eq!(c.value(), 1.0);
    /// ```
    #[must_use]
    pub const fn new(sample_offset: u32, index: ParamIndex, value: f32) -> Self {
        Self {
            sample_offset,
            index,
            value,
        }
    }

    /// Frames from the start of the block.
    /// ```
    /// assert_eq!(syrp_plugin_api::ParamChange::new(9, syrp_plugin_api::ParamIndex::new(0), 0.0).sample_offset(), 9);
    /// ```
    #[must_use]
    pub const fn sample_offset(self) -> u32 {
        self.sample_offset
    }

    /// The parameter that changes.
    /// ```
    /// use syrp_plugin_api::{ParamChange, ParamIndex};
    /// assert_eq!(ParamChange::new(0, ParamIndex::new(4), 0.0).index(), ParamIndex::new(4));
    /// ```
    #[must_use]
    pub const fn index(self) -> ParamIndex {
        self.index
    }

    /// The new plain value.
    /// ```
    /// assert_eq!(syrp_plugin_api::ParamChange::new(0, syrp_plugin_api::ParamIndex::new(0), 0.3).value(), 0.3);
    /// ```
    #[must_use]
    pub const fn value(self) -> f32 {
        self.value
    }
}

// SAFETY: plain `Copy` data (`u32`, `ParamIndex`, `f32`) with no destructor.
unsafe impl RtSafe for ParamChange {}

/// One block's parameter changes, sorted by sample offset, equal offsets in push order.
///
/// The capacity is fixed when the stream is created at prepare time;
/// [`try_push`](Self::try_push) and [`clear`](Self::clear) never allocate.
/// ```
/// use syrp_plugin_api::{ParamChange, ParamChanges, ParamIndex};
/// let mut changes = ParamChanges::with_capacity(4).unwrap();
/// changes.try_push(ParamChange::new(32, ParamIndex::new(0), 1.0)).unwrap();
/// changes.try_push(ParamChange::new(8, ParamIndex::new(1), 0.5)).unwrap();
/// let offsets: Vec<u32> = changes.iter().map(|c| c.sample_offset()).collect();
/// assert_eq!(offsets, [8, 32]);
/// ```
#[derive(Debug, Clone, Default)]
pub struct ParamChanges {
    changes: Vec<ParamChange>,
}

impl ParamChanges {
    /// An empty stream with capacity 0 (every push fails).
    /// ```
    /// assert_eq!(syrp_plugin_api::ParamChanges::new().capacity(), 0);
    /// ```
    #[must_use]
    pub const fn new() -> Self {
        Self {
            changes: Vec::new(),
        }
    }

    /// An empty stream that holds up to `capacity` changes.
    /// Fails with [`PluginError::AllocationFailed`] if the memory cannot be reserved.
    /// ```
    /// assert_eq!(syrp_plugin_api::ParamChanges::with_capacity(64).unwrap().capacity(), 64);
    /// ```
    pub fn with_capacity(capacity: usize) -> Result<Self, PluginError> {
        let mut changes = Vec::new();
        changes
            .try_reserve_exact(capacity)
            .map_err(|_| PluginError::AllocationFailed)?;
        Ok(Self { changes })
    }

    /// Inserts `change` after every change at or before its offset. RT-safe: never allocates.
    /// Fails with [`PluginError::ParamChangesFull`] at capacity, leaving the stream unchanged.
    /// ```
    /// use syrp_plugin_api::{ParamChange, ParamChanges, ParamIndex, PluginError};
    /// let mut c = ParamChanges::with_capacity(1).unwrap();
    /// c.try_push(ParamChange::new(0, ParamIndex::new(0), 0.0)).unwrap();
    /// assert_eq!(c.try_push(ParamChange::new(0, ParamIndex::new(0), 1.0)), Err(PluginError::ParamChangesFull));
    /// ```
    pub fn try_push(&mut self, change: ParamChange) -> Result<(), PluginError> {
        if self.changes.len() == self.changes.capacity() {
            return Err(PluginError::ParamChangesFull);
        }
        let at = self
            .changes
            .partition_point(|c| c.sample_offset <= change.sample_offset);
        self.changes.insert(at, change);
        Ok(())
    }

    /// Removes every change, keeping the capacity. RT-safe.
    /// ```
    /// use syrp_plugin_api::{ParamChange, ParamChanges, ParamIndex};
    /// let mut c = ParamChanges::with_capacity(1).unwrap();
    /// c.try_push(ParamChange::new(0, ParamIndex::new(0), 0.0)).unwrap();
    /// c.clear();
    /// assert!(c.is_empty() && c.capacity() == 1);
    /// ```
    pub fn clear(&mut self) {
        self.changes.clear();
    }

    /// Iterates the changes in order, by value.
    /// ```
    /// assert_eq!(syrp_plugin_api::ParamChanges::new().iter().count(), 0);
    /// ```
    pub fn iter(&self) -> std::iter::Copied<std::slice::Iter<'_, ParamChange>> {
        self.changes.iter().copied()
    }

    /// The changes whose sample offsets lie in `range`.
    /// ```
    /// use syrp_plugin_api::{ParamChange, ParamChanges, ParamIndex};
    /// let mut c = ParamChanges::with_capacity(3).unwrap();
    /// for t in [0, 10, 20] { c.try_push(ParamChange::new(t, ParamIndex::new(0), 0.0)).unwrap(); }
    /// assert_eq!(c.range(5..20).len(), 1);
    /// ```
    #[must_use]
    pub fn range(&self, range: Range<u32>) -> &[ParamChange] {
        let lo = self
            .changes
            .partition_point(|c| c.sample_offset < range.start);
        let hi = self
            .changes
            .partition_point(|c| c.sample_offset < range.end);
        &self.changes[lo..hi.max(lo)]
    }

    /// The number of changes.
    /// ```
    /// assert_eq!(syrp_plugin_api::ParamChanges::new().len(), 0);
    /// ```
    #[must_use]
    pub fn len(&self) -> usize {
        self.changes.len()
    }

    /// Whether there are no changes.
    /// ```
    /// assert!(syrp_plugin_api::ParamChanges::new().is_empty());
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// How many changes fit without allocating.
    /// ```
    /// assert_eq!(syrp_plugin_api::ParamChanges::with_capacity(2).unwrap().capacity(), 2);
    /// ```
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.changes.capacity()
    }
}

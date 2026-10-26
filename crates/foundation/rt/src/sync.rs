// SPDX-License-Identifier: MIT OR Apache-2.0
//! Synchronisation shim: `std` normally, `loom` under `--cfg loom` so the lock-free code is
//! model-checked as written.

#[cfg(loom)]
pub(crate) use loom::{
    cell::UnsafeCell,
    sync::Arc,
    sync::atomic::{AtomicPtr, AtomicUsize},
};
#[cfg(not(loom))]
pub(crate) use std::sync::{
    Arc,
    atomic::{AtomicPtr, AtomicUsize},
};

/// `core::cell::UnsafeCell` with loom's closure-based access API.
#[cfg(not(loom))]
#[derive(Debug)]
pub(crate) struct UnsafeCell<T>(core::cell::UnsafeCell<T>);

#[cfg(not(loom))]
impl<T> UnsafeCell<T> {
    pub(crate) fn new(value: T) -> Self {
        Self(core::cell::UnsafeCell::new(value))
    }

    /// Calls `f` with a raw pointer to the contents. The caller upholds aliasing rules.
    pub(crate) fn with_mut<R>(&self, f: impl FnOnce(*mut T) -> R) -> R {
        f(self.0.get())
    }
}

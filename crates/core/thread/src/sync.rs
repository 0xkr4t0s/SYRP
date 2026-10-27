// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Synchronisation shim: `std` normally, `loom` under `--cfg loom`, so the loom models in
//! `tests/loom.rs` check the code that ships.

#[cfg(loom)]
pub(crate) use loom::{
    cell::UnsafeCell,
    hint::spin_loop,
    sync::atomic::AtomicBool,
    sync::{Condvar, Mutex},
    thread::yield_now,
};
#[cfg(not(loom))]
pub(crate) use std::{
    hint::spin_loop,
    sync::atomic::AtomicBool,
    sync::{Condvar, Mutex},
    thread::yield_now,
};

/// `core::cell::UnsafeCell` with loom's closure-based access API.
#[cfg(not(loom))]
#[derive(Debug)]
pub(crate) struct UnsafeCell<T>(core::cell::UnsafeCell<T>);

#[cfg(not(loom))]
impl<T> UnsafeCell<T> {
    pub(crate) const fn new(value: T) -> Self {
        Self(core::cell::UnsafeCell::new(value))
    }

    /// Calls `f` with a raw pointer to the contents. The caller upholds aliasing rules.
    pub(crate) fn with_mut<R>(&self, f: impl FnOnce(*mut T) -> R) -> R {
        f(self.0.get())
    }

    pub(crate) fn into_inner(self) -> T {
        self.0.into_inner()
    }
}

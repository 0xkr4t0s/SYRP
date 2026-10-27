// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Owned desktop message loops for Syrp (master plan §5.5, kind 1).
//!
//! [`EventLoop`] is the loop an app runs on its main thread. It implements
//! [`syrp_loop_api::MessageLoop`], so code written against the contract (and tested on
//! `ManualLoop`) runs on it unchanged, and adds what only an owned loop has: [`run`],
//! [`run_for`], [`dispatch_pending`], [`with_token`] and a [`QuitHandle`]. On Linux it is an
//! `epoll` loop, and peers add their file descriptors with `EventLoop::add_fd`.
//!
//! [`run`]: EventLoop::run
//! [`run_for`]: EventLoop::run_for
//! [`dispatch_pending`]: EventLoop::dispatch_pending
//! [`with_token`]: EventLoop::with_token
//! ```
//! use std::time::Duration;
//! use syrp_events::EventLoop;
//! use syrp_loop_api::MessageLoop;
//! let lp = EventLoop::new()?;
//! let quit = lp.quit_handle();
//! let _t = lp.timer(Duration::from_millis(10), Box::new(move |_mt| quit.quit()));
//! lp.run();
//! # Ok::<(), syrp_events::EventsError>(())
//! ```

mod error;
mod event_loop;
mod os;

pub use error::EventsError;
pub use event_loop::{EventLoop, QuitHandle};
#[cfg(target_os = "linux")]
pub use os::epoll::{FdFn, FdGuard, Interest, Readiness};

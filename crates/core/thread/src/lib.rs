// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The Syrp threading layer (master plan §5.2, L2 Core).
//!
//! - [`Thread::builder`] spawns a named thread whose body gets a [`ThreadContext`] to poll
//!   [`ThreadContext::thread_should_exit`] and to [`wait`](ThreadContext::wait) for
//!   [`ThreadHandle::notify`]. [`ThreadHandle::stop`] signals and waits with a timeout; a
//!   thread that does not exit in time is detached, never killed (`SYRP-THREAD-0001`).
//!
//! ```
//! use std::time::Duration;
//! use syrp_thread::{Stopped, Thread};
//! let worker = Thread::builder("worker")
//!     .spawn(|ctx| {
//!         while !ctx.thread_should_exit() {
//!             ctx.wait(Some(Duration::from_millis(50)));
//!         }
//!     })
//!     .unwrap();
//! assert_eq!(worker.stop(Duration::from_secs(5)), Ok(Stopped::Finished));
//! ```

mod error;
mod thread;

pub use error::ThreadError;
pub use syrp_error::Diagnostic;
pub use thread::{
    DEFAULT_DROP_TIMEOUT, Stopped, Thread, ThreadBuilder, ThreadContext, ThreadHandle,
    current_thread_id, current_thread_name, sleep, yield_now,
};

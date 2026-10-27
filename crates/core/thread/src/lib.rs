// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The Syrp threading layer (master plan §5.2, L2 Core).
//!
//! - [`Thread::builder`] spawns a named thread whose body gets a [`ThreadContext`] to poll
//!   [`ThreadContext::thread_should_exit`] and to [`wait`](ThreadContext::wait) for
//!   [`ThreadHandle::notify`]. [`ThreadHandle::stop`] signals and waits with a timeout; a
//!   thread that does not exit in time is detached, never killed (`SYRP-THREAD-0001`).
//! - [`Priority`] and [`RealtimeOptions`] map onto each OS's scheduler; see [`priority`] for
//!   the table. Refusals are reported through [`SchedulingStatus`] and never stop the thread.
//! - [`WaitableEvent`] is an auto- or manual-reset signal to wait on, and [`SpinLock`] a lock
//!   for very short sections. JUCE's other locks map to `std` (see `docs/migration/locks.md`).
//!
//! ```
//! use std::time::Duration;
//! use syrp_thread::{Priority, Stopped, Thread};
//! let worker = Thread::builder("worker")
//!     .priority(Priority::Low)
//!     .spawn(|ctx| {
//!         while !ctx.thread_should_exit() {
//!             ctx.wait(Some(Duration::from_millis(50)));
//!         }
//!     })
//!     .unwrap();
//! assert_eq!(worker.stop(Duration::from_secs(5)), Ok(Stopped::Finished));
//! ```

mod error;
mod event;
mod os;
pub mod priority;
mod spin;
mod sync;
mod thread;

pub use error::ThreadError;
pub use event::WaitableEvent;
pub use priority::{
    MAX_REALTIME_PRIORITY, Priority, RealtimeOptions, SchedulingStatus,
    set_current_thread_affinity_mask, set_current_thread_priority,
};
pub use spin::{SpinLock, SpinLockGuard};
pub use syrp_error::Diagnostic;
pub use thread::{
    DEFAULT_DROP_TIMEOUT, Stopped, Thread, ThreadBuilder, ThreadContext, ThreadHandle,
    current_thread_id, current_thread_name, sleep, yield_now,
};

// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The clock-free core of the threaded destination: the shared queue, the batch scheduler
//! and the step function the analytics thread runs. Tests drive it with a manual clock.

use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

use super::BatchSink;
use crate::analytics::panic_message;
use crate::event::AnalyticsEvent;

/// Longest period the back-off grows to after failed batches, in milliseconds (Syrp's
/// choice: the JUCE documentation leaves back-off to the subclass).
pub(crate) const MAX_BACKOFF_PERIOD_MS: u64 = 60_000;

/// State shared by the destination and its thread.
#[derive(Debug, Default)]
pub(crate) struct Shared {
    state: Mutex<State>,
    pub(crate) wake: Condvar,
}

#[derive(Debug, Default)]
pub(crate) struct State {
    pub(crate) queue: VecDeque<AnalyticsEvent>,
    pub(crate) capacity: usize,
    /// Events refused because the queue was full, since the last stop.
    pub(crate) dropped: u64,
    /// Last generation handed out by [`Shared::begin`]; 0 means none yet.
    last_generation: u64,
    /// The generation that should keep running; 0 when stopping or stopped.
    pub(crate) active: u64,
    /// The generation allowed to remove delivered events from the queue; 0 after a stop
    /// has taken the queue.
    owner: u64,
    /// Highest generation whose thread has exited.
    pub(crate) finished: u64,
    /// The base batch period in milliseconds and a counter bumped whenever it changes.
    pub(crate) base_period_ms: u64,
    pub(crate) period_epoch: u64,
}

impl Shared {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            state: Mutex::new(State {
                capacity,
                ..State::default()
            }),
            wake: Condvar::new(),
        }
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Queues one event, or counts it as dropped when the queue is full.
    pub(crate) fn push(&self, event: &AnalyticsEvent) {
        let mut st = self.lock();
        if st.queue.len() < st.capacity {
            st.queue.push_back(event.clone());
        } else {
            st.dropped = st.dropped.saturating_add(1);
        }
    }

    /// Starts a new generation: restores the sink's saved events ahead of the queue (they
    /// are older) and makes the generation active. Returns the generation.
    pub(crate) fn begin<S: BatchSink + ?Sized>(&self, sink: &S, base_period_ms: u64) -> u64 {
        let mut restored = Vec::new();
        sink.restore_unlogged_events(&mut restored);
        let mut st = self.lock();
        for event in restored.into_iter().rev() {
            st.queue.push_front(event);
        }
        st.last_generation += 1;
        let generation = st.last_generation;
        st.active = generation;
        st.owner = generation;
        st.base_period_ms = base_period_ms;
        st.period_epoch += 1;
        generation
    }

    /// Asks the running generation to stop and wakes its thread.
    pub(crate) fn request_stop(&self) {
        self.lock().active = 0;
        self.wake.notify_all();
    }

    /// Marks `generation`'s thread as exited and wakes anyone waiting for it.
    pub(crate) fn mark_finished(&self, generation: u64) {
        let mut st = self.lock();
        st.finished = st.finished.max(generation);
        drop(st);
        self.wake.notify_all();
    }

    /// Takes every queued event and the dropped count, and stops any thread from removing
    /// events from the queue afterwards.
    pub(crate) fn take_unlogged(&self) -> (Vec<AnalyticsEvent>, u64) {
        let mut st = self.lock();
        st.active = 0;
        st.owner = 0;
        let events = std::mem::take(&mut st.queue).into();
        (events, std::mem::take(&mut st.dropped))
    }
}

/// When the next batch is due and how long the period currently is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Batcher {
    base_ms: u64,
    period_ms: u64,
    next_due_ms: u64,
    epoch: u64,
}

impl Batcher {
    /// A scheduler whose first batch is due one base period after `now_ms`.
    pub(crate) fn new(base_ms: u64, epoch: u64, now_ms: u64) -> Self {
        Self {
            base_ms,
            period_ms: base_ms,
            next_due_ms: now_ms.saturating_add(base_ms),
            epoch,
        }
    }

    #[cfg(test)]
    pub(crate) fn period_ms(&self) -> u64 {
        self.period_ms
    }

    pub(crate) fn epoch(&self) -> u64 {
        self.epoch
    }

    fn on_success(&mut self) {
        self.period_ms = self.base_ms;
    }

    /// Doubles the period, up to the cap (or the base period, if that is longer).
    fn on_failure(&mut self) {
        let cap = MAX_BACKOFF_PERIOD_MS.max(self.base_ms);
        self.period_ms = self.period_ms.saturating_mul(2).min(cap);
    }
}

/// What the thread does after a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    /// Wait until this time (milliseconds on the thread's clock), then step again.
    WaitUntil(u64),
    /// This generation is no longer active: exit.
    Stop,
}

/// One scheduling step at time `now_ms`. If a batch is due and events are queued, sends up
/// to `maximum_batch_size` of them (at least one) from the front of the queue. They leave the
/// queue only when the sink reports success; otherwise they stay first in line and the
/// period doubles. A panic in the sink counts as a failure and is logged.
pub(crate) fn step<S: BatchSink + ?Sized>(
    shared: &Shared,
    sink: &S,
    batcher: &mut Batcher,
    generation: u64,
    now_ms: u64,
) -> Step {
    let batch: Vec<AnalyticsEvent> = {
        let st = shared.lock();
        if st.active != generation {
            return Step::Stop;
        }
        if st.period_epoch != batcher.epoch {
            *batcher = Batcher::new(st.base_period_ms, st.period_epoch, now_ms);
        }
        if now_ms < batcher.next_due_ms {
            return Step::WaitUntil(batcher.next_due_ms);
        }
        if st.queue.is_empty() {
            batcher.next_due_ms = now_ms.saturating_add(batcher.period_ms);
            return Step::WaitUntil(batcher.next_due_ms);
        }
        let size = sink.maximum_batch_size().max(1);
        st.queue.iter().take(size).cloned().collect()
    };

    let delivered = match catch_unwind(AssertUnwindSafe(|| sink.log_batched_events(&batch))) {
        Ok(delivered) => delivered,
        Err(payload) => {
            syrp_sys::write_to_log(&format!(
                "syrp-analytics: log_batched_events panicked: {}",
                panic_message(payload.as_ref())
            ));
            false
        }
    };

    let mut st = shared.lock();
    if delivered {
        if st.owner == generation {
            // Only this generation removes events, and only from the front, so the first
            // `batch.len()` entries are still the ones just sent.
            st.queue.drain(..batch.len());
        }
        batcher.on_success();
    } else {
        batcher.on_failure();
    }
    batcher.next_due_ms = now_ms.saturating_add(batcher.period_ms);
    if st.active == generation {
        Step::WaitUntil(batcher.next_due_ms)
    } else {
        Step::Stop
    }
}

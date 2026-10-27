// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Targets without an owned loop (iOS, Android, the web): `EventLoop::new` fails with
//! `UnsupportedTarget`, so the poller is never made.

use std::rc::Rc;
use std::time::Duration;

use syrp_loop_api::MessageThread;

use crate::EventsError;

pub(crate) struct Waker;

impl Waker {
    pub(crate) fn new() -> Result<Self, EventsError> {
        Err(EventsError::UnsupportedTarget)
    }

    pub(crate) fn wake(&self) {}
}

pub(crate) struct Poller;

impl Poller {
    pub(crate) fn new(_waker: &Waker) -> Result<Self, EventsError> {
        Err(EventsError::UnsupportedTarget)
    }

    pub(crate) fn attach(&self, _dispatch: Rc<dyn Fn() -> usize>) {}

    pub(crate) fn arm(&self, _next: Option<Duration>) {}

    pub(crate) fn wait(&self, _: &Waker, _: Option<Duration>, _: &MessageThread<'_>) -> usize {
        0
    }
}

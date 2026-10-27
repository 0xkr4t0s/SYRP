// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Fallback for targets without a backend: every request is refused.

use crate::{Priority, RealtimeOptions, ThreadError};

/// Never constructed.
pub(crate) struct RealtimeToken;

pub(crate) fn set_priority(_: Priority) -> Result<(), Option<i32>> {
    Err(None)
}

pub(crate) fn enter_realtime(_: &RealtimeOptions) -> Result<RealtimeToken, Option<i32>> {
    Err(None)
}

pub(crate) fn set_affinity(_: u64) -> Result<(), ThreadError> {
    Err(ThreadError::AffinityUnsupported)
}

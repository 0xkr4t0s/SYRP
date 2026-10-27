// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Linux and Android: nice values, `SCHED_FIFO` and `sched_setaffinity`.

use super::last_error;
use crate::{Priority, RealtimeOptions, ThreadError};

/// Nothing to undo when a realtime thread ends on Linux.
pub(crate) struct RealtimeToken;

pub(crate) fn set_priority(priority: Priority) -> Result<(), Option<i32>> {
    let nice = match priority {
        Priority::Lowest => 15,
        Priority::Low => 5,
        Priority::Normal => 0,
        Priority::High => -5,
        Priority::Highest => -10,
    };
    // SAFETY: gettid takes no arguments, has no preconditions and cannot fail.
    let tid = unsafe { libc::gettid() };
    // SAFETY: setpriority takes only integers. On Linux, PRIO_PROCESS with a thread id
    // changes the nice value of that one thread, which here is the calling thread.
    let r = unsafe { libc::setpriority(libc::PRIO_PROCESS as _, tid as libc::id_t, nice) };
    if r == 0 { Ok(()) } else { Err(last_error()) }
}

pub(crate) fn enter_realtime(opts: &RealtimeOptions) -> Result<RealtimeToken, Option<i32>> {
    let policy = libc::SCHED_FIFO;
    // SAFETY: both calls take an integer policy and have no other preconditions.
    let (min, max) = unsafe {
        (
            libc::sched_get_priority_min(policy),
            libc::sched_get_priority_max(policy),
        )
    };
    if min < 0 || max < min {
        return Err(last_error());
    }
    let top = min + (max - min) * 4 / 5;
    // SAFETY: sched_param is plain integer data, for which all-zero bytes are valid.
    let mut param: libc::sched_param = unsafe { std::mem::zeroed() };
    param.sched_priority = min + (top - min) * i32::from(opts.priority()) / 10;
    // SAFETY: pthread_self() names the calling thread, which is alive for the whole call, and
    // `param` is a valid, initialised sched_param that outlives the call.
    let r = unsafe { libc::pthread_setschedparam(libc::pthread_self(), policy, &param) };
    if r == 0 {
        Ok(RealtimeToken)
    } else {
        Err(Some(r))
    }
}

pub(crate) fn set_affinity(mask: u64) -> Result<(), ThreadError> {
    // SAFETY: cpu_set_t is a plain bit array; all-zero bytes are the empty set.
    let mut set: libc::cpu_set_t = unsafe { std::mem::zeroed() };
    for cpu in (0..64).filter(|cpu| mask & (1 << cpu) != 0) {
        // SAFETY: `cpu` is below 64, well inside the set's CPU_SETSIZE bits, and `set` is a
        // valid, initialised cpu_set_t.
        unsafe { libc::CPU_SET(cpu, &mut set) };
    }
    // SAFETY: pid 0 selects the calling thread; the size is that of `set`, which is a valid,
    // initialised cpu_set_t that outlives the call.
    let r = unsafe { libc::sched_setaffinity(0, size_of::<libc::cpu_set_t>(), &set) };
    if r == 0 {
        Ok(())
    } else {
        Err(ThreadError::AffinityRefused {
            os_error: last_error(),
        })
    }
}

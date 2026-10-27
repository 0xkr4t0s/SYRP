// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! macOS and iOS: QoS classes and the Mach time-constraint policy. No thread affinity.

use std::ffi::c_int;
use std::time::Duration;

use crate::{Priority, RealtimeOptions, ThreadError};

/// Nothing to undo when a realtime thread ends on Apple platforms.
pub(crate) struct RealtimeToken;

/// `mach_timebase_info_data_t`: Mach absolute time units are `numer / denom` nanoseconds.
#[repr(C)]
struct Timebase {
    numer: u32,
    denom: u32,
}

// QoS class values from <sys/qos.h>.
const QOS_USER_INTERACTIVE: u32 = 0x21;
const QOS_USER_INITIATED: u32 = 0x19;
const QOS_DEFAULT: u32 = 0x15;
const QOS_UTILITY: u32 = 0x11;
const QOS_BACKGROUND: u32 = 0x09;

// SAFETY: these declarations match the C prototypes in <pthread/qos.h> and
// <mach/mach_time.h> (`qos_class_t` is an unsigned int enum).
unsafe extern "C" {
    fn pthread_set_qos_class_self_np(qos_class: u32, relative_priority: c_int) -> c_int;
    fn mach_timebase_info(info: *mut Timebase) -> c_int;
}

pub(crate) fn set_priority(priority: Priority) -> Result<(), Option<i32>> {
    let class = match priority {
        Priority::Lowest => QOS_BACKGROUND,
        Priority::Low => QOS_UTILITY,
        Priority::Normal => QOS_DEFAULT,
        Priority::High => QOS_USER_INITIATED,
        Priority::Highest => QOS_USER_INTERACTIVE,
    };
    // SAFETY: takes a valid QoS class constant and a relative priority of 0 (the allowed
    // range is -15..=0); it only changes the calling thread.
    let r = unsafe { pthread_set_qos_class_self_np(class, 0) };
    if r == 0 { Ok(()) } else { Err(Some(r)) }
}

pub(crate) fn enter_realtime(opts: &RealtimeOptions) -> Result<RealtimeToken, Option<i32>> {
    let mut tb = Timebase { numer: 0, denom: 0 };
    // SAFETY: `tb` is a valid, aligned, writable Timebase that outlives the call.
    if unsafe { mach_timebase_info(&mut tb) } != 0 || tb.numer == 0 {
        return Err(None);
    }
    let abs = |d: Duration| {
        let ticks = d.as_nanos() * u128::from(tb.denom) / u128::from(tb.numer);
        u32::try_from(ticks).unwrap_or(u32::MAX)
    };
    let period = opts.processing_period().unwrap_or(Duration::ZERO);
    let work = opts
        .expected_work_time()
        .or(opts.processing_period().map(|p| p / 2))
        .unwrap_or(Duration::from_millis(5))
        .clamp(Duration::from_micros(50), Duration::from_millis(50));
    let constraint = if period.is_zero() {
        work * 2
    } else {
        period.max(work)
    };
    let mut policy = libc::thread_time_constraint_policy {
        period: abs(period),
        computation: abs(work),
        constraint: abs(constraint),
        preemptible: 1,
    };
    // SAFETY: pthread_self() names the calling thread and pthread_mach_thread_np returns its
    // Mach port without adding a reference. `policy` is a valid, aligned
    // thread_time_constraint_policy that outlives the call, and the count is its size in
    // natural_t words, as the flavour requires.
    let r = unsafe {
        libc::thread_policy_set(
            libc::pthread_mach_thread_np(libc::pthread_self()),
            libc::THREAD_TIME_CONSTRAINT_POLICY as _,
            (&raw mut policy).cast(),
            libc::THREAD_TIME_CONSTRAINT_POLICY_COUNT,
        )
    };
    if r == 0 {
        Ok(RealtimeToken)
    } else {
        Err(Some(r))
    }
}

pub(crate) fn set_affinity(_: u64) -> Result<(), ThreadError> {
    Err(ThreadError::AffinityUnsupported)
}

// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Behaviour tests for WP-1.1-06 increment 1 (B1, B2, B6).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use syrp_thread::{
    DEFAULT_DROP_TIMEOUT, Diagnostic, Priority, RealtimeOptions, SchedulingStatus, Stopped, Thread,
    ThreadError, current_thread_id, current_thread_name, set_current_thread_affinity_mask,
    set_current_thread_priority, sleep, yield_now,
};

const LONG: Duration = Duration::from_secs(10);

fn code(e: &ThreadError) -> &'static str {
    match e.code().as_str() {
        "SYRP-THREAD-0001" => "0001",
        "SYRP-THREAD-0002" => "0002",
        "SYRP-THREAD-0003" => "0003",
        "SYRP-THREAD-0004" => "0004",
        "SYRP-THREAD-0005" => "0005",
        "SYRP-THREAD-0006" => "0006",
        "SYRP-THREAD-0007" => "0007",
        other => panic!("unexpected code {other}"),
    }
}

#[test]
fn b1_spawned_thread_has_name_and_id() {
    let (tx, rx) = mpsc::channel();
    let t = Thread::builder("b1-named")
        .stack_size(512 * 1024)
        .spawn(move |_| {
            tx.send((current_thread_id(), current_thread_name()))
                .unwrap()
        })
        .unwrap();
    let (id, name) = rx.recv().unwrap();
    assert_eq!(id, t.id());
    assert_ne!(id, current_thread_id());
    assert_eq!(name.as_deref(), Some("b1-named"));
    assert_eq!(t.name(), Some("b1-named"));
    assert_eq!(t.stop(LONG), Ok(Stopped::Finished));
}

#[test]
fn b1_notify_wakes_wait_once() {
    let (tx, rx) = mpsc::channel();
    let (go, gate) = mpsc::channel::<()>();
    let t = Thread::builder("b1-notify")
        .spawn(move |ctx| {
            gate.recv().unwrap();
            tx.send(ctx.wait(None)).unwrap();
            // Two notifies before the first wait collapse into one wake-up.
            tx.send(ctx.wait(Some(Duration::from_millis(50)))).unwrap();
        })
        .unwrap();
    t.notify();
    t.notify();
    go.send(()).unwrap();
    assert!(rx.recv().unwrap());
    assert!(!rx.recv().unwrap());
    assert!(t.wait_for_thread_to_exit(Some(LONG)));
}

#[test]
fn b1_wait_times_out_with_false() {
    let t = Thread::builder("b1-timeout")
        .spawn(|ctx| {
            let start = Instant::now();
            assert!(!ctx.wait(Some(Duration::from_millis(30))));
            assert!(start.elapsed() >= Duration::from_millis(30));
        })
        .unwrap();
    // Wait without signalling, which would wake the body early.
    assert!(t.wait_for_thread_to_exit(Some(LONG)));
    assert_eq!(t.stop(LONG), Ok(Stopped::Finished));
}

#[test]
fn b1_exit_signal_sets_flag_and_wakes_wait() {
    let (tx, rx) = mpsc::channel();
    let t = Thread::builder("b1-exit")
        .spawn(move |ctx| {
            assert!(!ctx.thread_should_exit());
            tx.send(()).unwrap();
            assert!(ctx.wait(None));
            assert!(ctx.thread_should_exit());
            // Once signalled, every wait returns at once.
            assert!(ctx.wait(None));
        })
        .unwrap();
    rx.recv().unwrap();
    assert!(t.is_running());
    t.signal_thread_should_exit();
    assert!(t.wait_for_thread_to_exit(Some(LONG)));
    assert!(!t.is_running());
    assert_eq!(t.stop(LONG), Ok(Stopped::Finished));
}

#[test]
fn b1_stop_reports_panic() {
    let t = Thread::builder("b1-panic")
        .spawn(|_| panic!("deliberate test panic"))
        .unwrap();
    assert_eq!(t.stop(LONG), Ok(Stopped::Panicked));
}

#[test]
fn b1_stop_times_out_and_detaches() {
    let (release, gate) = mpsc::channel::<()>();
    let done = Arc::new(AtomicUsize::new(0));
    let d = Arc::clone(&done);
    let t = Thread::builder("b1-stubborn")
        .spawn(move |_| {
            let _ = gate.recv(); // ignores the exit signal
            d.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();
    let start = Instant::now();
    let err = t.stop(Duration::from_millis(50)).unwrap_err();
    assert!(start.elapsed() < Duration::from_secs(5));
    assert_eq!(code(&err), "0001");
    assert!(err.to_string().contains("detached"));
    // The detached thread is still alive and finishes once released.
    release.send(()).unwrap();
    let deadline = Instant::now() + LONG;
    while done.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
        sleep(Duration::from_millis(1));
    }
    assert_eq!(done.load(Ordering::SeqCst), 1);
}

#[test]
fn b1_drop_waits_then_detaches_a_body_that_ignores_the_signal() {
    assert_eq!(DEFAULT_DROP_TIMEOUT, Duration::from_secs(5));
    let (release, gate) = mpsc::channel::<()>();
    let t = Thread::builder("b1-drop")
        .drop_timeout(Duration::from_millis(50))
        .spawn(move |_| {
            let _ = gate.recv();
        })
        .unwrap();
    let start = Instant::now();
    drop(t);
    let took = start.elapsed();
    assert!(
        took >= Duration::from_millis(50),
        "drop returned after {took:?}"
    );
    assert!(took < Duration::from_secs(5), "drop hung for {took:?}");
    release.send(()).unwrap();
}

#[test]
fn b1_drop_joins_a_cooperative_body() {
    let done = Arc::new(AtomicUsize::new(0));
    let d = Arc::clone(&done);
    let t = Thread::builder("b1-drop-coop")
        .spawn(move |ctx| {
            while !ctx.thread_should_exit() {
                ctx.wait(None);
            }
            d.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();
    drop(t);
    assert_eq!(done.load(Ordering::SeqCst), 1);
}

#[test]
fn b1_launch_and_free_functions() {
    let (tx, rx) = mpsc::channel();
    Thread::launch(move || {
        yield_now();
        sleep(Duration::from_millis(1));
        tx.send(current_thread_id()).unwrap();
    })
    .unwrap();
    assert_ne!(rx.recv_timeout(LONG).unwrap(), current_thread_id());
}

#[test]
fn b2_every_priority_runs_and_reports_its_status() {
    for p in [
        Priority::Lowest,
        Priority::Low,
        Priority::Normal,
        Priority::High,
        Priority::Highest,
    ] {
        let t = Thread::builder("b2-prio")
            .priority(p)
            .spawn(|_| {})
            .unwrap();
        assert_ne!(t.priority_status(), SchedulingStatus::NotRequested, "{p:?}");
        assert_eq!(t.realtime_status(), SchedulingStatus::NotRequested);
        assert_eq!(t.stop(LONG), Ok(Stopped::Finished));
    }
    // Lowering one's own priority needs no privilege on any supported OS.
    let t = Thread::builder("b2-low")
        .priority(Priority::Lowest)
        .spawn(|_| {})
        .unwrap();
    if cfg!(any(target_os = "linux", target_os = "macos", windows)) {
        assert_eq!(t.priority_status(), SchedulingStatus::Granted);
    }
}

#[test]
fn b2_realtime_is_granted_or_refused_and_the_thread_still_runs() {
    let opts = RealtimeOptions::new()
        .with_priority(8)
        .with_processing_period(Duration::from_micros(2_667))
        .with_expected_work_time(Duration::from_millis(1));
    let ran = Arc::new(AtomicUsize::new(0));
    let r = Arc::clone(&ran);
    let t = Thread::builder("b2-rt")
        .realtime(opts)
        .spawn(move |_| {
            r.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();
    let status = t.realtime_status();
    assert!(
        matches!(
            status,
            SchedulingStatus::Granted | SchedulingStatus::Refused { .. }
        ),
        "{status:?}"
    );
    println!("realtime status on this runner: {status:?}");
    assert_eq!(t.stop(LONG), Ok(Stopped::Finished));
    assert_eq!(ran.load(Ordering::SeqCst), 1);
}

#[test]
fn b2_realtime_options_are_clamped_and_checked() {
    let o = RealtimeOptions::default().with_priority(200);
    assert_eq!(o.priority(), 10);
    assert_eq!(RealtimeOptions::new().priority(), 5);
    let cases = [
        RealtimeOptions::new().with_processing_period(Duration::ZERO),
        RealtimeOptions::new().with_expected_work_time(Duration::ZERO),
        RealtimeOptions::new()
            .with_processing_period(Duration::from_millis(1))
            .with_expected_work_time(Duration::from_millis(2)),
    ];
    for opts in cases {
        let err = Thread::builder("b2-bad")
            .realtime(opts)
            .spawn(|_| {})
            .unwrap_err();
        assert_eq!(code(&err), "0006");
    }
}

#[test]
fn b2_set_current_thread_priority_is_granted_or_coded() {
    let t = Thread::builder("b2-set")
        .spawn(|_| {
            set_current_thread_priority(Priority::Low).unwrap();
            if let Err(e) = set_current_thread_priority(Priority::Highest) {
                assert_eq!(code(&e), "0003");
            }
        })
        .unwrap();
    assert_eq!(t.stop(LONG), Ok(Stopped::Finished));
}

#[test]
fn b2_affinity_is_set_or_unsupported_on_macos() {
    let t = Thread::builder("b2-affinity")
        .spawn(|_| {
            let one = set_current_thread_affinity_mask(0b1);
            let zero = set_current_thread_affinity_mask(0);
            if cfg!(target_vendor = "apple") {
                assert_eq!(code(&one.unwrap_err()), "0004");
                assert_eq!(code(&zero.unwrap_err()), "0004");
            } else {
                // CPU 0 may be outside a CI runner's CPU set; then the OS refuses it.
                if let Err(e) = one {
                    assert_eq!(code(&e), "0005");
                }
                assert_eq!(code(&zero.unwrap_err()), "0005");
            }
        })
        .unwrap();
    assert_eq!(t.stop(LONG), Ok(Stopped::Finished));
}

#[test]
fn b6_every_code_is_catalogued() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../errors/thread.toml");
    let catalogue = std::fs::read_to_string(path).unwrap();
    let e = ThreadError::AffinityUnsupported;
    assert!(catalogue.contains(e.code().as_str()), "{e}");
    assert!(e.hint().is_some());
    for n in 1..=7 {
        assert!(catalogue.contains(&format!("code = \"SYRP-THREAD-{n:04}\"")));
    }
    // Codes are dense and each entry is complete: no gaps, no extras, no missing fields.
    assert_eq!(catalogue.matches("code = \"SYRP-THREAD-").count(), 7);
    for field in ["title = ", "severity = ", "cause = ", "fix = "] {
        assert_eq!(catalogue.matches(field).count(), 7, "{field}");
    }
}

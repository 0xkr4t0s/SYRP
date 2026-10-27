// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Behaviour tests for WP-1.1-06 increment 4: ThreadPool (B4) and its error code (B6).

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use syrp_thread::{
    Diagnostic, JobContext, JobStatus, ThreadPool, ThreadPoolJob, ThreadPoolOptions, WaitableEvent,
};

const LONG: Option<Duration> = Some(Duration::from_secs(10));

fn pool(n: usize) -> ThreadPool {
    ThreadPool::new(ThreadPoolOptions {
        thread_name: "test-pool".into(),
        num_threads: n,
        ..ThreadPoolOptions::default()
    })
    .unwrap()
}

/// A job that blocks until `gate` is signalled or it is interrupted, and reports both.
fn gated(
    pool: &ThreadPool,
    gate: &Arc<WaitableEvent>,
    started: &Arc<WaitableEvent>,
) -> syrp_thread::JobHandle {
    let (gate, started) = (Arc::clone(gate), Arc::clone(started));
    pool.add_fn(move |ctx| {
        started.signal();
        while !ctx.should_exit() && !gate.wait(Some(Duration::from_millis(2))) {}
        JobStatus::Finished
    })
}

#[test]
fn b4_default_options_use_the_logical_cpu_count() {
    let opts = ThreadPoolOptions::default();
    assert_eq!(opts.num_threads, syrp_sys::SystemStats::num_cpus());
    assert_eq!(pool(3).num_threads(), 3);
    assert_eq!(pool(0).num_threads(), 1);
}

#[test]
fn b4_every_job_runs_exactly_once() {
    let p = pool(4);
    let runs: Arc<Vec<AtomicUsize>> = Arc::new((0..200).map(|_| AtomicUsize::new(0)).collect());
    let handles: Vec<_> = (0..200)
        .map(|i| {
            let runs = Arc::clone(&runs);
            p.add_fn(move |_| {
                runs[i].fetch_add(1, SeqCst);
                JobStatus::Finished
            })
        })
        .collect();
    for h in handles {
        assert!(p.wait_for_job_to_finish(h, LONG));
    }
    assert!(runs.iter().all(|r| r.load(SeqCst) == 1));
    assert_eq!(p.num_jobs(), 0);
}

#[test]
fn b4_jobs_start_in_fifo_order_and_move_to_front_jumps_the_queue() {
    let p = pool(1);
    let (gate, started) = (
        Arc::new(WaitableEvent::new(true)),
        Arc::new(WaitableEvent::new(true)),
    );
    let blocker = gated(&p, &gate, &started);
    assert!(started.wait(LONG));
    let order = Arc::new(Mutex::new(Vec::new()));
    let jobs: Vec<_> = (0..4)
        .map(|i| {
            let order = Arc::clone(&order);
            p.add_fn(move |_| {
                order.lock().unwrap().push(i);
                JobStatus::Finished
            })
        })
        .collect();
    assert!(p.is_job_running(blocker) && !p.is_job_running(jobs[3]));
    assert!(p.move_job_to_front(jobs[3]));
    assert!(!p.move_job_to_front(blocker), "a running job is not queued");
    gate.signal();
    for j in jobs {
        assert!(p.wait_for_job_to_finish(j, LONG));
    }
    assert_eq!(*order.lock().unwrap(), [3, 0, 1, 2]);
}

struct Counter {
    left: usize,
    runs: Arc<AtomicUsize>,
    busy: Arc<AtomicBool>,
}

impl ThreadPoolJob for Counter {
    fn name(&self) -> &str {
        "counter"
    }

    fn run_job(&mut self, _: &JobContext) -> JobStatus {
        assert!(
            !self.busy.swap(true, SeqCst),
            "job ran on two threads at once"
        );
        std::thread::yield_now();
        self.runs.fetch_add(1, SeqCst);
        self.left -= 1;
        self.busy.store(false, SeqCst);
        if self.left == 0 {
            JobStatus::Finished
        } else {
            JobStatus::RunAgain
        }
    }
}

#[test]
fn b4_run_again_counts_and_never_overlaps_itself() {
    let p = pool(4);
    let runs = Arc::new(AtomicUsize::new(0));
    let handles: Vec<_> = (0..8)
        .map(|_| {
            p.add_job(Counter {
                left: 25,
                runs: Arc::clone(&runs),
                busy: Arc::new(AtomicBool::new(false)),
            })
        })
        .collect();
    for h in handles {
        assert!(p.wait_for_job_to_finish(h, LONG));
    }
    assert_eq!(runs.load(SeqCst), 8 * 25);
}

#[test]
fn b4_run_again_goes_to_the_back_of_the_queue() {
    let p = pool(1);
    let (gate, started) = (
        Arc::new(WaitableEvent::new(true)),
        Arc::new(WaitableEvent::new(true)),
    );
    let blocker = gated(&p, &gate, &started);
    assert!(started.wait(LONG));
    let order = Arc::new(Mutex::new(Vec::new()));
    let (o1, o2) = (Arc::clone(&order), Arc::clone(&order));
    let mut twice = 2;
    let a = p.add_fn(move |_| {
        o1.lock().unwrap().push("a");
        twice -= 1;
        if twice == 0 {
            JobStatus::Finished
        } else {
            JobStatus::RunAgain
        }
    });
    let b = p.add_fn(move |_| {
        o2.lock().unwrap().push("b");
        JobStatus::Finished
    });
    gate.signal();
    assert!(p.wait_for_job_to_finish(a, LONG) && p.wait_for_job_to_finish(b, LONG));
    assert!(p.wait_for_job_to_finish(blocker, LONG));
    assert_eq!(*order.lock().unwrap(), ["a", "b", "a"]);
}

#[test]
fn b4_remove_job_interrupts_a_running_job_and_stops_it_running_again() {
    let p = pool(2);
    let started = Arc::new(WaitableEvent::new(true));
    let (s, seen) = (Arc::clone(&started), Arc::new(AtomicBool::new(false)));
    let seen2 = Arc::clone(&seen);
    let job = p.add_fn(move |ctx| {
        s.signal();
        while !ctx.should_exit() {
            std::thread::sleep(Duration::from_millis(1));
        }
        seen2.store(true, SeqCst);
        JobStatus::RunAgain
    });
    assert!(started.wait(LONG));
    assert_eq!(p.remove_job(job, true, LONG), Ok(()));
    assert!(seen.load(SeqCst));
    assert!(!p.contains(job));
    assert_eq!(p.num_jobs(), 0);
}

#[test]
fn b4_removing_a_running_job_with_a_timeout_reports_the_timeout() {
    let p = pool(1);
    let (release, gate) = mpsc::channel::<()>();
    let started = Arc::new(WaitableEvent::new(true));
    let s = Arc::clone(&started);
    let job = p.add_fn(move |_| {
        s.signal();
        let _ = gate.recv(); // ignores interruption
        JobStatus::RunAgain
    });
    assert!(started.wait(LONG));
    let t0 = Instant::now();
    let err = p
        .remove_job(job, true, Some(Duration::from_millis(30)))
        .unwrap_err();
    assert!(t0.elapsed() >= Duration::from_millis(30));
    assert_eq!(err.code().as_str(), "SYRP-THREAD-0007");
    assert!(err.to_string().starts_with("SYRP-THREAD-0007"));
    assert!(p.contains(job) && p.is_job_running(job));
    release.send(()).unwrap();
    assert!(
        p.wait_for_job_to_finish(job, LONG),
        "removed: RunAgain is ignored"
    );
}

#[test]
fn b4_remove_queued_jobs_and_remove_all_with_a_filter() {
    let p = pool(1);
    let (gate, started) = (
        Arc::new(WaitableEvent::new(true)),
        Arc::new(WaitableEvent::new(true)),
    );
    let blocker = gated(&p, &gate, &started);
    assert!(started.wait(LONG));
    let ran = Arc::new(AtomicUsize::new(0));
    let queued: Vec<_> = (0..3)
        .map(|_| {
            let ran = Arc::clone(&ran);
            p.add_fn(move |_| {
                ran.fetch_add(1, SeqCst);
                JobStatus::Finished
            })
        })
        .collect();
    assert_eq!(p.num_jobs(), 4);
    assert_eq!(p.remove_job(queued[0], false, Some(Duration::ZERO)), Ok(()));
    assert_eq!(p.remove_job(queued[0], false, None), Ok(()), "already gone");
    let keep = queued[2];
    assert_eq!(
        p.remove_all_jobs(true, LONG, |h, name| h != keep && name == "closure"),
        Ok(())
    );
    assert!(!p.contains(blocker) && !p.contains(queued[1]));
    assert!(p.wait_for_job_to_finish(keep, LONG));
    assert_eq!(ran.load(SeqCst), 1);
}

#[test]
fn b4_removing_a_queued_job_wakes_a_thread_waiting_for_it() {
    let p = Arc::new(pool(1));
    let (gate, started) = (
        Arc::new(WaitableEvent::new(true)),
        Arc::new(WaitableEvent::new(true)),
    );
    let _blocker = gated(&p, &gate, &started);
    assert!(started.wait(LONG));
    let queued = p.add_fn(|_| JobStatus::Finished);
    let (tx, rx) = mpsc::channel();
    let p2 = Arc::clone(&p);
    let waiter = std::thread::spawn(move || tx.send(p2.wait_for_job_to_finish(queued, None)));
    // Give the waiter time to block; the test still passes if it has not, but then it
    // would not exercise the wake-up.
    std::thread::sleep(Duration::from_millis(20));
    assert_eq!(p.remove_job(queued, false, Some(Duration::ZERO)), Ok(()));
    // The only worker is still busy, so only the removal itself can wake the waiter.
    assert_eq!(rx.recv_timeout(Duration::from_secs(10)), Ok(true));
    gate.signal();
    waiter.join().unwrap().unwrap();
}

#[test]
fn b4_dropping_the_pool_interrupts_and_waits_for_running_jobs() {
    let p = pool(2);
    let started = Arc::new(AtomicUsize::new(0));
    let finished = Arc::new(AtomicUsize::new(0));
    for _ in 0..2 {
        let (s, f) = (Arc::clone(&started), Arc::clone(&finished));
        p.add_fn(move |ctx| {
            s.fetch_add(1, SeqCst);
            while !ctx.should_exit() {
                std::thread::sleep(Duration::from_millis(1));
            }
            std::thread::sleep(Duration::from_millis(20));
            f.fetch_add(1, SeqCst);
            JobStatus::RunAgain
        });
    }
    let never = Arc::new(AtomicBool::new(false));
    for _ in 0..5 {
        let n = Arc::clone(&never);
        p.add_fn(move |_| {
            n.store(true, SeqCst);
            JobStatus::Finished
        });
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while started.load(SeqCst) < 2 {
        assert!(Instant::now() < deadline, "both blocking jobs should start");
        std::thread::sleep(Duration::from_millis(1));
    }
    drop(p);
    assert_eq!(
        finished.load(SeqCst),
        2,
        "drop returned before the running jobs did"
    );
    assert!(
        !never.load(SeqCst),
        "queued jobs must not start once the pool is dropped"
    );
}

#[test]
fn b4_a_panicking_job_counts_as_finished_and_the_worker_survives() {
    let p = pool(1);
    let bad = p.add_fn(|_| panic!("job panics"));
    assert!(p.wait_for_job_to_finish(bad, LONG));
    let good = p.add_fn(|_| JobStatus::Finished);
    assert!(p.wait_for_job_to_finish(good, LONG));
}

#[test]
fn b4_jobs_may_call_back_into_the_pool() {
    let p = Arc::new(pool(2));
    let inner = Arc::new(Mutex::new(None));
    let (p2, inner2) = (Arc::clone(&p), Arc::clone(&inner));
    let outer = p.add_fn(move |_| {
        let h = p2.add_fn(|_| JobStatus::Finished);
        *inner2.lock().unwrap() = Some(h);
        JobStatus::Finished
    });
    assert!(p.wait_for_job_to_finish(outer, LONG));
    let h = inner.lock().unwrap().take().unwrap();
    assert!(p.wait_for_job_to_finish(h, LONG));
    assert_eq!(p.num_jobs(), 0);
}

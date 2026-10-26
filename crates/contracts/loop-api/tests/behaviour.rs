// SPDX-License-Identifier: MIT OR Apache-2.0
//! One or more tests per behaviour in work/specs/WP-0.2-01.md.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use syrp_error::Diagnostic;
use syrp_loop_api::{
    Coalescer, LoopError, ManualLoop, MessageLoop, MessageThread, PeerId, Task, TaskSender,
    TimerGuard,
};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// A shared, single-threaded event log.
type Log<T> = Rc<RefCell<Vec<T>>>;

/// Asserts an error and its catalogued code (each `LoopError` variant has its own).
fn assert_code<T: std::fmt::Debug>(result: Result<T, LoopError>, code: &str) {
    assert_eq!(result.unwrap_err().code().as_str(), code);
}

/// Waits (in real time) until another thread has posted `n` tasks.
fn wait_for_tasks(lp: &ManualLoop, n: usize) {
    while lp.pending_tasks() < n {
        thread::yield_now();
    }
}

fn counting_coalescer(lp: &ManualLoop) -> (Coalescer, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let c = calls.clone();
    let co = Coalescer::new(lp.task_sender(), move |_| _ = c.fetch_add(1, Relaxed));
    (co, calls)
}

// `check` resolves only when `T` lacks the auto trait: otherwise both impls apply, `_` is
// ambiguous and the test does not compile.
trait AmbiguousIfSend<A> {
    fn check() {}
}
impl<T: ?Sized> AmbiguousIfSend<()> for T {}
impl<T: ?Sized + Send> AmbiguousIfSend<u8> for T {}
trait AmbiguousIfSync<A> {
    fn check() {}
}
impl<T: ?Sized> AmbiguousIfSync<()> for T {}
impl<T: ?Sized + Sync> AmbiguousIfSync<u8> for T {}

#[test]
fn b1_message_thread_is_a_zero_sized_non_send_non_sync_token() {
    assert_eq!(size_of::<MessageThread<'static>>(), 0);
    <MessageThread<'static> as AmbiguousIfSend<_>>::check();
    <MessageThread<'static> as AmbiguousIfSync<_>>::check();
    <ManualLoop as AmbiguousIfSend<_>>::check(); // so its tokens cannot travel either
}

#[test]
fn b1_loops_hand_a_token_to_tasks_timers_and_vblanks() {
    // Compiles only if each callback is given a `&MessageThread`.
    fn on(_: &MessageThread<'_>, what: &'static str) -> &'static str {
        what
    }
    let lp = ManualLoop::new();
    let log: Log<&str> = Rc::default();
    let l = log.clone();
    let _t = lp.timer(
        ms(10),
        Box::new(move |mt| l.borrow_mut().push(on(mt, "timer"))),
    );
    let l = log.clone();
    let vblank = Box::new(move |mt: &MessageThread<'_>, _| l.borrow_mut().push(on(mt, "vblank")));
    let _v = lp.on_vblank(PeerId::from_raw(1), vblank);
    lp.post(Task::new(|mt| _ = on(mt, "task")));
    lp.advance(ms(10));
    lp.vblank(PeerId::from_raw(1));
    assert_eq!(lp.run_until_idle(), Ok(1));
    assert_eq!(*log.borrow(), ["timer", "vblank"]);
}

#[test]
fn b2_post_from_another_thread_runs_later_on_the_message_thread() {
    let lp = ManualLoop::new();
    let ran_on = Arc::new(Mutex::new(None));
    let (r, sender) = (ran_on.clone(), lp.task_sender());
    let task = Task::new(move |_| *r.lock().unwrap() = Some(thread::current().id()));
    thread::spawn(move || sender.post(task)).join().unwrap();
    assert_eq!(*ran_on.lock().unwrap(), None, "not run inline");
    assert_eq!(lp.run_until_idle(), Ok(1));
    assert_eq!(*ran_on.lock().unwrap(), Some(thread::current().id()));
}

#[test]
fn b2_dropping_the_timer_guard_cancels_even_from_its_own_callback() {
    let lp = ManualLoop::new();
    let guard = lp.timer(ms(10), Box::new(|_| {}));
    assert_eq!(lp.advance(ms(25)), 2);
    drop(guard);
    assert_eq!(lp.advance(ms(100)), 0);

    let slot: Rc<RefCell<Option<TimerGuard>>> = Rc::default();
    let s = slot.clone();
    *slot.borrow_mut() = Some(lp.timer(ms(10), Box::new(move |_| drop(s.borrow_mut().take()))));
    assert_eq!(lp.advance(ms(100)), 1);
}

#[test]
fn b2_vblank_fires_for_its_peer_until_the_guard_drops() {
    let lp = ManualLoop::new();
    let (a, b) = (PeerId::from_raw(1), PeerId::from_raw(2));
    let seen: Log<Duration> = Rc::default();
    let s = seen.clone();
    let guard = lp.on_vblank(a, Box::new(move |_, at| s.borrow_mut().push(at)));
    lp.advance(ms(16));
    assert_eq!((lp.vblank(a), lp.vblank(b)), (1, 0));
    drop(guard);
    assert_eq!(lp.vblank(a), 0);
    assert_eq!(*seen.borrow(), [ms(16)]);
}

#[test]
fn b2_post_and_wait_on_the_message_thread_is_an_error() {
    let lp = ManualLoop::new();
    assert_code(lp.post_and_wait(|_| 1), "SYRP-LOOP-0001");
    assert_eq!(lp.pending_tasks(), 0, "nothing is posted");
    // Also from inside a task, which runs on the message thread.
    let (result, sender) = (Arc::new(Mutex::new(None)), lp.task_sender());
    let r = result.clone();
    lp.post(Task::new(move |_| {
        *r.lock().unwrap() = Some(sender.post_and_wait(|_| ()))
    }));
    lp.run_until_idle().unwrap();
    let inner = result.lock().unwrap().take().unwrap();
    assert_code(inner, "SYRP-LOOP-0001");
}

#[test]
fn b2_post_and_wait_from_another_thread_returns_the_result() {
    let lp = ManualLoop::new();
    let sender = lp.task_sender();
    let worker = thread::spawn(move || sender.post_and_wait(|_mt| 6 * 7));
    wait_for_tasks(&lp, 1);
    assert_eq!(lp.run_until_idle(), Ok(1));
    assert_eq!(worker.join().unwrap(), Ok(42));
}

#[test]
fn b2_post_and_wait_fails_when_the_loop_is_dropped() {
    let lp = ManualLoop::new();
    let sender = lp.task_sender();
    let worker = thread::spawn(move || sender.post_and_wait(|_| ()));
    wait_for_tasks(&lp, 1);
    drop(lp);
    assert_code(worker.join().unwrap(), "SYRP-LOOP-0003");
}

#[test]
fn b3_a_30ms_timer_fires_exactly_3_times_in_100ms() {
    let lp = ManualLoop::new();
    let times: Log<Duration> = Rc::default();
    let (t, l) = (times.clone(), lp.clone());
    let _g = lp.timer(ms(30), Box::new(move |_| t.borrow_mut().push(l.now())));
    assert_eq!(lp.advance(ms(100)), 3);
    assert_eq!(*times.borrow(), [ms(30), ms(60), ms(90)]);
    assert_eq!(lp.now(), ms(100));
    // Ten 10 ms steps fire the same timer as often as one 100 ms step.
    let lp = ManualLoop::new();
    let _g = lp.timer(ms(30), Box::new(|_| {}));
    assert_eq!((0..10).map(|_| lp.advance(ms(10))).sum::<usize>(), 3);
}

#[test]
fn b3_timers_fire_in_time_order_ties_in_creation_order() {
    let lp = ManualLoop::new();
    let log: Log<String> = Rc::default();
    let guards = [("a", 25), ("b", 10), ("c", 20)].map(|(name, period)| {
        let (l, lp2) = (log.clone(), lp.clone());
        let f = move |_: &MessageThread<'_>| l.borrow_mut().push(format!("{:?}{name}", lp2.now()));
        lp.timer(ms(period), Box::new(f))
    });
    lp.advance(ms(50));
    let want = "10msb 20msb 20msc 25msa 30msb 40msb 40msc 50msa 50msb";
    assert_eq!(log.borrow().join(" "), want);
    drop(guards);
}

#[test]
fn b3_run_until_idle_drains_tasks_posted_by_tasks() {
    let lp = ManualLoop::new();
    let order = Arc::new(Mutex::new(Vec::new()));
    let (o, sender) = (order.clone(), lp.task_sender());
    lp.post(Task::new(move |_| {
        o.lock().unwrap().push(1);
        let o2 = o.clone();
        sender.post(Task::new(move |_| o2.lock().unwrap().push(3)));
    }));
    let o = order.clone();
    lp.post(Task::new(move |_| o.lock().unwrap().push(2)));
    assert_eq!(lp.run_until_idle(), Ok(3));
    assert_eq!(*order.lock().unwrap(), [1, 2, 3]);
    assert_eq!(lp.run_until_idle(), Ok(0));
}

fn repost_forever(sender: TaskSender) -> Task {
    Task::new(move |_| sender.post(repost_forever(sender.clone())))
}

#[test]
fn b3_run_until_idle_stops_at_the_limit_with_an_error() {
    let lp = ManualLoop::new();
    lp.set_task_limit(50);
    lp.post(repost_forever(lp.task_sender()));
    assert_code(lp.run_until_idle(), "SYRP-LOOP-0002");
    assert_eq!(lp.pending_tasks(), 1, "the remaining work stays queued");
    // Exactly `limit` tasks with nothing left over is not an error.
    let lp = ManualLoop::new();
    lp.set_task_limit(2);
    (0..2).for_each(|_| lp.post(Task::new(|_| {})));
    assert_eq!(lp.run_until_idle(), Ok(2));
}

#[test]
fn b4_a_timer_made_in_a_callback_first_fires_on_its_next_due_time() {
    let lp = ManualLoop::new();
    let child_times: Log<Duration> = Rc::default();
    let child: Rc<RefCell<Option<TimerGuard>>> = Rc::default();
    let (lp2, ct, cg) = (lp.clone(), child_times.clone(), child.clone());
    let _parent = lp.timer(
        ms(10),
        Box::new(move |_mt: &MessageThread<'_>| {
            if cg.borrow().is_none() {
                let (lp3, ct2) = (lp2.clone(), ct.clone());
                // A zero period (raised to 1 ms) still must not fire at this same instant.
                let f = Box::new(move |_: &MessageThread<'_>| ct2.borrow_mut().push(lp3.now()));
                *cg.borrow_mut() = Some(lp2.timer(Duration::ZERO, f));
            }
        }),
    );
    lp.advance(ms(10));
    assert!(child_times.borrow().is_empty(), "not at the same instant");
    lp.advance(ms(2));
    assert_eq!(*child_times.borrow(), [ms(11), ms(12)]);
    child.borrow_mut().take();
}

#[test]
fn b5_many_triggers_before_the_loop_runs_give_one_callback() {
    let lp = ManualLoop::new();
    let (co, calls) = counting_coalescer(&lp);
    (0..1000).for_each(|_| co.trigger());
    assert_eq!(lp.pending_tasks(), 1);
    lp.run_until_idle().unwrap();
    assert_eq!((calls.load(Relaxed), co.is_pending()), (1, false));
    co.trigger();
    lp.run_until_idle().unwrap();
    assert_eq!(calls.load(Relaxed), 2);
}

#[test]
fn b5_triggers_from_many_threads_coalesce() {
    let lp = ManualLoop::new();
    let (co, calls) = counting_coalescer(&lp);
    thread::scope(|s| (0..8).for_each(|_| _ = s.spawn(|| (0..100).for_each(|_| co.trigger()))));
    lp.run_until_idle().unwrap();
    assert_eq!(calls.load(Relaxed), 1);
}

#[test]
fn b5_trigger_during_the_callback_schedules_one_more_run() {
    let lp = ManualLoop::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let slot: Arc<Mutex<Option<Arc<Coalescer>>>> = Arc::default();
    let (c, s) = (calls.clone(), slot.clone());
    let co = Arc::new(Coalescer::new(lp.task_sender(), move |_| {
        if c.fetch_add(1, Relaxed) == 0 {
            s.lock().unwrap().clone().unwrap().trigger();
        }
    }));
    *slot.lock().unwrap() = Some(co.clone());
    co.trigger();
    lp.run_until_idle().unwrap();
    assert_eq!(calls.load(Relaxed), 2);
    slot.lock().unwrap().take();
}

#[test]
fn b5_cancel_and_drop_withdraw_the_update() {
    let lp = ManualLoop::new();
    let (co, calls) = counting_coalescer(&lp);
    co.trigger();
    co.cancel();
    lp.run_until_idle().unwrap();
    co.trigger();
    drop(co);
    lp.run_until_idle().unwrap();
    assert_eq!(calls.load(Relaxed), 0);
}

/// B6 freezes the public API (the snapshot is committed by the PR manager after review);
/// ADR 0003 D3 requires frozen contract types to have no public fields.
#[test]
fn b6_contract_types_have_no_public_fields() {
    let src = [
        include_str!("../src/lib.rs"),
        include_str!("../src/manual.rs"),
        include_str!("../src/coalescer.rs"),
    ];
    let mut in_struct = false;
    for line in src.iter().flat_map(|s| s.lines()) {
        if line.starts_with("pub struct") {
            assert!(!line.contains("(pub"), "public tuple field: {line}");
            in_struct = line.ends_with('{');
        } else if line.starts_with('}') {
            in_struct = false;
        } else if in_struct {
            assert!(
                !line.trim_start().starts_with("pub"),
                "public field: {line}"
            );
        }
    }
}

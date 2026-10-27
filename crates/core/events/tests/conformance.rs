// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B2: the `syrp-loop-api` invariants, written once against a small driver trait and run on
//! both `ManualLoop` and `EventLoop`, so the two cannot drift.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use syrp_error::Diagnostic;
use syrp_events::EventLoop;
use syrp_loop_api::{ManualLoop, MessageLoop, Task, TimerGuard};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// What the suite needs from a loop beyond the contract.
trait Driver: MessageLoop + Sized + 'static {
    fn create() -> Self;
    /// Loop time on the clock its timers use.
    fn clock(&self) -> Duration;
    /// Runs posted tasks, and the tasks they post, until none are left.
    fn settle(&self);
    /// Lets `d` pass, firing timers and running tasks.
    fn pass_time(&self, d: Duration);
}

impl Driver for ManualLoop {
    fn create() -> Self {
        ManualLoop::new()
    }
    fn clock(&self) -> Duration {
        self.now()
    }
    fn settle(&self) {
        self.run_until_idle().unwrap();
    }
    fn pass_time(&self, d: Duration) {
        self.advance(d);
        self.settle();
    }
}

impl Driver for EventLoop {
    fn create() -> Self {
        EventLoop::new().unwrap()
    }
    fn clock(&self) -> Duration {
        self.now()
    }
    fn settle(&self) {
        while self.dispatch_pending() > 0 {}
    }
    fn pass_time(&self, d: Duration) {
        self.run_for(d);
    }
}

type Log<T> = Rc<RefCell<Vec<T>>>;

fn post_never_runs_inline_and_keeps_one_threads_order<D: Driver>() {
    let lp = D::create();
    let log = Arc::new(Mutex::new(Vec::new()));
    let sender = lp.task_sender();
    let l = log.clone();
    thread::spawn(move || {
        for i in 0..100 {
            let l = l.clone();
            sender.post(Task::new(move |_| l.lock().unwrap().push(i)));
        }
    })
    .join()
    .unwrap();
    let flag = Arc::new(Mutex::new(false));
    let f = flag.clone();
    lp.post(Task::new(move |_| *f.lock().unwrap() = true));
    assert!(!*flag.lock().unwrap(), "post ran the task inline");
    lp.settle();
    assert!(*flag.lock().unwrap());
    assert_eq!(*log.lock().unwrap(), (0..100).collect::<Vec<_>>());
}

#[test]
fn b2_post_never_runs_inline_and_keeps_one_threads_order() {
    post_never_runs_inline_and_keeps_one_threads_order::<ManualLoop>();
    post_never_runs_inline_and_keeps_one_threads_order::<EventLoop>();
}

fn post_and_wait<D: Driver>() {
    let lp = D::create();
    let code = lp.post_and_wait(|_| ()).unwrap_err().code();
    assert_eq!(code.as_str(), "SYRP-LOOP-0001");
    // From another thread it waits for the loop and returns the result.
    let sender = lp.task_sender();
    let waiter = thread::spawn(move || sender.post_and_wait(|_| 7));
    while !waiter.is_finished() {
        lp.pass_time(ms(1));
    }
    assert_eq!(waiter.join().unwrap(), Ok(7));
    // A dropped loop wakes waiters with SYRP-LOOP-0003.
    let sender = lp.task_sender();
    let waiter = thread::spawn(move || sender.post_and_wait(|_| 7));
    drop(lp);
    let code = waiter.join().unwrap().unwrap_err().code();
    assert_eq!(code.as_str(), "SYRP-LOOP-0003");
}

#[test]
fn b2_post_and_wait_fails_on_the_loop_thread_and_when_the_loop_drops() {
    post_and_wait::<ManualLoop>();
    post_and_wait::<EventLoop>();
}

fn timers_fire_in_due_then_creation_order<D: Driver>() {
    let lp = D::create();
    let log: Log<char> = Rc::default();
    let timer = |period, name| {
        let l = log.clone();
        lp.timer(ms(period), Box::new(move |_| l.borrow_mut().push(name)))
    };
    let _guards = [timer(30, 'a'), timer(20, 'b'), timer(20, 'c')];
    lp.pass_time(ms(65));
    assert_eq!(log.borrow()[..3], ['b', 'c', 'a']);
}

#[test]
fn b2_timers_fire_in_due_then_creation_order() {
    timers_fire_in_due_then_creation_order::<ManualLoop>();
    timers_fire_in_due_then_creation_order::<EventLoop>();
}

fn sub_millisecond_periods_become_one<D: Driver>() -> usize {
    let lp = D::create();
    let count = Rc::new(RefCell::new(0));
    let c = count.clone();
    let _t = lp.timer(Duration::ZERO, Box::new(move |_| *c.borrow_mut() += 1));
    lp.pass_time(ms(20));
    count.take()
}

#[test]
fn b2_sub_millisecond_periods_become_one_millisecond() {
    assert_eq!(sub_millisecond_periods_become_one::<ManualLoop>(), 20);
    // Real time: at most one firing per millisecond, with slack for the end of run_for.
    assert!((1..=25).contains(&sub_millisecond_periods_become_one::<EventLoop>()));
}

fn timer_made_in_callback_fires_one_period_later<D: Driver>() {
    let lp = Rc::new(D::create());
    let created: Rc<RefCell<Option<(Duration, TimerGuard)>>> = Rc::default();
    let first: Rc<RefCell<Option<Duration>>> = Rc::default();
    let (weak, c, fst) = (Rc::downgrade(&lp), created.clone(), first.clone());
    let _outer = lp.timer(
        ms(10),
        Box::new(move |_| {
            let Some(lp) = weak.upgrade() else { return };
            if c.borrow().is_some() {
                return;
            }
            let (w, f) = (weak.clone(), fst.clone());
            let inner = lp.timer(
                ms(30),
                Box::new(move |_| {
                    let now = w.upgrade().map(|lp| lp.clock());
                    f.borrow_mut().get_or_insert(now.unwrap());
                }),
            );
            *c.borrow_mut() = Some((lp.clock(), inner));
        }),
    );
    lp.pass_time(ms(60));
    let made = created.borrow().as_ref().map(|(at, _)| *at).unwrap();
    assert!(first.borrow().unwrap() >= made + ms(30));
}

#[test]
fn b2_timer_made_in_a_callback_first_fires_one_period_later() {
    timer_made_in_callback_fires_one_period_later::<ManualLoop>();
    timer_made_in_callback_fires_one_period_later::<EventLoop>();
}

fn dropping_guard_in_own_callback_cancels<D: Driver>() {
    let lp = D::create();
    let slot: Rc<RefCell<Option<TimerGuard>>> = Rc::default();
    let count = Rc::new(RefCell::new(0));
    let (s, c) = (slot.clone(), count.clone());
    let guard = lp.timer(
        ms(5),
        Box::new(move |_| {
            *c.borrow_mut() += 1;
            drop(s.borrow_mut().take());
        }),
    );
    *slot.borrow_mut() = Some(guard);
    lp.pass_time(ms(40));
    assert_eq!(count.take(), 1);
}

#[test]
fn b2_dropping_a_guard_inside_its_own_callback_cancels_at_once() {
    dropping_guard_in_own_callback_cancels::<ManualLoop>();
    dropping_guard_in_own_callback_cancels::<EventLoop>();
}

fn no_borrow_held_during_callbacks<D: Driver>() {
    let lp = Rc::new(D::create());
    let other: Rc<RefCell<Option<TimerGuard>>> = Rc::default();
    let made: Rc<RefCell<Vec<TimerGuard>>> = Rc::default();
    *other.borrow_mut() = Some(lp.timer(ms(1000), Box::new(|_| {})));
    let (weak, o, m) = (Rc::downgrade(&lp), other.clone(), made.clone());
    let _t = lp.timer(
        ms(5),
        Box::new(move |_| {
            let Some(lp) = weak.upgrade() else { return };
            // Re-enter the loop from inside a callback: drop a guard, add a timer, post.
            drop(o.borrow_mut().take());
            m.borrow_mut().push(lp.timer(ms(5), Box::new(|_| {})));
            lp.post(Task::new(|_| {}));
        }),
    );
    lp.pass_time(ms(30));
    assert!(!made.borrow().is_empty());
    made.borrow_mut().clear(); // drop guards while the loop is idle
}

#[test]
fn b2_no_borrow_or_lock_is_held_while_callbacks_run() {
    no_borrow_held_during_callbacks::<ManualLoop>();
    no_borrow_held_during_callbacks::<EventLoop>();
}

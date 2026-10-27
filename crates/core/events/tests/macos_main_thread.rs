// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B5 tests that need the process's main thread, so this binary has `harness = false`. On
//! the main thread, tasks and timers are delivered when something else runs the `CFRunLoop`
//! (`CFRunLoopRunInMode` here, standing in for `NSApplication run`), with no call to
//! `EventLoop::run`. Other targets run nothing.

#[cfg(target_os = "macos")]
fn main() {
    b5_foreign_driver_delivers_tasks_and_timers_on_the_main_thread();
    println!("test b5_foreign_driver_delivers_tasks_and_timers_on_the_main_thread ... ok");
}

#[cfg(not(target_os = "macos"))]
fn main() {}

#[cfg(target_os = "macos")]
fn b5_foreign_driver_delivers_tasks_and_timers_on_the_main_thread() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{Duration, Instant};

    use core_foundation_sys::runloop::{
        CFRunLoopGetCurrent, CFRunLoopGetMain, CFRunLoopRunInMode, kCFRunLoopDefaultMode,
    };
    use syrp_events::EventLoop;
    use syrp_loop_api::{MessageLoop, Task};

    // SAFETY: both calls only return the run loops of this process; nothing is dereferenced.
    let on_main = unsafe { CFRunLoopGetMain() == CFRunLoopGetCurrent() };
    assert!(on_main, "not the main thread");
    let lp = EventLoop::new().unwrap();
    let log = Arc::new(Mutex::new(Vec::new()));
    let l = Arc::clone(&log);
    lp.post(Task::new(move |_mt| l.lock().unwrap().push("task")));
    let l = Arc::clone(&log);
    let _timer = lp.timer(
        Duration::from_millis(5),
        Box::new(move |_mt| l.lock().unwrap().push("timer")),
    );
    let remote = Arc::new(AtomicBool::new(false));
    let (r, sender) = (Arc::clone(&remote), lp.task_sender());
    thread::spawn(move || sender.post(Task::new(move |_mt| r.store(true, Ordering::SeqCst))))
        .join()
        .unwrap();

    let start = Instant::now();
    let done = || {
        let log = log.lock().unwrap();
        let timers = log.iter().filter(|e| **e == "timer").count();
        log.first() == Some(&"task") && timers >= 3 && remote.load(Ordering::SeqCst)
    };
    while !done() && start.elapsed() < Duration::from_secs(5) {
        // SAFETY: runs this thread's run loop for at most 50 ms, as a foreign driver would.
        unsafe { CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.05, 0) };
    }
    assert!(
        done(),
        "delivered: {:?}, remote: {}",
        log.lock().unwrap(),
        remote.load(Ordering::SeqCst)
    );
}

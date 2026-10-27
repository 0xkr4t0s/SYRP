---
juce: juce::MessageManager
syrp: syrp_events::EventLoop
notes: "callAsync is post, callFunctionOnMessageThread is post_and_wait (an error on the message thread), isThisTheMessageThread is TaskSender::is_message_thread. CallbackMessage and Message/MessageListener become tasks. MessageManagerLock is not provided."
status: done
---

# MessageManager and messages

In JUCE one global `MessageManager` owns the message thread. In Syrp the loop is a value: an
app creates one `syrp_events::EventLoop` on its main thread and runs it. Code that only needs
to reach the message thread takes the `syrp_loop_api::MessageLoop` trait, or a `TaskSender`
when it runs on another thread, so it also runs on `ManualLoop` in tests.

| JUCE | Syrp | Notes |
|---|---|---|
| `MessageManager::getInstance()` | `EventLoop::new()`, then pass `&EventLoop` or a `TaskSender` | No global. One loop per thread; a second is `SYRP-LOOP-0004`. |
| `MessageManager::callAsync (fn)` | `lp.post(Task::new(fn))` or `sender.post(...)` | Never runs inline; tasks from one thread run in post order. |
| `MessageManager::callFunctionOnMessageThread (fn, data)` | `sender.post_and_wait(fn)` | Returns `Result<R, LoopError>`. On the message thread it fails with `SYRP-LOOP-0001` instead of calling `fn` directly. A loop that is dropped first gives `SYRP-LOOP-0003`. |
| `MessageManager::isThisTheMessageThread()` | `sender.is_message_thread()` | Code that must be on the message thread takes a `&MessageThread` token instead of checking. |
| `MessageManager::runDispatchLoop()` | `lp.run()` | `lp.run_for(d)` and `lp.dispatch_pending()` run for a time or one pass. |
| `MessageManager::stopDispatchLoop()` | `lp.quit_handle().quit()` | The `QuitHandle` is `Send + Sync`. |
| `CallbackMessage` (override `messageCallback`, then `post()`) | `post(Task::new(move \|mt\| ...))` | The closure is the callback; it owns what it captures. |
| `Message` and `MessageListener::postMessage` / `handleMessage` | `post` a task that calls the listener's method | See below. |
| `MessageManagerLock` | Not provided | See below. |

## Messages and listeners

A JUCE `MessageListener` receives `Message` objects in `handleMessage` on the message thread.
In Syrp the message is whatever the task captures. A listener that lives on the message thread
can be shared through `Rc<RefCell<_>>`, or reached through an id looked up on the message
thread; the posting thread sends a `Task` that does the lookup and the call. A listener that
has gone away is the task's business: it looks the listener up and does nothing if it is gone.

```rust
use std::sync::mpsc;
use syrp_events::EventLoop;
use syrp_loop_api::{MessageLoop, Task};

let lp = EventLoop::new()?;
let sender = lp.task_sender();
let (tx, rx) = mpsc::channel();
std::thread::spawn(move || {
    // was: listener->postMessage (new MyMessage (42));
    sender.post(Task::new(move |_mt| tx.send(42).unwrap()));
});
while rx.try_recv().is_err() {
    lp.dispatch_pending();
}
# Ok::<(), syrp_events::EventsError>(())
```

## No MessageManagerLock

JUCE's `MessageManagerLock` lets a background thread block the message thread and call
message-thread code directly. Syrp does not provide it (master plan §5.5): the pattern is easy
to deadlock and hides which thread is running UI code. Instead:

- post the work with `post` when the caller does not need the result, or
- use `post_and_wait` when it does; it blocks the calling thread until the task has run on the
  message thread, which is what most `MessageManagerLock` scopes are for, or
- send the result back through a channel or a future.

Message-thread APIs take `&MessageThread`, which only the loop hands out, so code that used to
run under the lock does not compile on another thread.

## Differences from JUCE

- Timers on `EventLoop` that fall behind fire once and keep their phase; they do not catch up
  with a burst (Syrp's choice; `ManualLoop::advance` does fire every missed period).
- VBlank callbacks (`on_vblank`, JUCE's `VBlankAttachment`) run from a 60 Hz fallback timer
  until a display peer takes them over with `EventLoop::vblank_driver`.
- On Linux, peers add their file descriptors with `EventLoop::add_fd`; the loop opens no
  display connection itself.

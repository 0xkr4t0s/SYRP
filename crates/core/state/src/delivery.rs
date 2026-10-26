//! Listener delivery: [`Subscription`] and the loop that hands log records to listeners.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use syrp_loop_api::MessageThread;

use crate::log::Change;
use crate::node::NodeKey;
use crate::store::{Inner, Store};

/// A listener callback, shared so a per-record snapshot can call it without holding a borrow.
pub(crate) type Listener = Rc<RefCell<dyn FnMut(&Store, &MessageThread<'_>, &Change)>>;

/// Keeps a listener registered and its node alive; dropping it unsubscribes.
///
/// ```
/// # use std::{cell::Cell, rc::Rc};
/// # use syrp_state::Store;
/// # use syrp_var::Identifier;
/// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
/// let store = Store::new();
/// let node = store.create(mt, Identifier::new("A"));
/// let heard = Rc::new(Cell::new(0));
/// let h = Rc::clone(&heard);
/// let sub = store.subscribe(mt, &node, move |_, _, _| h.set(h.get() + 1)).unwrap();
/// store.set_property(mt, &node, Identifier::new("x"), 1).unwrap();
/// drop(sub);
/// store.set_property(mt, &node, Identifier::new("x"), 2).unwrap();
/// assert_eq!(heard.get(), 1);
/// # });
/// ```
#[must_use = "dropping a Subscription unsubscribes the listener"]
pub struct Subscription {
    store: Rc<RefCell<Inner>>,
    key: NodeKey,
    id: u64,
}

impl Subscription {
    pub(crate) fn new(store: &Rc<RefCell<Inner>>, key: NodeKey, id: u64) -> Self {
        Self {
            store: Rc::clone(store),
            key,
            id,
        }
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        let removed = {
            let mut inner = self.store.borrow_mut();
            let removed = inner.unsubscribe(self.key, self.id);
            inner.release(self.key);
            removed
        };
        drop(removed); // outside the borrow: the closure may own handles
    }
}

impl fmt::Debug for Subscription {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Subscription").field(&self.key).finish()
    }
}

/// Marks a delivery in progress. Dropping it, normally or while a listener panics, finishes
/// the current record (releasing its pins) and clears the flag, so the store never wedges.
struct Delivering<'a> {
    store: &'a Rc<RefCell<Inner>>,
    current: Option<Change>,
}

impl Delivering<'_> {
    fn finish_record(&mut self) {
        if let Some(change) = self.current.take() {
            let mut inner = self.store.borrow_mut();
            inner.unpin(&change);
            inner.trim();
        }
    }
}

impl Drop for Delivering<'_> {
    fn drop(&mut self) {
        self.finish_record();
        self.store.borrow_mut().delivering = false;
    }
}

impl Store {
    /// Delivers every undelivered record, in order. Returns at once when a delivery is already
    /// running further up the stack: a listener's own mutations are delivered by that outer
    /// loop, after the current record's remaining listeners.
    pub(crate) fn flush(&self, mt: &MessageThread<'_>) {
        let rc = self.rc();
        {
            let mut inner = rc.borrow_mut();
            if inner.delivering {
                return;
            }
            if inner.subscriptions == 0 {
                inner.drain_unobserved(); // B6: nobody listens, so build nothing
                return;
            }
            inner.delivering = true;
        }
        let mut guard = Delivering {
            store: rc,
            current: None,
        };
        // A reused snapshot buffer (B6); lost, harmlessly, if a listener panics.
        let mut targets = std::mem::take(&mut rc.borrow_mut().scratch);
        // No handle or listener is cloned or dropped while `rc` is borrowed: listeners run, and
        // snapshots drop, outside any borrow (`b5_listeners_may_clone_and_drop_handles`).
        loop {
            let change = {
                let mut inner = rc.borrow_mut();
                let Some(change) = inner.take_undelivered() else {
                    break;
                };
                inner.targets(&change, &mut targets);
                change
            };
            guard.current = Some(change);
            let change = guard.current.as_ref().expect("just set");
            for (node, id, listener) in &targets {
                // A listener removed by an earlier one during this record is skipped.
                if !rc.borrow().is_subscribed(*node, *id) {
                    continue;
                }
                if let Ok(mut f) = listener.try_borrow_mut() {
                    f(self, mt, change);
                }
            }
            targets.clear(); // outside the borrow: may drop an unsubscribed listener
            guard.finish_record();
        }
        rc.borrow_mut().scratch = targets;
    }
}

impl Inner {
    /// Snapshots the listeners `change` goes to: the node's own, then each ancestor's upwards;
    /// only the node's own for `ParentChanged`. Ancestors are taken as they are now.
    pub(crate) fn targets(&self, change: &Change, out: &mut Vec<(NodeKey, u64, Listener)>) {
        let own_only = matches!(change, Change::ParentChanged { .. });
        let mut cur = Some(change.node());
        while let Some(key) = cur {
            if let Some(list) = self.listeners.get(key) {
                out.extend(list.iter().map(|(id, l)| (key, *id, Rc::clone(l))));
            }
            cur = if own_only {
                None
            } else {
                self.node(key).parent
            };
        }
    }

    pub(crate) fn is_subscribed(&self, key: NodeKey, id: u64) -> bool {
        self.listeners
            .get(key)
            .is_some_and(|list| list.iter().any(|(i, _)| *i == id))
    }

    /// Removes listener `id` from `key`'s list and returns it, to be dropped outside the borrow.
    pub(crate) fn unsubscribe(&mut self, key: NodeKey, id: u64) -> Option<Listener> {
        let list = self.listeners.get_mut(key)?;
        let at = list.iter().position(|(i, _)| *i == id)?;
        let (_, listener) = list.remove(at);
        self.subscriptions -= 1;
        if list.is_empty() {
            self.listeners.remove(key);
        }
        Some(listener)
    }
}

impl Inner {
    /// Marks every undelivered record delivered and releases its pins, without building
    /// events. Used when the store has no subscriptions.
    fn drain_unobserved(&mut self) {
        while self.delivered < self.next_seq {
            let first = self
                .log
                .front()
                .expect("undelivered records are retained")
                .seq();
            let change =
                self.log[usize::try_from(self.delivered - first).expect("in range")].change();
            let (node, child) = (change.node(), change.child());
            self.delivered += 1;
            self.release(node);
            if let Some(child) = child {
                self.release(child);
            }
        }
        self.trim();
    }
}

//! The change log: [`Change`], [`LogRecord`] and [`LogCursor`].

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use slotmap::DefaultKey;
use syrp_var::{Identifier, Var};

use crate::node::NodeKey;
use crate::store::Inner;

/// One change to a store, with what is needed to reverse it. Nodes are named by [`NodeKey`];
/// [`Store::upgrade`](crate::Store::upgrade) builds a handle when one is needed.
///
/// ```
/// use syrp_state::{Change, Store};
/// use syrp_var::Identifier;
///
/// syrp_loop_api::ManualLoop::new().with_token(|mt| {
///     let store = Store::new();
///     let mut cursor = store.cursor();
///     let node = store.create(mt, Identifier::new("A"));
///     store.set_property(mt, &node, Identifier::new("x"), 1).unwrap();
///     let record = cursor.next().unwrap();
///     assert!(matches!(record.change(), Change::PropertyChanged { old: None, .. }));
/// });
/// ```
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Change {
    /// Property `id` of `node` went from `old` (`None`: it was absent) to `new`.
    #[non_exhaustive]
    PropertyChanged {
        /// The node.
        node: NodeKey,
        /// The property.
        id: Identifier,
        /// The previous value, or `None` if the property was added.
        old: Option<Var>,
        /// The value it has now.
        new: Var,
    },
    /// Property `id` of `node`, whose value was `old`, was removed.
    #[non_exhaustive]
    PropertyRemoved {
        /// The node.
        node: NodeKey,
        /// The property.
        id: Identifier,
        /// The value it had.
        old: Var,
    },
    /// `child` was inserted into `parent`'s children at `index`.
    #[non_exhaustive]
    ChildAdded {
        /// The parent.
        parent: NodeKey,
        /// The new child.
        child: NodeKey,
        /// Its index after insertion.
        index: usize,
    },
    /// `child` was removed from `parent`'s children, where it was at `index`.
    #[non_exhaustive]
    ChildRemoved {
        /// The former parent.
        parent: NodeKey,
        /// The removed child.
        child: NodeKey,
        /// The index it had.
        index: usize,
    },
    /// The child of `parent` at index `from` moved to index `to`.
    #[non_exhaustive]
    ChildMoved {
        /// The parent.
        parent: NodeKey,
        /// The old index.
        from: usize,
        /// The new index.
        to: usize,
    },
    /// `node` was added to or removed from a parent. Only `node`'s own listeners hear it.
    #[non_exhaustive]
    ParentChanged {
        /// The node.
        node: NodeKey,
    },
}

impl Change {
    /// The node the change happened in: the node, or the parent for child changes. Delivery
    /// starts its walk up the tree here.
    ///
    /// ```
    /// # use syrp_state::{Change, Store};
    /// # use syrp_var::Identifier;
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let store = Store::new();
    /// let mut cursor = store.cursor();
    /// let (p, c) = (store.create(mt, Identifier::new("P")), store.create(mt, Identifier::new("C")));
    /// store.add_child(mt, &p, &c, None).unwrap();
    /// assert_eq!(cursor.next().unwrap().change().node(), p.key()); // ChildAdded
    /// assert_eq!(cursor.next().unwrap().change().node(), c.key()); // ParentChanged
    /// # });
    /// ```
    #[must_use]
    pub fn node(&self) -> NodeKey {
        match *self {
            Self::PropertyChanged { node, .. }
            | Self::PropertyRemoved { node, .. }
            | Self::ParentChanged { node } => node,
            Self::ChildAdded { parent, .. }
            | Self::ChildRemoved { parent, .. }
            | Self::ChildMoved { parent, .. } => parent,
        }
    }

    /// The child a child change names, if any.
    pub(crate) fn child(&self) -> Option<NodeKey> {
        match *self {
            Self::ChildAdded { child, .. } | Self::ChildRemoved { child, .. } => Some(child),
            _ => None,
        }
    }
}

/// A [`Change`] with its sequence number, which rises by one per record and never repeats.
///
/// ```
/// # use syrp_state::Store;
/// # use syrp_var::Identifier;
/// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
/// let store = Store::new();
/// let mut cursor = store.cursor();
/// let node = store.create(mt, Identifier::new("A"));
/// store.set_property(mt, &node, Identifier::new("x"), 1).unwrap();
/// store.set_property(mt, &node, Identifier::new("x"), 2).unwrap();
/// let (a, b) = (cursor.next().unwrap(), cursor.next().unwrap());
/// assert_eq!(b.seq(), a.seq() + 1);
/// # });
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct LogRecord {
    seq: u64,
    change: Change,
}

impl LogRecord {
    pub(crate) fn new(seq: u64, change: Change) -> Self {
        Self { seq, change }
    }

    /// The sequence number.
    ///
    /// ```
    /// # use syrp_state::Store;
    /// # use syrp_var::Identifier;
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let store = Store::new();
    /// let mut cursor = store.cursor();
    /// let node = store.create(mt, Identifier::new("A"));
    /// store.set_property(mt, &node, Identifier::new("x"), 1).unwrap();
    /// assert_eq!(cursor.next().unwrap().seq(), 0);
    /// # });
    /// ```
    #[must_use]
    pub fn seq(&self) -> u64 {
        self.seq
    }

    /// What happened.
    ///
    /// ```
    /// # use syrp_state::{Change, Store};
    /// # use syrp_var::Identifier;
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let store = Store::new();
    /// let mut cursor = store.cursor();
    /// let node = store.create(mt, Identifier::new("A"));
    /// store.set_property(mt, &node, Identifier::new("x"), 1).unwrap();
    /// assert_eq!(cursor.next().unwrap().change().node(), node.key());
    /// # });
    /// ```
    #[must_use]
    pub fn change(&self) -> &Change {
        &self.change
    }
}

/// A registered reader of the change log, from [`Store::cursor`](crate::Store::cursor).
///
/// It yields every record appended after it was made, in order. Records stay in the log until
/// every registered cursor has read them; dropping the cursor unregisters it. Without cursors,
/// records are dropped once delivered.
///
/// ```
/// # use syrp_state::Store;
/// # use syrp_var::Identifier;
/// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
/// let store = Store::new();
/// let cursor = store.cursor();
/// let node = store.create(mt, Identifier::new("A"));
/// store.set_property(mt, &node, Identifier::new("x"), 1).unwrap();
/// assert_eq!(store.log_len(), 1); // kept for the cursor
/// drop(cursor);
/// assert_eq!(store.log_len(), 0);
/// # });
/// ```
pub struct LogCursor {
    store: Rc<RefCell<Inner>>,
    key: DefaultKey,
}

impl LogCursor {
    pub(crate) fn new(store: &Rc<RefCell<Inner>>, key: DefaultKey) -> Self {
        Self {
            store: Rc::clone(store),
            key,
        }
    }
}

impl Iterator for LogCursor {
    type Item = LogRecord;

    fn next(&mut self) -> Option<LogRecord> {
        self.store.borrow_mut().read_cursor(self.key)
    }
}

impl Drop for LogCursor {
    fn drop(&mut self) {
        let mut inner = self.store.borrow_mut();
        inner.cursors.remove(self.key);
        inner.trim();
    }
}

impl fmt::Debug for LogCursor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("LogCursor").field(&self.key).finish()
    }
}

impl Inner {
    /// Appends a record and pins the nodes it names until it is delivered.
    pub(crate) fn push(&mut self, change: Change) {
        self.retain(change.node());
        if let Some(child) = change.child() {
            self.retain(child);
        }
        self.log.push_back(LogRecord::new(self.next_seq, change));
        self.next_seq += 1;
    }

    /// Releases the pins of a delivered record.
    pub(crate) fn unpin(&mut self, change: &Change) {
        self.release(change.node());
        if let Some(child) = change.child() {
            self.release(child);
        }
    }

    /// Drops records that are delivered and read by every registered cursor.
    pub(crate) fn trim(&mut self) {
        let keep_from = self.cursors.values().fold(self.delivered, |m, &c| m.min(c));
        while self.log.front().is_some_and(|r| r.seq() < keep_from) {
            self.log.pop_front();
        }
    }

    /// The oldest undelivered change, now counted as delivered. Its pins stay until `unpin`.
    pub(crate) fn take_undelivered(&mut self) -> Option<Change> {
        let first = self.log.front()?.seq();
        let record = self
            .log
            .get(usize::try_from(self.delivered - first).ok()?)?;
        let change = record.change().clone();
        self.delivered += 1;
        Some(change)
    }

    /// The next record for cursor `key`, which then moves past it.
    pub(crate) fn read_cursor(&mut self, key: DefaultKey) -> Option<LogRecord> {
        let at = *self.cursors.get(key)?;
        let first = self.log.front()?.seq();
        let record = self.log.get(usize::try_from(at - first).ok()?)?.clone();
        self.cursors[key] += 1;
        self.trim();
        Some(record)
    }
}

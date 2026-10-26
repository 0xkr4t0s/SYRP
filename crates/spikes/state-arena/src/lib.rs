//! Throwaway spike for WP-0.2-15: can a slotmap arena, with a change log as the source of truth
//! for observers, reproduce the documented semantics of JUCE's `ValueTree`?
//!
//! The answer and the measurements are in `docs/design/0005-state-arena-spike.md`. This crate
//! exists to back that write-up with tests; it is not an API anyone should depend on.
//!
//! The model, its reference-counting rule and its delivery rule are described in the write-up
//! and in this crate's `AGENTS.md`. Everything is `!Send`: message thread only.
//!
//! ```
//! use syrp_state_arena_spike::{Store, Var};
//! let store = Store::new();
//! let root = store.create(store.id("Root"));
//! let child = store.create(store.id("Child"));
//! assert!(root.add_child(&child, None));
//! child.set_property(store.id("gain"), Var::Double(0.5));
//! assert_eq!(root.child(0).unwrap().property(store.id("gain")), Some(Var::Double(0.5)));
//! ```

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use slotmap::{SlotMap, new_key_type};

new_key_type! {
    /// Generational key of a node in a [`Store`]'s arena (the escape hatch of ADR 0003 D1).
    pub struct NodeKey;
}

/// An interned identifier: a node type or a property name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Id(u32);

/// A property value. Deliberately small; the real `Var` is specified by the `syrp-state` WP.
#[derive(Clone, Debug, PartialEq)]
pub enum Var {
    /// A boolean.
    Bool(bool),
    /// A 64-bit integer.
    Int(i64),
    /// A 64-bit float.
    Double(f64),
    /// A shared string.
    Str(Rc<str>),
}

/// What a change-log record says happened to its node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// A property was set to a new value or removed.
    PropertyChanged(Id),
    /// A child was inserted; its key and its index after insertion.
    ChildAdded(NodeKey, usize),
    /// A child was removed; its key and the index it had.
    ChildRemoved(NodeKey, usize),
    /// This node was added to, or removed from, a parent.
    ParentChanged,
}

/// One change-log entry: the node the operation applies to (the parent, for child operations).
#[derive(Clone, Copy)]
struct Record {
    node: NodeKey,
    op: Op,
}

impl Record {
    fn child(&self) -> Option<NodeKey> {
        match self.op {
            Op::ChildAdded(child, _) | Op::ChildRemoved(child, _) => Some(child),
            _ => None,
        }
    }
}

/// A notification passed to listeners.
pub struct Event {
    /// The tree the change happened in: the node whose property changed, or the parent whose
    /// child list changed, or the node whose parent changed.
    pub tree: TreeRef,
    /// What happened.
    pub kind: EventKind,
}

/// The kind of an [`Event`], mirroring the `ValueTree::Listener` callbacks.
pub enum EventKind {
    /// `valueTreePropertyChanged`.
    PropertyChanged(Id),
    /// `valueTreeChildAdded`.
    ChildAdded(TreeRef),
    /// `valueTreeChildRemoved`, with the child's former index.
    ChildRemoved(TreeRef, usize),
    /// `valueTreeParentChanged`: only for listeners on the node itself.
    ParentChanged,
}

type Listener = Rc<dyn Fn(&Event)>;

struct Node {
    type_id: Id,
    props: Vec<(Id, Var)>,
    children: Vec<NodeKey>,
    parent: Option<NodeKey>,
    refs: u32,
}

#[derive(Default)]
struct Inner {
    nodes: SlotMap<NodeKey, Node>,
    names: Vec<Box<str>>,
    ids: HashMap<Box<str>, Id>,
    log: Vec<Record>,
    delivered: usize,
    delivering: bool,
    listeners: HashMap<NodeKey, Vec<(u64, Listener)>>,
    next_listener: u64,
}

impl Inner {
    fn retain(&mut self, key: NodeKey) {
        self.nodes[key].refs += 1;
    }

    /// Drops one count; reclaims the node (and, transitively, orphaned children) at zero.
    fn release(&mut self, key: NodeKey) {
        self.nodes[key].refs -= 1;
        if self.nodes[key].refs > 0 {
            return;
        }
        let mut stack = vec![key];
        while let Some(k) = stack.pop() {
            for c in self.nodes.remove(k).expect("live node").children {
                let child = &mut self.nodes[c];
                child.parent = None;
                child.refs -= 1;
                if child.refs == 0 {
                    stack.push(c);
                }
            }
        }
    }

    /// Releases the pins a delivered record held.
    fn unpin(&mut self, rec: Record) {
        self.release(rec.node);
        if let Some(c) = rec.child() {
            self.release(c);
        }
    }

    /// Appends a record and pins the nodes it names until it is delivered.
    fn push(&mut self, node: NodeKey, op: Op) {
        let rec = Record { node, op };
        self.retain(node);
        if let Some(c) = rec.child() {
            self.retain(c);
        }
        self.log.push(rec);
    }

    fn handle(&mut self, rc: &Rc<RefCell<Inner>>, key: NodeKey) -> TreeRef {
        self.retain(key);
        TreeRef {
            store: rc.clone(),
            key,
        }
    }

    fn is_ancestor_or_self(&self, maybe: NodeKey, of: NodeKey) -> bool {
        let mut cur = Some(of);
        while let Some(k) = cur {
            if k == maybe {
                return true;
            }
            cur = self.nodes[k].parent;
        }
        false
    }
}

/// Delivers every undelivered record. Returns at once if a delivery is already running further
/// up the stack, so a listener's own mutation is delivered by the outer loop, after it returns.
fn flush(rc: &Rc<RefCell<Inner>>) {
    {
        let mut inner = rc.borrow_mut();
        if inner.delivering {
            return;
        }
        inner.delivering = true;
    }
    loop {
        let (rec, event, targets) = {
            let mut inner = rc.borrow_mut();
            if inner.delivered == inner.log.len() {
                inner.delivering = false;
                return;
            }
            let rec = inner.log[inner.delivered];
            inner.delivered += 1;
            // Listener lists are snapshotted here, per record: a listener added while this
            // record is being delivered is not called for it (an open question, see the write-up).
            let mut targets: Vec<(NodeKey, u64, Listener)> = Vec::new();
            let mut cur = Some(rec.node);
            while let Some(k) = cur {
                if let Some(ls) = inner.listeners.get(&k) {
                    targets.extend(ls.iter().map(|(id, l)| (k, *id, l.clone())));
                }
                cur = if rec.op == Op::ParentChanged {
                    None
                } else {
                    inner.nodes[k].parent
                };
            }
            if targets.is_empty() {
                inner.unpin(rec); // fast path: nobody is listening, so build no event
                continue;
            }
            let tree = inner.handle(rc, rec.node);
            let kind = match rec.op {
                Op::PropertyChanged(id) => EventKind::PropertyChanged(id),
                Op::ChildAdded(child, _) => EventKind::ChildAdded(inner.handle(rc, child)),
                Op::ChildRemoved(c, index) => EventKind::ChildRemoved(inner.handle(rc, c), index),
                Op::ParentChanged => EventKind::ParentChanged,
            };
            (rec, Event { tree, kind }, targets)
        };
        for (node, id, listener) in targets {
            // Skip a listener that an earlier listener removed during this delivery.
            let live = rc
                .borrow()
                .listeners
                .get(&node)
                .is_some_and(|ls| ls.iter().any(|(i, _)| *i == id));
            if live {
                listener(&event);
            }
        }
        drop(event);
        rc.borrow_mut().unpin(rec);
    }
}

/// A state document: the arena, the identifier interner, the change log and the listeners.
///
/// ```
/// # use syrp_state_arena_spike::{Store, Var};
/// let store = Store::new();
/// let n = store.create(store.id("Synth"));
/// assert_eq!(store.name(n.type_id()), "Synth");
/// n.set_property(store.id("x"), Var::Int(1));
/// assert_eq!((store.live_nodes(), store.log_len()), (1, 1));
/// store.compact_log();
/// drop(n);
/// assert_eq!((store.live_nodes(), store.log_len()), (0, 0));
/// ```
#[derive(Clone, Default)]
pub struct Store {
    inner: Rc<RefCell<Inner>>,
}

impl Store {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Interns `name`, returning the same [`Id`] for the same string.
    pub fn id(&self, name: &str) -> Id {
        let mut inner = self.inner.borrow_mut();
        if let Some(id) = inner.ids.get(name) {
            return *id;
        }
        let id = Id(u32::try_from(inner.names.len()).expect("fewer than 2^32 identifiers"));
        inner.names.push(name.into());
        inner.ids.insert(name.into(), id);
        id
    }

    /// The string an [`Id`] was interned from.
    pub fn name(&self, id: Id) -> String {
        self.inner.borrow().names[id.0 as usize].to_string()
    }

    /// Creates a parentless node of type `type_id` and returns the only handle to it.
    pub fn create(&self, type_id: Id) -> TreeRef {
        let key = self.inner.borrow_mut().nodes.insert(Node {
            type_id,
            props: Vec::new(),
            children: Vec::new(),
            parent: None,
            refs: 1,
        });
        TreeRef {
            store: self.inner.clone(),
            key,
        }
    }

    /// Number of occupied arena slots.
    pub fn live_nodes(&self) -> usize {
        self.inner.borrow().nodes.len()
    }

    /// Number of records currently held in the change log.
    pub fn log_len(&self) -> usize {
        self.inner.borrow().log.len()
    }

    /// The most recent change-log record, if any.
    pub fn last_record(&self) -> Option<(NodeKey, Op)> {
        self.inner.borrow().log.last().map(|r| (r.node, r.op))
    }

    /// Drops delivered records from the log. A real store hands them to undo and sync first.
    pub fn compact_log(&self) {
        let mut inner = self.inner.borrow_mut();
        let d = inner.delivered;
        inner.log.drain(..d);
        inner.delivered = 0;
    }
}

/// A shallow, reference-counted handle to a node: the ValueTree shape over the arena.
///
/// ```
/// # use syrp_state_arena_spike::{Store, Var};
/// let s = Store::new();
/// let a = s.create(s.id("A"));
/// let b = a.clone(); // same node, not a copy
/// b.set_property(s.id("x"), Var::Int(3));
/// assert_eq!(a.property(s.id("x")), Some(Var::Int(3)));
/// assert_eq!((a.key(), a.type_id(), a.num_properties()), (b.key(), s.id("A"), 1));
/// assert_eq!(a.reference_count(), 2);
/// let root = s.create(s.id("Root"));
/// root.add_child(&a, None);
/// assert_eq!((root.num_children(), root.child(0).as_ref()), (1, Some(&a)));
/// assert_eq!(a.parent().as_ref(), Some(&root));
/// assert!(a.remove_property(s.id("x")) && !a.remove_property(s.id("x")));
/// assert!(root.remove_child(&a) && a.parent().is_none());
/// ```
pub struct TreeRef {
    store: Rc<RefCell<Inner>>,
    key: NodeKey,
}

impl Clone for TreeRef {
    fn clone(&self) -> Self {
        self.store.borrow_mut().retain(self.key);
        TreeRef {
            store: self.store.clone(),
            key: self.key,
        }
    }
}

impl Drop for TreeRef {
    fn drop(&mut self) {
        self.store.borrow_mut().release(self.key);
    }
}

impl PartialEq for TreeRef {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.store, &other.store) && self.key == other.key
    }
}

impl fmt::Debug for TreeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TreeRef({:?})", self.key)
    }
}

impl TreeRef {
    /// The node's arena key.
    pub fn key(&self) -> NodeKey {
        self.key
    }

    /// The node's type identifier.
    #[doc(alias = "getType")]
    pub fn type_id(&self) -> Id {
        self.store.borrow().nodes[self.key].type_id
    }

    /// The node's reference count: handles, parent link and undelivered log records.
    #[doc(alias = "getReferenceCount")]
    pub fn reference_count(&self) -> u32 {
        self.store.borrow().nodes[self.key].refs
    }

    /// Reads a property.
    #[doc(alias = "getProperty")]
    pub fn property(&self, name: Id) -> Option<Var> {
        let inner = self.store.borrow();
        let props = &inner.nodes[self.key].props;
        props
            .binary_search_by_key(&name, |(k, _)| *k)
            .ok()
            .map(|i| props[i].1.clone())
    }

    /// Number of properties.
    #[doc(alias = "getNumProperties")]
    pub fn num_properties(&self) -> usize {
        self.store.borrow().nodes[self.key].props.len()
    }

    /// Sets a property. Setting the value it already has changes nothing and logs nothing.
    ///
    /// ```
    /// # use syrp_state_arena_spike::{Store, Var};
    /// let s = Store::new();
    /// let a = s.create(s.id("A"));
    /// a.set_property(s.id("x"), Var::Int(1));
    /// a.set_property(s.id("x"), Var::Int(1));
    /// assert_eq!(s.log_len(), 1);
    /// ```
    #[doc(alias = "setProperty")]
    pub fn set_property(&self, name: Id, value: Var) {
        {
            let mut inner = self.store.borrow_mut();
            let props = &mut inner.nodes[self.key].props;
            match props.binary_search_by_key(&name, |(k, _)| *k) {
                Ok(i) if props[i].1 == value => return,
                Ok(i) => props[i].1 = value,
                Err(i) => props.insert(i, (name, value)),
            }
            inner.push(self.key, Op::PropertyChanged(name));
        }
        flush(&self.store);
    }

    /// Removes a property. Returns `false`, logging nothing, if it was not present.
    #[doc(alias = "removeProperty")]
    pub fn remove_property(&self, name: Id) -> bool {
        {
            let mut inner = self.store.borrow_mut();
            let props = &mut inner.nodes[self.key].props;
            let Ok(i) = props.binary_search_by_key(&name, |(k, _)| *k) else {
                return false;
            };
            props.remove(i);
            inner.push(self.key, Op::PropertyChanged(name));
        }
        flush(&self.store);
        true
    }

    /// The parent, or `None` for a root or detached node.
    #[doc(alias = "getParent")]
    pub fn parent(&self) -> Option<TreeRef> {
        let mut inner = self.store.borrow_mut();
        let p = inner.nodes[self.key].parent?;
        Some(inner.handle(&self.store, p))
    }

    /// The child at `index`, or `None` if out of range.
    #[doc(alias = "getChild")]
    pub fn child(&self, index: usize) -> Option<TreeRef> {
        let mut inner = self.store.borrow_mut();
        let c = *inner.nodes[self.key].children.get(index)?;
        Some(inner.handle(&self.store, c))
    }

    /// Number of children.
    #[doc(alias = "getNumChildren")]
    pub fn num_children(&self) -> usize {
        self.store.borrow().nodes[self.key].children.len()
    }

    /// Inserts `child` at `index` (`None`, or past the end, appends).
    ///
    /// Returns `false` and changes nothing if `child` belongs to another store, already has a
    /// parent, or is this node or one of its ancestors. JUCE asserts in the first two cases.
    ///
    /// ```
    /// # use syrp_state_arena_spike::Store;
    /// let s = Store::new();
    /// let (a, b) = (s.create(s.id("A")), s.create(s.id("B")));
    /// assert!(a.add_child(&b, None));
    /// assert!(!a.add_child(&b, None)); // already parented
    /// assert!(!b.add_child(&a, None)); // would make a cycle
    /// ```
    #[doc(alias = "addChild")]
    #[doc(alias = "appendChild")]
    pub fn add_child(&self, child: &TreeRef, index: Option<usize>) -> bool {
        if !Rc::ptr_eq(&self.store, &child.store) {
            return false;
        }
        {
            let mut inner = self.store.borrow_mut();
            if inner.nodes[child.key].parent.is_some()
                || inner.is_ancestor_or_self(child.key, self.key)
            {
                return false;
            }
            let children = &mut inner.nodes[self.key].children;
            let index = index.unwrap_or(usize::MAX).min(children.len());
            children.insert(index, child.key);
            let c = &mut inner.nodes[child.key];
            c.parent = Some(self.key);
            c.refs += 1; // the parent link
            inner.push(self.key, Op::ChildAdded(child.key, index));
            inner.push(child.key, Op::ParentChanged);
        }
        flush(&self.store);
        true
    }

    /// Removes `child` from this node. Returns `false` if it is not a child of this node.
    ///
    /// The child stays alive while any handle refers to it.
    #[doc(alias = "removeChild")]
    pub fn remove_child(&self, child: &TreeRef) -> bool {
        if !Rc::ptr_eq(&self.store, &child.store) {
            return false;
        }
        {
            let mut inner = self.store.borrow_mut();
            let children = &mut inner.nodes[self.key].children;
            let Some(index) = children.iter().position(|k| *k == child.key) else {
                return false;
            };
            children.remove(index);
            inner.nodes[child.key].parent = None;
            // Log first: the record pins the child, so dropping the parent link cannot reclaim it.
            inner.push(self.key, Op::ChildRemoved(child.key, index));
            inner.push(child.key, Op::ParentChanged);
            inner.release(child.key);
        }
        flush(&self.store);
        true
    }

    /// Registers a listener for changes to this node and its descendants (and to this node's
    /// own parent). The listener lives until the returned [`Subscription`] is dropped, and the
    /// subscription keeps the node alive.
    ///
    /// ```
    /// # use syrp_state_arena_spike::{Store, Var};
    /// # use std::{cell::Cell, rc::Rc};
    /// let s = Store::new();
    /// let a = s.create(s.id("A"));
    /// let n = Rc::new(Cell::new(0));
    /// let n2 = n.clone();
    /// let sub = a.subscribe(move |_| n2.set(n2.get() + 1));
    /// a.set_property(s.id("x"), Var::Int(1));
    /// drop(sub);
    /// a.set_property(s.id("x"), Var::Int(2));
    /// assert_eq!(n.get(), 1);
    /// ```
    #[doc(alias = "addListener")]
    pub fn subscribe(&self, listener: impl Fn(&Event) + 'static) -> Subscription {
        let mut inner = self.store.borrow_mut();
        let id = inner.next_listener;
        inner.next_listener += 1;
        inner
            .listeners
            .entry(self.key)
            .or_default()
            .push((id, Rc::new(listener)));
        let tree = inner.handle(&self.store, self.key);
        Subscription { tree, id }
    }
}

/// Keeps a listener registered; dropping it unsubscribes (`removeListener`).
#[must_use = "dropping a Subscription unsubscribes the listener"]
pub struct Subscription {
    tree: TreeRef,
    id: u64,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        let removed = {
            let mut inner = self.tree.store.borrow_mut();
            let ls = inner.listeners.get_mut(&self.tree.key);
            let removed = ls.and_then(|ls| {
                let i = ls.iter().position(|(id, _)| *id == self.id)?;
                Some(ls.remove(i))
            });
            if inner
                .listeners
                .get(&self.tree.key)
                .is_some_and(Vec::is_empty)
            {
                inner.listeners.remove(&self.tree.key);
            }
            removed
        };
        // Dropped outside the borrow: the closure may own TreeRefs.
        drop(removed);
    }
}

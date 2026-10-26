//! [`Store`]: the node arena and every operation on it.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use std::collections::VecDeque;

use slotmap::{DefaultKey, SecondaryMap, SlotMap};
use syrp_loop_api::MessageThread;
use syrp_var::{Identifier, Var};

use crate::delivery::{Listener, Subscription};
use crate::error::{StateError, StateErrorKind};
use crate::log::{Change, LogCursor, LogRecord};
use crate::node::{NodeKey, NodeRef, Properties};

/// One arena slot.
pub(crate) struct Node {
    pub(crate) node_type: Identifier,
    /// Sorted by identifier, no duplicates.
    pub(crate) props: Vec<(Identifier, Var)>,
    pub(crate) children: Vec<NodeKey>,
    pub(crate) parent: Option<NodeKey>,
    /// Handles + subscriptions + parent link + undelivered records naming the node.
    pub(crate) refs: u32,
}

/// The shared interior of a [`Store`]. No borrow of it is held while a [`NodeRef`] is dropped.
#[derive(Default)]
pub(crate) struct Inner {
    pub(crate) nodes: SlotMap<NodeKey, Node>,
    /// Scratch stack for freeing subtrees, kept to reuse its allocation.
    free_stack: Vec<NodeKey>,
    /// Retained records, oldest first, with consecutive sequence numbers.
    pub(crate) log: VecDeque<LogRecord>,
    /// Sequence number of the next record appended.
    pub(crate) next_seq: u64,
    /// Sequence number of the first record not yet delivered.
    pub(crate) delivered: u64,
    /// Whether a delivery loop is running.
    pub(crate) delivering: bool,
    /// Each registered cursor's next sequence number.
    pub(crate) cursors: SlotMap<DefaultKey, u64>,
    /// Listeners per node, in registration order, with their ids.
    pub(crate) listeners: SecondaryMap<NodeKey, Vec<(u64, Listener)>>,
    pub(crate) next_listener: u64,
    /// Live subscriptions in the store; zero lets delivery skip building events.
    pub(crate) subscriptions: usize,
    /// Reused per-record listener snapshot.
    pub(crate) scratch: Vec<(NodeKey, u64, Listener)>,
    /// First sequence number an `UndoManager` is capturing from, during a `*_with_undo` call.
    pub(crate) capture_from: Option<u64>,
    /// Records captured for that call, each counting the nodes it names.
    pub(crate) captured: Vec<Change>,
    /// Whether an `UndoManager` is replaying a transaction on this store.
    pub(crate) undoing: bool,
}

impl Inner {
    /// The live node at `key`. Callers hold a count on it, so it is present.
    pub(crate) fn node(&self, key: NodeKey) -> &Node {
        &self.nodes[key]
    }

    pub(crate) fn node_mut(&mut self, key: NodeKey) -> &mut Node {
        &mut self.nodes[key]
    }

    pub(crate) fn retain(&mut self, key: NodeKey) {
        let node = self.node_mut(key);
        node.refs = node.refs.checked_add(1).expect("reference count overflow");
    }

    /// Drops one count. At zero the slot is freed, and each child loses its parent link and the
    /// count that link held, which may free it in turn.
    pub(crate) fn release(&mut self, key: NodeKey) {
        let node = self.node_mut(key);
        node.refs -= 1;
        if node.refs > 0 {
            return;
        }
        let mut stack = std::mem::take(&mut self.free_stack);
        stack.push(key);
        while let Some(dead) = stack.pop() {
            let Some(node) = self.nodes.remove(dead) else {
                continue;
            };
            for child in node.children {
                let c = self.node_mut(child);
                c.parent = None;
                c.refs -= 1;
                if c.refs == 0 {
                    stack.push(child);
                }
            }
        }
        self.free_stack = stack;
    }

    /// Whether `maybe` is `of` or one of its ancestors.
    pub(crate) fn is_self_or_ancestor(&self, maybe: NodeKey, of: NodeKey) -> bool {
        let mut cur = Some(of);
        while let Some(k) = cur {
            if k == maybe {
                return true;
            }
            cur = self.node(k).parent;
        }
        false
    }

    /// Removes `parent`'s child at `index`, logs the removal (the records pin the child) and
    /// then drops the parent link's count.
    fn detach(&mut self, parent: NodeKey, index: usize) {
        let child = self.node_mut(parent).children.remove(index);
        self.node_mut(child).parent = None;
        self.push(Change::ChildRemoved {
            parent,
            child,
            index,
        });
        self.push(Change::ParentChanged { node: child });
        self.release(child);
    }

    pub(crate) fn find_prop(&self, key: NodeKey, id: Identifier) -> Result<usize, usize> {
        self.node(key).props.binary_search_by_key(&id, |(k, _)| *k)
    }
}

/// A state document: the node arena behind every [`NodeRef`] made from it.
///
/// One store per document. It is `!Send` and `!Sync`, and every mutation takes the
/// [`MessageThread`] token. Each node has a type [`Identifier`], properties kept sorted by
/// identifier, and an ordered list of children. A node lives while it is counted: one count per
/// live [`NodeRef`] and one for its parent link. When the count reaches zero the slot is freed
/// and each child loses its parent link and that link's count, so a detached subtree lives
/// while something holds it and is reclaimed after.
///
/// Calls that take a node return [`StateErrorKind::ForeignNode`] for a node of another store.
/// The store is unchanged after any error.
///
/// ```
/// use syrp_state::Store;
/// use syrp_var::Identifier;
///
/// syrp_loop_api::ManualLoop::new().with_token(|mt| {
///     let store = Store::new();
///     let root = store.create(mt, Identifier::new("Root"));
///     let child = store.create(mt, Identifier::new("Child"));
///     store.add_child(mt, &root, &child, None).unwrap();
///     drop(child);
///     assert_eq!(store.live_nodes(), 2); // the parent link keeps the child
///     drop(root);
///     assert_eq!(store.live_nodes(), 0);
/// });
/// ```
#[doc(alias = "ValueTree")]
#[derive(Default)]
pub struct Store {
    inner: Rc<RefCell<Inner>>,
}

impl fmt::Debug for Store {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Store")
            .field("live_nodes", &self.live_nodes())
            .finish()
    }
}

impl Store {
    /// An empty store.
    ///
    /// ```
    /// assert_eq!(syrp_state::Store::new().live_nodes(), 0);
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The key of `node` if it belongs to this store.
    pub(crate) fn own(&self, node: &NodeRef) -> Result<NodeKey, StateError> {
        if Rc::ptr_eq(&self.inner, &node.store) {
            Ok(node.key)
        } else {
            Err(StateError::new(StateErrorKind::ForeignNode))
        }
    }

    /// A new handle to the live node `key`, inside an existing borrow.
    fn handle(&self, inner: &mut Inner, key: NodeKey) -> NodeRef {
        inner.retain(key);
        NodeRef::adopt(&self.inner, key)
    }

    /// Reads from the node behind `node`.
    fn read<T>(
        &self,
        node: &NodeRef,
        f: impl FnOnce(&Inner, NodeKey) -> T,
    ) -> Result<T, StateError> {
        let key = self.own(node)?;
        Ok(f(&self.inner.borrow(), key))
    }

    /// Reads a key from the node behind `node` and returns a handle to it.
    fn link(
        &self,
        node: &NodeRef,
        f: impl FnOnce(&Inner, NodeKey) -> Option<NodeKey>,
    ) -> Result<Option<NodeRef>, StateError> {
        let key = self.own(node)?;
        let mut inner = self.inner.borrow_mut();
        Ok(f(&inner, key).map(|k| self.handle(&mut inner, k)))
    }

    /// Creates a parentless node with no properties or children and returns the only handle.
    ///
    /// ```
    /// # use syrp_var::{Identifier, Var}; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let node = store.create(mt, Identifier::new("Synth"));
    /// assert_eq!(store.node_type(&node).unwrap(), Identifier::new("Synth"));
    /// # });
    /// ```
    pub fn create(&self, _mt: &MessageThread<'_>, node_type: Identifier) -> NodeRef {
        let key = self.inner.borrow_mut().nodes.insert(Node {
            node_type,
            props: Vec::new(),
            children: Vec::new(),
            parent: None,
            refs: 1,
        });
        NodeRef::adopt(&self.inner, key)
    }

    /// A handle to the node `key` names, or [`StateErrorKind::DeadKey`] if it has been freed.
    ///
    /// A [`NodeKey`] carries no store identity, so this cannot detect a key issued by another
    /// store: such a key gives `DeadKey` or, if it happens to match a live slot here, a handle
    /// to an unrelated node. Upgrade keys only on the store that issued them.
    ///
    /// ```
    /// # use syrp_var::{Identifier, Var}; use syrp_state::StateErrorKind; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let key = store.create(mt, Identifier::new("A")).key(); // handle dropped at once
    /// assert_eq!(store.upgrade(key).unwrap_err().kind(), StateErrorKind::DeadKey);
    /// # });
    /// ```
    pub fn upgrade(&self, key: NodeKey) -> Result<NodeRef, StateError> {
        let mut inner = self.inner.borrow_mut();
        if !inner.nodes.contains_key(key) {
            return Err(StateError::new(StateErrorKind::DeadKey));
        }
        Ok(self.handle(&mut inner, key))
    }

    /// Number of occupied arena slots, for tests and diagnostics.
    ///
    /// ```
    /// # use syrp_var::{Identifier, Var}; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let a = store.create(mt, Identifier::new("A"));
    /// assert_eq!(store.live_nodes(), 1);
    /// # drop(a);
    /// # });
    /// ```
    #[must_use]
    pub fn live_nodes(&self) -> usize {
        self.inner.borrow().nodes.len()
    }

    /// The node's count: its live handles plus one if it has a parent.
    ///
    /// ```
    /// # use syrp_var::{Identifier, Var}; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let a = store.create(mt, Identifier::new("A"));
    /// let b = a.clone();
    /// assert_eq!(store.reference_count(&b).unwrap(), 2);
    /// # });
    /// ```
    #[doc(alias = "getReferenceCount")]
    pub fn reference_count(&self, node: &NodeRef) -> Result<u32, StateError> {
        self.read(node, |i, k| i.node(k).refs)
    }

    /// The node's type identifier.
    ///
    /// ```
    /// # use syrp_var::{Identifier, Var}; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let node = store.create(mt, Identifier::new("Voice"));
    /// assert_eq!(store.node_type(&node).unwrap().as_str(), "Voice");
    /// # });
    /// ```
    #[doc(alias = "getType")]
    pub fn node_type(&self, node: &NodeRef) -> Result<Identifier, StateError> {
        self.read(node, |i, k| i.node(k).node_type)
    }

    /// The value of property `id`, or `None` if the node does not have it.
    ///
    /// ```
    /// # use syrp_var::{Identifier, Var}; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let node = store.create(mt, Identifier::new("A"));
    /// let gain = Identifier::new("gain");
    /// assert_eq!(store.property(&node, gain).unwrap(), None);
    /// store.set_property(mt, &node, gain, 0.5).unwrap();
    /// assert_eq!(store.property(&node, gain).unwrap(), Some(Var::from(0.5)));
    /// # });
    /// ```
    #[doc(alias = "getProperty")]
    pub fn property(&self, node: &NodeRef, id: Identifier) -> Result<Option<Var>, StateError> {
        self.read(node, |i, k| {
            let at = i.find_prop(k, id).ok()?;
            Some(i.node(k).props[at].1.clone())
        })
    }

    /// Whether the node has property `id`.
    ///
    /// ```
    /// # use syrp_var::{Identifier, Var}; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let node = store.create(mt, Identifier::new("A"));
    /// store.set_property(mt, &node, Identifier::new("x"), 1).unwrap();
    /// assert!(store.has_property(&node, Identifier::new("x")).unwrap());
    /// # });
    /// ```
    #[doc(alias = "hasProperty")]
    pub fn has_property(&self, node: &NodeRef, id: Identifier) -> Result<bool, StateError> {
        self.read(node, |i, k| i.find_prop(k, id).is_ok())
    }

    /// Number of properties on the node.
    ///
    /// ```
    /// # use syrp_var::{Identifier, Var}; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let node = store.create(mt, Identifier::new("A"));
    /// assert_eq!(store.num_properties(&node).unwrap(), 0);
    /// # });
    /// ```
    #[doc(alias = "getNumProperties")]
    pub fn num_properties(&self, node: &NodeRef) -> Result<usize, StateError> {
        self.read(node, |i, k| i.node(k).props.len())
    }

    /// Iterates over the node's properties in identifier order (see [`Properties`]).
    ///
    /// ```
    /// # use syrp_var::{Identifier, Var}; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let node = store.create(mt, Identifier::new("A"));
    /// store.set_property(mt, &node, Identifier::new("x"), 1).unwrap();
    /// assert_eq!(store.properties(&node).unwrap().count(), 1);
    /// # });
    /// ```
    #[doc(alias = "getPropertyName")]
    pub fn properties(&self, node: &NodeRef) -> Result<Properties, StateError> {
        self.own(node)?;
        Ok(Properties::new(node.clone()))
    }

    /// Sets property `id` to `value`, adding it if absent. Setting the value it already has
    /// (same variant and equal, [`Var::eq_same_type`]) changes nothing.
    ///
    /// ```
    /// # use syrp_var::{Identifier, Var}; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let node = store.create(mt, Identifier::new("A"));
    /// store.set_property(mt, &node, Identifier::new("name"), "lead").unwrap();
    /// assert_eq!(store.property(&node, Identifier::new("name")).unwrap(), Some(Var::from("lead")));
    /// # });
    /// ```
    #[doc(alias = "setProperty")]
    pub fn set_property(
        &self,
        mt: &MessageThread<'_>,
        node: &NodeRef,
        id: Identifier,
        value: impl Into<Var>,
    ) -> Result<(), StateError> {
        let key = self.own(node)?;
        let value = value.into();
        let mut inner = self.inner.borrow_mut();
        let at = inner.find_prop(key, id);
        let props = &mut inner.node_mut(key).props;
        let new = value.clone();
        let old = match at {
            Ok(i) if props[i].1.eq_same_type(&value) => return Ok(()),
            Ok(i) => Some(std::mem::replace(&mut props[i].1, value)),
            Err(i) => {
                props.insert(i, (id, value));
                None
            }
        };
        inner.push(Change::PropertyChanged {
            node: key,
            id,
            old,
            new,
        });
        drop(inner);
        self.flush(mt);
        Ok(())
    }

    /// Removes property `id` and returns its old value, or `None` if it was absent.
    ///
    /// ```
    /// # use syrp_var::{Identifier, Var}; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let node = store.create(mt, Identifier::new("A"));
    /// store.set_property(mt, &node, Identifier::new("x"), 1).unwrap();
    /// assert_eq!(store.remove_property(mt, &node, Identifier::new("x")).unwrap(), Some(Var::from(1)));
    /// assert_eq!(store.remove_property(mt, &node, Identifier::new("x")).unwrap(), None);
    /// # });
    /// ```
    #[doc(alias = "removeProperty")]
    pub fn remove_property(
        &self,
        mt: &MessageThread<'_>,
        node: &NodeRef,
        id: Identifier,
    ) -> Result<Option<Var>, StateError> {
        let key = self.own(node)?;
        let mut inner = self.inner.borrow_mut();
        let Ok(at) = inner.find_prop(key, id) else {
            return Ok(None);
        };
        let old = inner.node_mut(key).props.remove(at).1;
        inner.push(Change::PropertyRemoved {
            node: key,
            id,
            old: old.clone(),
        });
        drop(inner);
        self.flush(mt);
        Ok(Some(old))
    }

    /// The node's parent, or `None` for a root or detached node.
    ///
    /// ```
    /// # use syrp_var::{Identifier, Var}; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let (p, c) = (store.create(mt, Identifier::new("P")), store.create(mt, Identifier::new("C")));
    /// store.add_child(mt, &p, &c, None).unwrap();
    /// assert_eq!(store.parent(&c).unwrap(), Some(p.clone()));
    /// assert_eq!(store.parent(&p).unwrap(), None);
    /// # });
    /// ```
    #[doc(alias = "getParent")]
    pub fn parent(&self, node: &NodeRef) -> Result<Option<NodeRef>, StateError> {
        self.link(node, |i, k| i.node(k).parent)
    }

    /// The child at `index`, or `None` if out of range.
    ///
    /// ```
    /// # use syrp_var::{Identifier, Var}; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let (p, c) = (store.create(mt, Identifier::new("P")), store.create(mt, Identifier::new("C")));
    /// store.add_child(mt, &p, &c, None).unwrap();
    /// assert_eq!(store.child(&p, 0).unwrap(), Some(c.clone()));
    /// assert_eq!(store.child(&p, 1).unwrap(), None);
    /// # });
    /// ```
    #[doc(alias = "getChild")]
    pub fn child(&self, node: &NodeRef, index: usize) -> Result<Option<NodeRef>, StateError> {
        self.link(node, |i, k| i.node(k).children.get(index).copied())
    }

    /// Number of children.
    ///
    /// ```
    /// # use syrp_var::{Identifier, Var}; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let p = store.create(mt, Identifier::new("P"));
    /// assert_eq!(store.num_children(&p).unwrap(), 0);
    /// # });
    /// ```
    #[doc(alias = "getNumChildren")]
    pub fn num_children(&self, node: &NodeRef) -> Result<usize, StateError> {
        self.read(node, |i, k| i.node(k).children.len())
    }

    /// The index of `child` among `parent`'s children, or `None` if it is not one of them.
    ///
    /// ```
    /// # use syrp_var::{Identifier, Var}; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let (p, c) = (store.create(mt, Identifier::new("P")), store.create(mt, Identifier::new("C")));
    /// assert_eq!(store.index_of(&p, &c).unwrap(), None);
    /// store.add_child(mt, &p, &c, None).unwrap();
    /// assert_eq!(store.index_of(&p, &c).unwrap(), Some(0));
    /// # });
    /// ```
    #[doc(alias = "indexOf")]
    pub fn index_of(&self, parent: &NodeRef, child: &NodeRef) -> Result<Option<usize>, StateError> {
        let c = self.own(child)?;
        self.read(parent, |i, k| {
            i.node(k).children.iter().position(|x| *x == c)
        })
    }

    /// Inserts `child` into `parent`'s children at `index`. `None`, or an index past the end,
    /// appends.
    ///
    /// Fails with [`StateErrorKind::ChildHasParent`] if `child` already has a parent and
    /// [`StateErrorKind::WouldCreateCycle`] if `child` is `parent` or one of its ancestors.
    ///
    /// ```
    /// # use syrp_var::{Identifier, Var}; use syrp_state::StateErrorKind; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let (p, c) = (store.create(mt, Identifier::new("P")), store.create(mt, Identifier::new("C")));
    /// store.add_child(mt, &p, &c, Some(99)).unwrap(); // appends
    /// let err = store.add_child(mt, &p, &c, None).unwrap_err();
    /// assert_eq!(err.kind(), StateErrorKind::ChildHasParent);
    /// # });
    /// ```
    #[doc(alias = "addChild")]
    #[doc(alias = "appendChild")]
    pub fn add_child(
        &self,
        mt: &MessageThread<'_>,
        parent: &NodeRef,
        child: &NodeRef,
        index: Option<usize>,
    ) -> Result<(), StateError> {
        let p = self.own(parent)?;
        let c = self.own(child)?;
        let mut inner = self.inner.borrow_mut();
        if inner.node(c).parent.is_some() {
            return Err(StateError::new(StateErrorKind::ChildHasParent));
        }
        if inner.is_self_or_ancestor(c, p) {
            return Err(StateError::new(StateErrorKind::WouldCreateCycle));
        }
        let children = &mut inner.node_mut(p).children;
        let at = index.map_or(children.len(), |i| i.min(children.len()));
        children.insert(at, c);
        inner.node_mut(c).parent = Some(p);
        inner.retain(c); // the parent link
        inner.push(Change::ChildAdded {
            parent: p,
            child: c,
            index: at,
        });
        inner.push(Change::ParentChanged { node: c });
        drop(inner);
        self.flush(mt);
        Ok(())
    }

    /// Removes `child` from `parent`. The child lives on while anything else counts it.
    ///
    /// Fails with [`StateErrorKind::NotAChild`] if `child` is not a child of `parent`.
    ///
    /// ```
    /// # use syrp_var::{Identifier, Var}; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let (p, c) = (store.create(mt, Identifier::new("P")), store.create(mt, Identifier::new("C")));
    /// store.add_child(mt, &p, &c, None).unwrap();
    /// store.remove_child(mt, &p, &c).unwrap();
    /// assert_eq!(store.parent(&c).unwrap(), None);
    /// # });
    /// ```
    #[doc(alias = "removeChild")]
    pub fn remove_child(
        &self,
        mt: &MessageThread<'_>,
        parent: &NodeRef,
        child: &NodeRef,
    ) -> Result<(), StateError> {
        let p = self.own(parent)?;
        let c = self.own(child)?;
        let mut inner = self.inner.borrow_mut();
        let Some(at) = inner.node(p).children.iter().position(|k| *k == c) else {
            return Err(StateError::new(StateErrorKind::NotAChild));
        };
        inner.detach(p, at);
        drop(inner);
        self.flush(mt);
        Ok(())
    }

    pub(crate) fn rc(&self) -> &Rc<RefCell<Inner>> {
        &self.inner
    }

    /// Whether an [`UndoManager`](crate::UndoManager) is undoing or redoing on this store, so
    /// a listener can tell a replayed change from a new one.
    ///
    /// ```
    /// assert!(!syrp_state::Store::new().is_performing_undo_redo());
    /// ```
    #[doc(alias = "isPerformingUndoRedo")]
    #[must_use]
    pub fn is_performing_undo_redo(&self) -> bool {
        self.inner.borrow().undoing
    }

    /// Another `Store` value over the same document.
    pub(crate) fn from_rc(inner: &Rc<RefCell<Inner>>) -> Self {
        Self {
            inner: Rc::clone(inner),
        }
    }

    /// Registers a [`LogCursor`] that reads every record appended from now on.
    ///
    /// ```
    /// # use syrp_var::{Identifier, Var}; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let mut cursor = store.cursor();
    /// let node = store.create(mt, Identifier::new("A"));
    /// store.set_property(mt, &node, Identifier::new("x"), 1).unwrap();
    /// assert!(cursor.next().is_some() && cursor.next().is_none());
    /// # });
    /// ```
    pub fn cursor(&self) -> LogCursor {
        let mut inner = self.inner.borrow_mut();
        let at = inner.next_seq;
        LogCursor::new(&self.inner, inner.cursors.insert(at))
    }

    /// Number of records the log currently retains (undelivered, or unread by a cursor).
    ///
    /// ```
    /// assert_eq!(syrp_state::Store::new().log_len(), 0);
    /// ```
    #[must_use]
    pub fn log_len(&self) -> usize {
        self.inner.borrow().log.len()
    }

    /// Registers `listener` for changes to `node` and its descendants, and to `node`'s own
    /// parent. It is called with the store, the token and the change, after the mutation that
    /// made it, and may mutate the store itself; those mutations are delivered after the current
    /// change's remaining listeners. The returned [`Subscription`] counts toward the node.
    ///
    /// Order (not yet checked against JUCE): the node's listeners in registration order, then
    /// each ancestor's, upwards. A listener added during a delivery hears the next change; one
    /// removed during a delivery is not called again.
    ///
    /// If a listener panics, the panic propagates out of the mutating call. The record being
    /// delivered is finished, but the records after it stay undelivered, and keep their counts
    /// on the nodes they name, until the next mutation on this store delivers them.
    ///
    /// ```
    /// # use syrp_var::{Identifier, Var}; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let (p, c) = (store.create(mt, Identifier::new("P")), store.create(mt, Identifier::new("C")));
    /// store.add_child(mt, &p, &c, None).unwrap();
    /// let seen = std::rc::Rc::new(std::cell::Cell::new(None));
    /// let s = seen.clone();
    /// let _sub = store.subscribe(mt, &p, move |_, _, ch| s.set(Some(ch.node()))).unwrap();
    /// store.set_property(mt, &c, Identifier::new("x"), 1).unwrap();
    /// assert_eq!(seen.get(), Some(c.key())); // heard on the ancestor
    /// # });
    /// ```
    #[doc(alias = "addListener")]
    pub fn subscribe(
        &self,
        _mt: &MessageThread<'_>,
        node: &NodeRef,
        listener: impl FnMut(&Store, &MessageThread<'_>, &Change) + 'static,
    ) -> Result<Subscription, StateError> {
        let key = self.own(node)?;
        let mut inner = self.inner.borrow_mut();
        let id = inner.next_listener;
        inner.next_listener += 1;
        inner.subscriptions += 1;
        let listener: Listener = Rc::new(RefCell::new(listener));
        let list = inner.listeners.entry(key).expect("live node");
        list.or_default().push((id, listener));
        inner.retain(key);
        Ok(Subscription::new(&self.inner, key, id))
    }

    /// Removes every property of the node, logging one `PropertyRemoved` per property.
    ///
    /// ```
    /// # use syrp_var::{Identifier, Var}; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let node = store.create(mt, Identifier::new("A"));
    /// store.set_property(mt, &node, Identifier::new("x"), 1).unwrap();
    /// store.remove_all_properties(mt, &node).unwrap();
    /// assert_eq!(store.num_properties(&node).unwrap(), 0);
    /// # });
    /// ```
    #[doc(alias = "removeAllProperties")]
    pub fn remove_all_properties(
        &self,
        mt: &MessageThread<'_>,
        node: &NodeRef,
    ) -> Result<(), StateError> {
        let key = self.own(node)?;
        let mut inner = self.inner.borrow_mut();
        for (id, old) in std::mem::take(&mut inner.node_mut(key).props) {
            inner.push(Change::PropertyRemoved { node: key, id, old });
        }
        drop(inner);
        self.flush(mt);
        Ok(())
    }

    /// Removes every child, last first. Children nothing else counts are freed once delivered.
    ///
    /// ```
    /// # use syrp_var::{Identifier, Var}; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let p = store.create(mt, Identifier::new("P"));
    /// store.add_child(mt, &p, &store.create(mt, Identifier::new("C")), None).unwrap();
    /// store.remove_all_children(mt, &p).unwrap();
    /// assert_eq!(store.live_nodes(), 1);
    /// # });
    /// ```
    #[doc(alias = "removeAllChildren")]
    pub fn remove_all_children(
        &self,
        mt: &MessageThread<'_>,
        node: &NodeRef,
    ) -> Result<(), StateError> {
        let key = self.own(node)?;
        let mut inner = self.inner.borrow_mut();
        for index in (0..inner.node(key).children.len()).rev() {
            inner.detach(key, index);
        }
        drop(inner);
        self.flush(mt);
        Ok(())
    }

    /// Moves the child at `from` to index `to`; a `to` past the end moves it to the end.
    ///
    /// Fails with [`StateErrorKind::IndexOutOfRange`] if `from` is not a child index.
    ///
    /// ```
    /// # use syrp_var::{Identifier, Var}; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let p = store.create(mt, Identifier::new("P"));
    /// let a = store.create(mt, Identifier::new("A"));
    /// store.add_child(mt, &p, &a, None).unwrap();
    /// store.add_child(mt, &p, &store.create(mt, Identifier::new("B")), None).unwrap();
    /// store.move_child(mt, &p, 0, 9).unwrap();
    /// assert_eq!(store.index_of(&p, &a).unwrap(), Some(1));
    /// # });
    /// ```
    #[doc(alias = "moveChild")]
    pub fn move_child(
        &self,
        mt: &MessageThread<'_>,
        parent: &NodeRef,
        from: usize,
        to: usize,
    ) -> Result<(), StateError> {
        let key = self.own(parent)?;
        let mut inner = self.inner.borrow_mut();
        let children = &mut inner.node_mut(key).children;
        if from >= children.len() {
            return Err(StateError::new(StateErrorKind::IndexOutOfRange));
        }
        let to = to.min(children.len() - 1);
        if from == to {
            return Ok(());
        }
        let moved = children.remove(from);
        children.insert(to, moved);
        inner.push(Change::ChildMoved {
            parent: key,
            from,
            to,
        });
        drop(inner);
        self.flush(mt);
        Ok(())
    }

    /// The first child whose type is `node_type`.
    ///
    /// ```
    /// # use syrp_var::{Identifier, Var}; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let (p, c) = (store.create(mt, Identifier::new("P")), store.create(mt, Identifier::new("C")));
    /// store.add_child(mt, &p, &c, None).unwrap();
    /// assert_eq!(store.child_with_type(&p, Identifier::new("C")).unwrap(), Some(c.clone()));
    /// # });
    /// ```
    #[doc(alias = "getChildWithName")]
    pub fn child_with_type(
        &self,
        node: &NodeRef,
        node_type: Identifier,
    ) -> Result<Option<NodeRef>, StateError> {
        self.link(node, |i, k| {
            let children = &i.node(k).children;
            children
                .iter()
                .copied()
                .find(|c| i.node(*c).node_type == node_type)
        })
    }

    /// The first child whose property `id` equals `value` by `Var`'s strict `==`: the same
    /// variant and value, except that `i32`, `i64` and `f64` compare by exact numeric value and
    /// objects by identity. It is not `loose_eq` (`"1"` does not match `1`).
    ///
    /// ```
    /// # use syrp_var::{Identifier, Var}; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let (p, c) = (store.create(mt, Identifier::new("P")), store.create(mt, Identifier::new("C")));
    /// store.add_child(mt, &p, &c, None).unwrap();
    /// store.set_property(mt, &c, Identifier::new("id"), 7).unwrap();
    /// let found = store.child_with_property(&p, Identifier::new("id"), &Var::from(7)).unwrap();
    /// assert_eq!(found, Some(c.clone()));
    /// # });
    /// ```
    #[doc(alias = "getChildWithProperty")]
    pub fn child_with_property(
        &self,
        node: &NodeRef,
        id: Identifier,
        value: &Var,
    ) -> Result<Option<NodeRef>, StateError> {
        self.link(node, |i, k| {
            let children = &i.node(k).children;
            let has = |c: NodeKey| {
                i.find_prop(c, id)
                    .is_ok_and(|at| i.node(c).props[at].1 == *value)
            };
            children.iter().copied().find(|c| has(*c))
        })
    }

    /// The topmost ancestor of the node, or the node itself if it has no parent.
    ///
    /// ```
    /// # use syrp_var::{Identifier, Var}; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let (p, c) = (store.create(mt, Identifier::new("P")), store.create(mt, Identifier::new("C")));
    /// store.add_child(mt, &p, &c, None).unwrap();
    /// assert_eq!(store.root(&c).unwrap(), p);
    /// # });
    /// ```
    #[doc(alias = "getRoot")]
    pub fn root(&self, node: &NodeRef) -> Result<NodeRef, StateError> {
        let found = self.link(node, |i, mut k| {
            while let Some(p) = i.node(k).parent {
                k = p;
            }
            Some(k)
        })?;
        Ok(found.expect("the walk always ends on a node"))
    }

    /// Whether `ancestor` is a parent of `node` at any depth. A node is not its own ancestor.
    ///
    /// ```
    /// # use syrp_var::{Identifier, Var}; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let (p, c) = (store.create(mt, Identifier::new("P")), store.create(mt, Identifier::new("C")));
    /// store.add_child(mt, &p, &c, None).unwrap();
    /// assert!(store.is_ancestor_of(&p, &c).unwrap() && !store.is_ancestor_of(&c, &c).unwrap());
    /// # });
    /// ```
    #[doc(alias = "isAChildOf")]
    pub fn is_ancestor_of(&self, ancestor: &NodeRef, node: &NodeRef) -> Result<bool, StateError> {
        let a = self.own(ancestor)?;
        self.read(node, |i, k| {
            let parent = i.node(k).parent;
            parent.is_some_and(|p| i.is_self_or_ancestor(a, p))
        })
    }
}

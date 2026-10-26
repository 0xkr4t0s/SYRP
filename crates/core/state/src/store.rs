//! [`Store`]: the node arena and every operation on it.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use slotmap::SlotMap;
use syrp_loop_api::MessageThread;
use syrp_var::{Identifier, Var};

use crate::error::{StateError, StateErrorKind};
use crate::node::{NodeKey, NodeRef, Properties};

/// One arena slot.
pub(crate) struct Node {
    node_type: Identifier,
    /// Sorted by identifier, no duplicates.
    pub(crate) props: Vec<(Identifier, Var)>,
    children: Vec<NodeKey>,
    parent: Option<NodeKey>,
    /// Handles + parent link (+ subscriptions and pending records, in later increments).
    refs: u32,
}

/// The shared interior of a [`Store`]. No borrow of it is held while a [`NodeRef`] is dropped.
#[derive(Default)]
pub(crate) struct Inner {
    nodes: SlotMap<NodeKey, Node>,
    /// Scratch stack for freeing subtrees, kept to reuse its allocation.
    free_stack: Vec<NodeKey>,
}

impl Inner {
    /// The live node at `key`. Callers hold a count on it, so it is present.
    pub(crate) fn node(&self, key: NodeKey) -> &Node {
        &self.nodes[key]
    }

    fn node_mut(&mut self, key: NodeKey) -> &mut Node {
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
    fn is_self_or_ancestor(&self, maybe: NodeKey, of: NodeKey) -> bool {
        let mut cur = Some(of);
        while let Some(k) = cur {
            if k == maybe {
                return true;
            }
            cur = self.node(k).parent;
        }
        false
    }

    fn find_prop(&self, key: NodeKey, id: Identifier) -> Result<usize, usize> {
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
    fn own(&self, node: &NodeRef) -> Result<NodeKey, StateError> {
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
        _mt: &MessageThread<'_>,
        node: &NodeRef,
        id: Identifier,
        value: impl Into<Var>,
    ) -> Result<(), StateError> {
        let key = self.own(node)?;
        let value = value.into();
        let mut inner = self.inner.borrow_mut();
        let at = inner.find_prop(key, id);
        let props = &mut inner.node_mut(key).props;
        let old = match at {
            Ok(i) if props[i].1.eq_same_type(&value) => return Ok(()),
            Ok(i) => Some(std::mem::replace(&mut props[i].1, value)),
            Err(i) => {
                props.insert(i, (id, value));
                None
            }
        };
        drop(inner);
        drop(old);
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
        _mt: &MessageThread<'_>,
        node: &NodeRef,
        id: Identifier,
    ) -> Result<Option<Var>, StateError> {
        let key = self.own(node)?;
        let mut inner = self.inner.borrow_mut();
        let Ok(at) = inner.find_prop(key, id) else {
            return Ok(None);
        };
        Ok(Some(inner.node_mut(key).props.remove(at).1))
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
        _mt: &MessageThread<'_>,
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
        _mt: &MessageThread<'_>,
        parent: &NodeRef,
        child: &NodeRef,
    ) -> Result<(), StateError> {
        let p = self.own(parent)?;
        let c = self.own(child)?;
        let mut inner = self.inner.borrow_mut();
        let children = &mut inner.node_mut(p).children;
        let Some(at) = children.iter().position(|k| *k == c) else {
            return Err(StateError::new(StateErrorKind::NotAChild));
        };
        children.remove(at);
        inner.node_mut(c).parent = None;
        inner.release(c); // `child` is a handle, so this never frees it
        Ok(())
    }
}

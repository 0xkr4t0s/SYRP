//! [`ValueTree`]: a JUCE-shaped handle to a node of a `syrp-state` [`Store`].

use std::fmt;
use std::rc::Rc;

use syrp_loop_api::MessageThread;
use syrp_state::{NodeKey, NodeRef, StateError, Store, UndoManager};
use syrp_var::{Identifier, Var};

thread_local! {
    /// The store that [`ValueTree::new`] creates nodes in: one per thread, so all trees made
    /// on the message thread can be joined together.
    static DOCUMENT: Rc<Store> = Rc::new(Store::new());
}

/// A JUCE-shaped handle to a node in a [`Store`] (ADR 0003 D1). It owns no data.
///
/// Like JUCE's `ValueTree`, it is a lightweight reference: [`Clone`] makes another reference
/// to the same node, [`ValueTree::deep_clone`] copies the subtree, `==` is identity and
/// [`ValueTree::is_equivalent_to`] compares content. A detached subtree lives while any
/// reference to it exists. A default `ValueTree` is invalid: it refers to nothing, reads give
/// empty results and mutations do nothing.
///
/// Mutations take the [`MessageThread`] token and JUCE's optional undo manager as
/// `Option<&mut UndoManager>`, and return the store's [`StateError`] where JUCE would assert
/// (adding a child that has a parent, or one from another store). [`ValueTree::handle`] gives
/// the native key, to move code to the `syrp-state` API one piece at a time.
///
/// ```
/// use syrp_compat_state::ValueTree;
/// use syrp_state::UndoManager;
///
/// syrp_loop_api::ManualLoop::new().with_token(|mt| {
///     let synth = ValueTree::new(mt, "Synth");
///     let mut undo = UndoManager::new();
///     synth.set_property(mt, "cutoff", 1200.0, Some(&mut undo)).unwrap();
///     let voice = ValueTree::new(mt, "Voice");
///     synth.append_child(mt, &voice, None).unwrap();
///     assert_eq!(voice.parent(), synth);
///     undo.undo(mt).unwrap();
///     assert!(!synth.has_property("cutoff"));
/// });
/// ```
#[doc(alias = "juce::ValueTree")]
#[derive(Clone, Default, PartialEq, Eq, Hash)]
pub struct ValueTree {
    node: Option<Node>,
}

/// A valid tree's store and node.
#[derive(Clone)]
struct Node {
    store: Rc<Store>,
    node: NodeRef,
}

impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        self.node == other.node
    }
}

impl Eq for Node {}

impl std::hash::Hash for Node {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.node.hash(state);
    }
}

impl fmt::Debug for ValueTree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.node {
            Some(n) => write!(f, "ValueTree({}, {:?})", self.node_type(), n.node.key()),
            None => f.write_str("ValueTree(invalid)"),
        }
    }
}

impl ValueTree {
    /// A new, parentless tree of type `node_type` in this thread's shared store.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let tree = syrp_compat_state::ValueTree::new(mt, "Synth");
    /// assert!(tree.is_valid() && tree.has_type("Synth"));
    /// # });
    /// ```
    pub fn new(mt: &MessageThread<'_>, node_type: impl Into<Identifier>) -> Self {
        DOCUMENT.with(|store| Self::new_in(mt, store, node_type))
    }

    /// A new, parentless tree of type `node_type` in `store`.
    ///
    /// ```
    /// # use std::rc::Rc;
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let store = Rc::new(syrp_state::Store::new());
    /// let tree = syrp_compat_state::ValueTree::new_in(mt, &store, "Doc");
    /// assert_eq!(store.live_nodes(), 1);
    /// # drop(tree);
    /// # });
    /// ```
    pub fn new_in(
        mt: &MessageThread<'_>,
        store: &Rc<Store>,
        node_type: impl Into<Identifier>,
    ) -> Self {
        let node = store.create(mt, node_type.into());
        Self::wrap(store, node)
    }

    /// The tree for the native node `key` of `store`, or an invalid tree if it was freed.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let tree = syrp_compat_state::ValueTree::new(mt, "A");
    /// let again = syrp_compat_state::ValueTree::from_handle(tree.store().unwrap(), tree.handle().unwrap());
    /// assert_eq!(again, tree);
    /// # });
    /// ```
    pub fn from_handle(store: &Rc<Store>, key: NodeKey) -> Self {
        store
            .upgrade(key)
            .map_or_else(|_| Self::default(), |n| Self::wrap(store, n))
    }

    pub(crate) fn wrap(store: &Rc<Store>, node: NodeRef) -> Self {
        let store = Rc::clone(store);
        Self {
            node: Some(Node { store, node }),
        }
    }

    fn wrap_opt(&self, node: Option<NodeRef>) -> Self {
        match (&self.node, node) {
            (Some(n), Some(node)) => Self::wrap(&n.store, node),
            _ => Self::default(),
        }
    }

    /// The native key of the node, for the `syrp-state` API (ADR 0003 D1); `None` if invalid.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let tree = syrp_compat_state::ValueTree::new(mt, "A");
    /// let store = tree.store().unwrap();
    /// let node = store.upgrade(tree.handle().unwrap()).unwrap();
    /// assert_eq!(store.node_type(&node).unwrap(), "A".into());
    /// # });
    /// ```
    #[must_use]
    pub fn handle(&self) -> Option<NodeKey> {
        self.node.as_ref().map(|n| n.node.key())
    }

    /// The native counted handle to the node, if valid.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let tree = syrp_compat_state::ValueTree::new(mt, "A");
    /// assert_eq!(tree.node_ref().unwrap().key(), tree.handle().unwrap());
    /// # });
    /// ```
    #[must_use]
    pub fn node_ref(&self) -> Option<&NodeRef> {
        self.node.as_ref().map(|n| &n.node)
    }

    /// The store the node lives in, if valid.
    ///
    /// ```
    /// assert!(syrp_compat_state::ValueTree::default().store().is_none());
    /// ```
    #[must_use]
    pub fn store(&self) -> Option<&Rc<Store>> {
        self.node.as_ref().map(|n| &n.store)
    }

    /// Whether the tree refers to a node.
    ///
    /// ```
    /// assert!(!syrp_compat_state::ValueTree::default().is_valid());
    /// ```
    #[doc(alias = "isValid")]
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.node.is_some()
    }

    /// Runs a read on the store, or returns `T::default()` for an invalid tree.
    fn read<T: Default>(&self, f: impl FnOnce(&Store, &NodeRef) -> Result<T, StateError>) -> T {
        self.node
            .as_ref()
            .and_then(|n| f(&n.store, &n.node).ok())
            .unwrap_or_default()
    }

    /// Runs a mutation, or does nothing (`Ok`) for an invalid tree.
    fn write(
        &self,
        f: impl FnOnce(&Store, &NodeRef) -> Result<(), StateError>,
    ) -> Result<(), StateError> {
        self.node.as_ref().map_or(Ok(()), |n| f(&n.store, &n.node))
    }

    /// The node's type; empty for an invalid tree.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// assert_eq!(syrp_compat_state::ValueTree::new(mt, "A").node_type().as_str(), "A");
    /// # });
    /// ```
    #[doc(alias = "getType")]
    #[must_use]
    pub fn node_type(&self) -> Identifier {
        self.read(|s, n| s.node_type(n))
    }

    /// Whether the node's type is `node_type`.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// assert!(!syrp_compat_state::ValueTree::new(mt, "A").has_type("B"));
    /// # });
    /// ```
    #[doc(alias = "hasType")]
    #[must_use]
    pub fn has_type(&self, node_type: impl Into<Identifier>) -> bool {
        self.is_valid() && self.node_type() == node_type.into()
    }

    /// The value of property `name`, or a void `Var` if it is absent.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let tree = syrp_compat_state::ValueTree::new(mt, "A");
    /// tree.set_property(mt, "x", 3, None).unwrap();
    /// assert_eq!(tree.property("x"), 3.into());
    /// assert!(tree.property("missing").is_void());
    /// # });
    /// ```
    #[doc(alias = "getProperty")]
    #[must_use]
    pub fn property(&self, name: impl Into<Identifier>) -> Var {
        let id = name.into();
        self.read(|s, n| s.property(n, id).map(Option::unwrap_or_default))
    }

    /// The value of property `name`, or `default` if it is absent.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let tree = syrp_compat_state::ValueTree::new(mt, "A");
    /// assert_eq!(tree.property_or("x", 7.into()), 7.into());
    /// # });
    /// ```
    #[doc(alias = "getProperty")]
    #[must_use]
    pub fn property_or(&self, name: impl Into<Identifier>, default: Var) -> Var {
        let id = name.into();
        self.read(|s, n| s.property(n, id)).unwrap_or(default)
    }

    /// Whether the node has property `name`.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// assert!(!syrp_compat_state::ValueTree::new(mt, "A").has_property("x"));
    /// # });
    /// ```
    #[doc(alias = "hasProperty")]
    #[must_use]
    pub fn has_property(&self, name: impl Into<Identifier>) -> bool {
        let id = name.into();
        self.read(|s, n| s.has_property(n, id))
    }

    /// Number of properties.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// assert_eq!(syrp_compat_state::ValueTree::new(mt, "A").num_properties(), 0);
    /// # });
    /// ```
    #[doc(alias = "getNumProperties")]
    #[must_use]
    pub fn num_properties(&self) -> usize {
        self.read(|s, n| s.num_properties(n))
    }

    /// The name of the property at `index` in the node's (unspecified, stable) property order.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let tree = syrp_compat_state::ValueTree::new(mt, "A");
    /// tree.set_property(mt, "x", 1, None).unwrap();
    /// assert_eq!(tree.property_name(0), Some("x".into()));
    /// # });
    /// ```
    #[doc(alias = "getPropertyName")]
    #[must_use]
    pub fn property_name(&self, index: usize) -> Option<Identifier> {
        self.read(|s, n| Ok(s.properties(n)?.nth(index).map(|(k, _)| k)))
    }

    /// Sets property `name` to `value`, recording it in `undo` if given.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let tree = syrp_compat_state::ValueTree::new(mt, "A");
    /// tree.set_property(mt, "name", "lead", None).unwrap();
    /// assert_eq!(tree.property("name").to_text(), "lead");
    /// # });
    /// ```
    #[doc(alias = "setProperty")]
    pub fn set_property(
        &self,
        mt: &MessageThread<'_>,
        name: impl Into<Identifier>,
        value: impl Into<Var>,
        undo: Option<&mut UndoManager>,
    ) -> Result<(), StateError> {
        let (id, value) = (name.into(), value.into());
        self.write(|s, n| match undo {
            Some(u) => s.set_property_with_undo(mt, u, n, id, value),
            None => s.set_property(mt, n, id, value),
        })
    }

    /// Removes property `name`, recording it in `undo` if given.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let tree = syrp_compat_state::ValueTree::new(mt, "A");
    /// tree.set_property(mt, "x", 1, None).unwrap();
    /// tree.remove_property(mt, "x", None).unwrap();
    /// assert!(!tree.has_property("x"));
    /// # });
    /// ```
    #[doc(alias = "removeProperty")]
    pub fn remove_property(
        &self,
        mt: &MessageThread<'_>,
        name: impl Into<Identifier>,
        undo: Option<&mut UndoManager>,
    ) -> Result<(), StateError> {
        let id = name.into();
        self.write(|s, n| match undo {
            Some(u) => s.remove_property_with_undo(mt, u, n, id).map(drop),
            None => s.remove_property(mt, n, id).map(drop),
        })
    }

    /// Removes every property, recording it in `undo` if given.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let tree = syrp_compat_state::ValueTree::new(mt, "A");
    /// tree.set_property(mt, "x", 1, None).unwrap();
    /// tree.remove_all_properties(mt, None).unwrap();
    /// assert_eq!(tree.num_properties(), 0);
    /// # });
    /// ```
    #[doc(alias = "removeAllProperties")]
    pub fn remove_all_properties(
        &self,
        mt: &MessageThread<'_>,
        undo: Option<&mut UndoManager>,
    ) -> Result<(), StateError> {
        self.write(|s, n| match undo {
            Some(u) => s.remove_all_properties_with_undo(mt, u, n),
            None => s.remove_all_properties(mt, n),
        })
    }

    /// Number of children.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// assert_eq!(syrp_compat_state::ValueTree::new(mt, "A").num_children(), 0);
    /// # });
    /// ```
    #[doc(alias = "getNumChildren")]
    #[must_use]
    pub fn num_children(&self) -> usize {
        self.read(|s, n| s.num_children(n))
    }

    /// The child at `index`, or an invalid tree if out of range.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let tree = syrp_compat_state::ValueTree::new(mt, "A");
    /// assert!(!tree.child(0).is_valid());
    /// # });
    /// ```
    #[doc(alias = "getChild")]
    #[must_use]
    pub fn child(&self, index: usize) -> ValueTree {
        self.wrap_opt(self.read(|s, n| s.child(n, index)))
    }

    /// The children, in order.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let tree = syrp_compat_state::ValueTree::new(mt, "A");
    /// tree.append_child(mt, &syrp_compat_state::ValueTree::new(mt, "B"), None).unwrap();
    /// assert_eq!(tree.children().map(|c| c.node_type()).collect::<Vec<_>>(), ["B".into()]);
    /// # });
    /// ```
    pub fn children(&self) -> Children {
        Children {
            tree: self.clone(),
            next: 0,
        }
    }

    /// The first child of type `node_type`, or an invalid tree.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let tree = syrp_compat_state::ValueTree::new(mt, "A");
    /// assert!(!tree.child_with_name("B").is_valid());
    /// # });
    /// ```
    #[doc(alias = "getChildWithName")]
    #[must_use]
    pub fn child_with_name(&self, node_type: impl Into<Identifier>) -> ValueTree {
        let ty = node_type.into();
        self.wrap_opt(self.read(|s, n| s.child_with_type(n, ty)))
    }

    /// The first child whose property `name` equals `value`, or an invalid tree.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let tree = syrp_compat_state::ValueTree::new(mt, "A");
    /// let b = syrp_compat_state::ValueTree::new(mt, "B");
    /// b.set_property(mt, "id", 2, None).unwrap();
    /// tree.append_child(mt, &b, None).unwrap();
    /// assert_eq!(tree.child_with_property("id", &2.into()), b);
    /// # });
    /// ```
    #[doc(alias = "getChildWithProperty")]
    #[must_use]
    pub fn child_with_property(&self, name: impl Into<Identifier>, value: &Var) -> ValueTree {
        let id = name.into();
        self.wrap_opt(self.read(|s, n| s.child_with_property(n, id, value)))
    }

    /// The first child of type `node_type`, creating and appending one if there is none.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let tree = syrp_compat_state::ValueTree::new(mt, "A");
    /// let b = tree.get_or_create_child_with_name(mt, "B", None).unwrap();
    /// assert_eq!(tree.get_or_create_child_with_name(mt, "B", None).unwrap(), b);
    /// # });
    /// ```
    #[doc(alias = "getOrCreateChildWithName")]
    pub fn get_or_create_child_with_name(
        &self,
        mt: &MessageThread<'_>,
        node_type: impl Into<Identifier>,
        undo: Option<&mut UndoManager>,
    ) -> Result<ValueTree, StateError> {
        let ty = node_type.into();
        let found = self.child_with_name(ty);
        let Some(n) = &self.node else {
            return Ok(found);
        };
        if found.is_valid() {
            return Ok(found);
        }
        let child = Self::new_in(mt, &n.store, ty);
        self.append_child(mt, &child, undo)?;
        Ok(child)
    }

    /// Inserts `child` at `index` (`None`, or past the end, appends), recording it in `undo`.
    /// Fails if `child` already has a parent, is this tree or an ancestor, or is in another
    /// store. Adding an invalid tree does nothing.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let (a, b) = (syrp_compat_state::ValueTree::new(mt, "A"), syrp_compat_state::ValueTree::new(mt, "B"));
    /// a.add_child(mt, &b, Some(0), None).unwrap();
    /// assert!(b.add_child(mt, &a, None, None).is_err()); // a cycle
    /// # });
    /// ```
    #[doc(alias = "addChild")]
    pub fn add_child(
        &self,
        mt: &MessageThread<'_>,
        child: &ValueTree,
        index: Option<usize>,
        undo: Option<&mut UndoManager>,
    ) -> Result<(), StateError> {
        let Some(c) = child.node_ref() else {
            return Ok(());
        };
        self.write(|s, n| match undo {
            Some(u) => s.add_child_with_undo(mt, u, n, c, index),
            None => s.add_child(mt, n, c, index),
        })
    }

    /// Appends `child`; see [`ValueTree::add_child`].
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let (a, b) = (syrp_compat_state::ValueTree::new(mt, "A"), syrp_compat_state::ValueTree::new(mt, "B"));
    /// a.append_child(mt, &b, None).unwrap();
    /// assert_eq!(a.child(0), b);
    /// # });
    /// ```
    #[doc(alias = "appendChild")]
    pub fn append_child(
        &self,
        mt: &MessageThread<'_>,
        child: &ValueTree,
        undo: Option<&mut UndoManager>,
    ) -> Result<(), StateError> {
        self.add_child(mt, child, None, undo)
    }

    /// Removes `child` from this tree, recording it in `undo`. The child lives on while
    /// referenced. Fails if it is not a child of this tree.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let (a, b) = (syrp_compat_state::ValueTree::new(mt, "A"), syrp_compat_state::ValueTree::new(mt, "B"));
    /// a.append_child(mt, &b, None).unwrap();
    /// a.remove_child(mt, &b, None).unwrap();
    /// assert!(!b.parent().is_valid());
    /// # });
    /// ```
    #[doc(alias = "removeChild")]
    pub fn remove_child(
        &self,
        mt: &MessageThread<'_>,
        child: &ValueTree,
        undo: Option<&mut UndoManager>,
    ) -> Result<(), StateError> {
        let Some(c) = child.node_ref() else {
            return Ok(());
        };
        self.write(|s, n| match undo {
            Some(u) => s.remove_child_with_undo(mt, u, n, c),
            None => s.remove_child(mt, n, c),
        })
    }

    /// Removes the child at `index`, recording it in `undo`; out of range does nothing.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let (a, b) = (syrp_compat_state::ValueTree::new(mt, "A"), syrp_compat_state::ValueTree::new(mt, "B"));
    /// a.append_child(mt, &b, None).unwrap();
    /// a.remove_child_at(mt, 0, None).unwrap();
    /// assert_eq!(a.num_children(), 0);
    /// # });
    /// ```
    #[doc(alias = "removeChild")]
    pub fn remove_child_at(
        &self,
        mt: &MessageThread<'_>,
        index: usize,
        undo: Option<&mut UndoManager>,
    ) -> Result<(), StateError> {
        self.remove_child(mt, &self.child(index), undo)
    }

    /// Removes every child, recording it in `undo`.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let a = syrp_compat_state::ValueTree::new(mt, "A");
    /// a.append_child(mt, &syrp_compat_state::ValueTree::new(mt, "B"), None).unwrap();
    /// a.remove_all_children(mt, None).unwrap();
    /// assert_eq!(a.num_children(), 0);
    /// # });
    /// ```
    #[doc(alias = "removeAllChildren")]
    pub fn remove_all_children(
        &self,
        mt: &MessageThread<'_>,
        undo: Option<&mut UndoManager>,
    ) -> Result<(), StateError> {
        self.write(|s, n| match undo {
            Some(u) => s.remove_all_children_with_undo(mt, u, n),
            None => s.remove_all_children(mt, n),
        })
    }

    /// Moves the child at `from` to `to` (past the end moves it to the end), recording it in
    /// `undo`. Fails if `from` is out of range.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let a = syrp_compat_state::ValueTree::new(mt, "A");
    /// let b = syrp_compat_state::ValueTree::new(mt, "B");
    /// a.append_child(mt, &b, None).unwrap();
    /// a.append_child(mt, &syrp_compat_state::ValueTree::new(mt, "C"), None).unwrap();
    /// a.move_child(mt, 0, 1, None).unwrap();
    /// assert_eq!(a.index_of(&b), Some(1));
    /// # });
    /// ```
    #[doc(alias = "moveChild")]
    pub fn move_child(
        &self,
        mt: &MessageThread<'_>,
        from: usize,
        to: usize,
        undo: Option<&mut UndoManager>,
    ) -> Result<(), StateError> {
        self.write(|s, n| match undo {
            Some(u) => s.move_child_with_undo(mt, u, n, from, to),
            None => s.move_child(mt, n, from, to),
        })
    }

    /// The index of `child` among this tree's children.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let (a, b) = (syrp_compat_state::ValueTree::new(mt, "A"), syrp_compat_state::ValueTree::new(mt, "B"));
    /// assert_eq!(a.index_of(&b), None);
    /// # });
    /// ```
    #[doc(alias = "indexOf")]
    #[must_use]
    pub fn index_of(&self, child: &ValueTree) -> Option<usize> {
        let c = child.node_ref()?;
        self.read(|s, n| s.index_of(n, c))
    }

    /// The parent, or an invalid tree for a root or detached tree.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// assert!(!syrp_compat_state::ValueTree::new(mt, "A").parent().is_valid());
    /// # });
    /// ```
    #[doc(alias = "getParent")]
    #[must_use]
    pub fn parent(&self) -> ValueTree {
        self.wrap_opt(self.read(|s, n| s.parent(n)))
    }

    /// The topmost ancestor, or this tree if it has no parent.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let (a, b) = (syrp_compat_state::ValueTree::new(mt, "A"), syrp_compat_state::ValueTree::new(mt, "B"));
    /// a.append_child(mt, &b, None).unwrap();
    /// assert_eq!(b.root(), a);
    /// # });
    /// ```
    #[doc(alias = "getRoot")]
    #[must_use]
    pub fn root(&self) -> ValueTree {
        self.wrap_opt(self.read(|s, n| s.root(n).map(Some)))
    }

    /// Whether `possible_parent` is a parent of this tree at any depth.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let (a, b) = (syrp_compat_state::ValueTree::new(mt, "A"), syrp_compat_state::ValueTree::new(mt, "B"));
    /// a.append_child(mt, &b, None).unwrap();
    /// assert!(b.is_a_child_of(&a) && !a.is_a_child_of(&b));
    /// # });
    /// ```
    #[doc(alias = "isAChildOf")]
    #[must_use]
    pub fn is_a_child_of(&self, possible_parent: &ValueTree) -> bool {
        let Some(p) = possible_parent.node_ref() else {
            return false;
        };
        self.read(|s, n| s.is_ancestor_of(p, n))
    }

    /// The sibling `delta` places away in the parent's children, or an invalid tree.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let a = syrp_compat_state::ValueTree::new(mt, "A");
    /// let (b, c) = (syrp_compat_state::ValueTree::new(mt, "B"), syrp_compat_state::ValueTree::new(mt, "C"));
    /// a.append_child(mt, &b, None).unwrap();
    /// a.append_child(mt, &c, None).unwrap();
    /// assert_eq!((b.sibling(1), c.sibling(-1)), (c.clone(), b.clone()));
    /// assert!(!b.sibling(-1).is_valid());
    /// # });
    /// ```
    #[doc(alias = "getSibling")]
    #[must_use]
    pub fn sibling(&self, delta: isize) -> ValueTree {
        let parent = self.parent();
        let at = parent
            .index_of(self)
            .and_then(|i| i.checked_add_signed(delta));
        at.map(|i| parent.child(i)).unwrap_or_default()
    }

    /// A detached deep copy of this tree's subtree, in the same store.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let a = syrp_compat_state::ValueTree::new(mt, "A");
    /// let copy = a.deep_clone(mt);
    /// assert!(copy != a && copy.is_equivalent_to(&a));
    /// # });
    /// ```
    #[doc(alias = "createCopy")]
    #[must_use]
    pub fn deep_clone(&self, mt: &MessageThread<'_>) -> ValueTree {
        self.wrap_opt(self.read(|s, n| s.deep_copy(mt, n).map(Some)))
    }

    /// Whether both trees have the same type, the same properties (in any order, compared with
    /// `Var`'s `==`) and equivalent children in the same order. Two invalid trees are
    /// equivalent. Trees in different stores are compared the same way, by content.
    ///
    /// ```
    /// # use std::rc::Rc;
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let (a, b) = (syrp_compat_state::ValueTree::new(mt, "A"), syrp_compat_state::ValueTree::new(mt, "A"));
    /// assert!(a.is_equivalent_to(&b) && a != b);
    /// let elsewhere = syrp_compat_state::ValueTree::new_in(mt, &Rc::new(syrp_state::Store::new()), "A");
    /// assert!(a.is_equivalent_to(&elsewhere));
    /// # });
    /// ```
    #[doc(alias = "isEquivalentTo")]
    #[must_use]
    pub fn is_equivalent_to(&self, other: &ValueTree) -> bool {
        match (&self.node, &other.node) {
            (None, None) => true,
            (Some(n), Some(o)) if Rc::ptr_eq(&n.store, &o.store) => {
                n.store.is_equivalent(&n.node, &o.node).unwrap_or(false)
            }
            (Some(_), Some(_)) => Self::equivalent_across_stores(self, other),
            _ => false,
        }
    }

    /// Structural comparison through the public read API, for trees in different stores.
    /// Iterative, so deep trees cannot overflow the stack.
    fn equivalent_across_stores(a: &ValueTree, b: &ValueTree) -> bool {
        let mut stack = vec![(a.clone(), b.clone())];
        while let Some((x, y)) = stack.pop() {
            let n = x.num_properties();
            if x.node_type() != y.node_type()
                || n != y.num_properties()
                || x.num_children() != y.num_children()
            {
                return false;
            }
            let same = (0..n)
                .filter_map(|i| x.property_name(i))
                .all(|id| y.has_property(id) && x.property(id) == y.property(id));
            if !same {
                return false;
            }
            stack.extend(x.children().zip(y.children()));
        }
        true
    }

    /// The node's reference count: references to it, its parent link, the undo history's and
    /// the change log's holds, and listeners.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let a = syrp_compat_state::ValueTree::new(mt, "A");
    /// let b = a.clone();
    /// assert_eq!(b.reference_count(), 2);
    /// # });
    /// ```
    #[doc(alias = "getReferenceCount")]
    #[must_use]
    pub fn reference_count(&self) -> u32 {
        self.read(|s, n| s.reference_count(n))
    }
}

/// Iterator over a tree's children, from [`ValueTree::children`]. It reads the live child list
/// by index, so changes during iteration are seen.
///
/// ```
/// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
/// let a = syrp_compat_state::ValueTree::new(mt, "A");
/// assert_eq!(a.children().count(), 0);
/// # });
/// ```
#[derive(Debug, Clone)]
pub struct Children {
    tree: ValueTree,
    next: usize,
}

impl Iterator for Children {
    type Item = ValueTree;

    fn next(&mut self) -> Option<ValueTree> {
        let child = self.tree.child(self.next);
        self.next += 1;
        child.is_valid().then_some(child)
    }
}

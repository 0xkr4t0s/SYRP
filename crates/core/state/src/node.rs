//! [`NodeKey`], [`NodeRef`] and the [`Properties`] iterator.

use std::cell::RefCell;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use syrp_var::{Identifier, Var};

use crate::store::Inner;

slotmap::new_key_type! {
    /// A `Copy` generational key naming a node in one [`Store`](crate::Store)'s arena.
    ///
    /// A key holds no reference count, so it does not keep its node alive. Turn it into a
    /// [`NodeRef`] with [`Store::upgrade`](crate::Store::upgrade) while the node is alive. Keys
    /// carry no store identity: a key is meaningful only to the store that issued it.
    ///
    /// ```
    /// let null = syrp_state::NodeKey::default(); // names no node
    /// assert!(syrp_state::Store::new().upgrade(null).is_err());
    /// ```
    pub struct NodeKey;
}

/// A counted handle to a node: the store it lives in plus its key.
///
/// Cloning is shallow (a new handle to the same node, one more count) and equality is
/// identity: two handles are equal when they name the same node of the same store. While any
/// handle exists the node stays alive, attached or not. Dropping the last count frees it.
///
/// ```
/// use syrp_state::Store;
/// use syrp_var::Identifier;
///
/// syrp_loop_api::ManualLoop::new().with_token(|mt| {
///     let store = Store::new();
///     let a = store.create(mt, Identifier::new("A"));
///     let b = a.clone();
///     store.set_property(mt, &b, Identifier::new("x"), 3).unwrap();
///     assert_eq!(store.property(&a, Identifier::new("x")).unwrap(), Some(3.into()));
///     assert_eq!(a, b);
///     assert_ne!(a, store.create(mt, Identifier::new("A")));
/// });
/// ```
pub struct NodeRef {
    pub(crate) store: Rc<RefCell<Inner>>,
    pub(crate) key: NodeKey,
}

impl NodeRef {
    /// Makes a handle for a node whose count the caller has already raised.
    pub(crate) fn adopt(store: &Rc<RefCell<Inner>>, key: NodeKey) -> Self {
        Self {
            store: Rc::clone(store),
            key,
        }
    }

    /// The node's key, which holds no count.
    ///
    /// ```
    /// # use syrp_state::Store;
    /// # use syrp_var::Identifier;
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let store = Store::new();
    /// let node = store.create(mt, Identifier::new("A"));
    /// assert_eq!(node.clone().key(), node.key());
    /// # });
    /// ```
    #[must_use]
    pub fn key(&self) -> NodeKey {
        self.key
    }
}

impl Clone for NodeRef {
    fn clone(&self) -> Self {
        self.store.borrow_mut().retain(self.key);
        Self::adopt(&self.store, self.key)
    }
}

impl Drop for NodeRef {
    fn drop(&mut self) {
        self.store.borrow_mut().release(self.key);
    }
}

impl PartialEq for NodeRef {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.store, &other.store) && self.key == other.key
    }
}

impl Eq for NodeRef {}

impl Hash for NodeRef {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Rc::as_ptr(&self.store).hash(state);
        self.key.hash(state);
    }
}

impl fmt::Debug for NodeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("NodeRef").field(&self.key).finish()
    }
}

/// Iterator over a node's properties in identifier order, from
/// [`Store::properties`](crate::Store::properties).
///
/// It holds a handle to the node, so the node outlives it. Each step reads the node's current
/// properties by position: a property set or removed during iteration may be skipped or seen
/// twice, but iteration never fails.
///
/// ```
/// use syrp_state::Store;
/// use syrp_var::{Identifier, Var};
///
/// syrp_loop_api::ManualLoop::new().with_token(|mt| {
///     let store = Store::new();
///     let node = store.create(mt, Identifier::new("A"));
///     store.set_property(mt, &node, Identifier::new("x"), 1).unwrap();
///     let all: Vec<(Identifier, Var)> = store.properties(&node).unwrap().collect();
///     assert_eq!(all, vec![(Identifier::new("x"), Var::from(1))]);
/// });
/// ```
#[derive(Debug)]
pub struct Properties {
    node: NodeRef,
    index: usize,
}

impl Properties {
    pub(crate) fn new(node: NodeRef) -> Self {
        Self { node, index: 0 }
    }
}

impl Iterator for Properties {
    type Item = (Identifier, Var);

    fn next(&mut self) -> Option<Self::Item> {
        let item = self
            .node
            .store
            .borrow()
            .node(self.node.key)
            .props
            .get(self.index)
            .cloned()?;
        self.index += 1;
        Some(item)
    }
}

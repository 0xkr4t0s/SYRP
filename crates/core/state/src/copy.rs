//! [`Store::deep_copy`] and [`Store::is_equivalent`]. Both walk iteratively, never recursively.

use syrp_loop_api::MessageThread;

use crate::error::StateError;
use crate::node::{NodeKey, NodeRef};
use crate::store::{Inner, Node, Store};

impl Inner {
    /// A new parentless slot with `src`'s type and properties, no children and no count.
    fn copy_slot(&mut self, src: NodeKey) -> NodeKey {
        let node = self.node(src);
        let copy = Node {
            node_type: node.node_type,
            props: node.props.clone(),
            children: Vec::new(),
            parent: None,
            refs: 0,
        };
        self.nodes.insert(copy)
    }
}

impl Store {
    /// A detached copy of `node`'s subtree: new nodes with the same types, properties (values
    /// are cloned as `Var` clones them, so arrays and objects stay shared) and children in the
    /// same order. Building the copy appends no log records; it is not attached anywhere.
    ///
    /// ```
    /// # use syrp_var::{Identifier, Var}; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let (p, c) = (store.create(mt, Identifier::new("P")), store.create(mt, Identifier::new("C")));
    /// store.add_child(mt, &p, &c, None).unwrap();
    /// let copy = store.deep_copy(mt, &p).unwrap();
    /// assert!(copy != p && store.is_equivalent(&copy, &p).unwrap());
    /// assert_eq!(store.live_nodes(), 4);
    /// # });
    /// ```
    #[doc(alias = "createCopy")]
    pub fn deep_copy(
        &self,
        _mt: &MessageThread<'_>,
        node: &NodeRef,
    ) -> Result<NodeRef, StateError> {
        let src = self.own(node)?;
        let mut inner = self.rc().borrow_mut();
        let root = inner.copy_slot(src);
        let mut stack = vec![(src, root)];
        while let Some((from, to)) = stack.pop() {
            for i in 0..inner.node(from).children.len() {
                let child = inner.node(from).children[i];
                let copy = inner.copy_slot(child);
                let slot = inner.node_mut(copy);
                (slot.parent, slot.refs) = (Some(to), 1); // the parent link
                inner.node_mut(to).children.push(copy);
                stack.push((child, copy));
            }
        }
        inner.retain(root);
        Ok(NodeRef::adopt(self.rc(), root))
    }

    /// Whether two subtrees are equivalent: the same type, the same properties with equal
    /// values (`Var`'s `==`; property order never matters), and pairwise equivalent children
    /// in the same order. Identity, parents and listeners are ignored.
    ///
    /// ```
    /// # use syrp_var::{Identifier, Var}; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let (a, b) = (store.create(mt, Identifier::new("N")), store.create(mt, Identifier::new("N")));
    /// assert!(store.is_equivalent(&a, &b).unwrap());
    /// store.set_property(mt, &a, Identifier::new("x"), 1).unwrap();
    /// assert!(!store.is_equivalent(&a, &b).unwrap());
    /// # });
    /// ```
    #[doc(alias = "isEquivalentTo")]
    pub fn is_equivalent(&self, a: &NodeRef, b: &NodeRef) -> Result<bool, StateError> {
        let (a, b) = (self.own(a)?, self.own(b)?);
        let inner = self.rc().borrow();
        let mut stack = vec![(a, b)];
        while let Some((x, y)) = stack.pop() {
            let (x, y) = (inner.node(x), inner.node(y));
            if x.node_type != y.node_type
                || x.props != y.props
                || x.children.len() != y.children.len()
            {
                return Ok(false);
            }
            stack.extend(x.children.iter().copied().zip(y.children.iter().copied()));
        }
        Ok(true)
    }
}

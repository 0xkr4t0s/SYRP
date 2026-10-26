//! [`Listener`]: JUCE's `ValueTree::Listener`, fed from the store's change log, and the
//! wrapper-owned registration that follows a [`ValueTree`] on [`ValueTree::assign`].

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use syrp_loop_api::MessageThread;
use syrp_state::{Change, Store, Subscription};
use syrp_var::Identifier;

use crate::tree::{Node, ValueTree};

/// Callbacks for changes to a [`ValueTree`], as JUCE's `ValueTree::Listener`. Every method
/// has an empty default, so implement only the ones you need.
///
/// Delivery comes from the store's change log after the mutating call has applied its change
/// (see `syrp_state::Store::subscribe` for the order): the tree's own listeners in the order they
/// were added, then each ancestor's, upwards. A change made inside a callback is delivered
/// after the current change's remaining listeners. A callback that is running is not entered
/// again: a change it causes that would reach itself through another store is skipped.
///
/// ```
/// use std::{cell::RefCell, rc::Rc};
/// use syrp_compat_state::{Listener, ValueTree};
/// use syrp_var::Identifier;
///
/// #[derive(Default)]
/// struct Count(usize);
/// impl Listener for Count {
///     fn property_changed(&mut self, _: &syrp_loop_api::MessageThread<'_>, _: &ValueTree, _: Identifier) {
///         self.0 += 1;
///     }
/// }
///
/// syrp_loop_api::ManualLoop::new().with_token(|mt| {
///     let mut tree = ValueTree::new(mt, "Synth");
///     let count = Rc::new(RefCell::new(Count::default()));
///     tree.add_listener(mt, &count);
///     tree.set_property(mt, "gain", 0.5, None).unwrap();
///     assert_eq!(count.borrow().0, 1);
/// });
/// ```
#[doc(alias = "ValueTree::Listener")]
#[allow(unused_variables)]
pub trait Listener {
    /// A property of `tree`, or of a descendant at any depth, was set or removed.
    ///
    /// ```
    /// # use syrp_compat_state::{Listener, ValueTree};
    /// struct L;
    /// impl Listener for L {
    ///     fn property_changed(&mut self, _: &syrp_loop_api::MessageThread<'_>, tree: &ValueTree, id: syrp_var::Identifier) {
    ///         println!("{} changed on {:?}", id.as_str(), tree);
    ///     }
    /// }
    /// ```
    #[doc(alias = "valueTreePropertyChanged")]
    fn property_changed(&mut self, mt: &MessageThread<'_>, tree: &ValueTree, property: Identifier) {
    }

    /// `child` was added to `parent`, the listened tree or a descendant.
    ///
    /// ```
    /// # use syrp_compat_state::{Listener, ValueTree};
    /// struct L;
    /// impl Listener for L {
    ///     fn child_added(&mut self, _: &syrp_loop_api::MessageThread<'_>, parent: &ValueTree, child: &ValueTree) {
    ///         assert_eq!(child.parent(), *parent);
    ///     }
    /// }
    /// ```
    #[doc(alias = "valueTreeChildAdded")]
    fn child_added(&mut self, mt: &MessageThread<'_>, parent: &ValueTree, child: &ValueTree) {}

    /// `child` was removed from `parent` at `index`.
    ///
    /// ```
    /// # use syrp_compat_state::{Listener, ValueTree};
    /// struct L;
    /// impl Listener for L {
    ///     fn child_removed(&mut self, _: &syrp_loop_api::MessageThread<'_>, _: &ValueTree, child: &ValueTree, index: usize) {
    ///         println!("{child:?} left slot {index}");
    ///     }
    /// }
    /// ```
    #[doc(alias = "valueTreeChildRemoved")]
    fn child_removed(
        &mut self,
        mt: &MessageThread<'_>,
        parent: &ValueTree,
        child: &ValueTree,
        index: usize,
    ) {
    }

    /// A child of `parent` moved from `old_index` to `new_index`.
    ///
    /// ```
    /// # use syrp_compat_state::{Listener, ValueTree};
    /// struct L;
    /// impl Listener for L {
    ///     fn child_order_changed(&mut self, _: &syrp_loop_api::MessageThread<'_>, _: &ValueTree, from: usize, to: usize) {
    ///         println!("{from} -> {to}");
    ///     }
    /// }
    /// ```
    #[doc(alias = "valueTreeChildOrderChanged")]
    fn child_order_changed(
        &mut self,
        mt: &MessageThread<'_>,
        parent: &ValueTree,
        old_index: usize,
        new_index: usize,
    ) {
    }

    /// The listened tree itself was added to or removed from a parent. Unlike the other
    /// callbacks it is not sent for descendants.
    ///
    /// ```
    /// # use syrp_compat_state::{Listener, ValueTree};
    /// struct L;
    /// impl Listener for L {
    ///     fn parent_changed(&mut self, _: &syrp_loop_api::MessageThread<'_>, tree: &ValueTree) {
    ///         println!("parent is now {:?}", tree.parent());
    ///     }
    /// }
    /// ```
    #[doc(alias = "valueTreeParentChanged")]
    fn parent_changed(&mut self, mt: &MessageThread<'_>, tree: &ValueTree) {}

    /// The wrapper this listener was added to now refers to another node
    /// ([`ValueTree::assign`]); `tree` is that wrapper.
    ///
    /// ```
    /// # use syrp_compat_state::{Listener, ValueTree};
    /// struct L;
    /// impl Listener for L {
    ///     fn redirected(&mut self, _: &syrp_loop_api::MessageThread<'_>, tree: &ValueTree) {
    ///         println!("now listening to {tree:?}");
    ///     }
    /// }
    /// ```
    #[doc(alias = "valueTreeRedirected")]
    fn redirected(&mut self, mt: &MessageThread<'_>, tree: &ValueTree) {}
}

/// A listener added to one wrapper, with its store subscription while the wrapper is valid.
pub(crate) struct Attached {
    listener: Weak<RefCell<dyn Listener>>,
    _sub: Option<Subscription>,
}

impl Attached {
    fn is(&self, other: *const ()) -> bool {
        self.listener.as_ptr().cast::<()>() == other
    }
}

/// Subscribes `listener` to `node`. The closure holds the store and the listener weakly, so a
/// listener that keeps a `ValueTree` makes no reference cycle.
fn subscribe(
    mt: &MessageThread<'_>,
    node: Option<&Node>,
    listener: &Weak<RefCell<dyn Listener>>,
) -> Attached {
    let sub = node.and_then(|n| {
        let (store, l) = (Rc::downgrade(&n.store), listener.clone());
        let deliver = move |_: &Store, mt: &MessageThread<'_>, change: &Change| {
            let (Some(store), Some(l)) = (store.upgrade(), l.upgrade()) else {
                return;
            };
            let Ok(mut l) = l.try_borrow_mut() else {
                return;
            };
            dispatch(&mut *l, &store, mt, change);
        };
        n.store.subscribe(mt, &n.node, deliver).ok()
    });
    Attached {
        listener: listener.clone(),
        _sub: sub,
    }
}

/// Maps one store change to its JUCE callback.
fn dispatch(l: &mut dyn Listener, store: &Rc<Store>, mt: &MessageThread<'_>, change: &Change) {
    let tree = |k| ValueTree::from_handle(store, k);
    match change {
        Change::PropertyChanged { node, id, .. } | Change::PropertyRemoved { node, id, .. } => {
            l.property_changed(mt, &tree(*node), *id);
        }
        Change::ChildAdded { parent, child, .. } => {
            l.child_added(mt, &tree(*parent), &tree(*child));
        }
        Change::ChildRemoved {
            parent,
            child,
            index,
            ..
        } => {
            l.child_removed(mt, &tree(*parent), &tree(*child), *index);
        }
        Change::ChildMoved {
            parent, from, to, ..
        } => {
            l.child_order_changed(mt, &tree(*parent), *from, *to);
        }
        Change::ParentChanged { node, .. } => l.parent_changed(mt, &tree(*node)),
        _ => {}
    }
}

impl ValueTree {
    /// Adds `listener` to this wrapper (not to the shared node: clones do not carry it). It
    /// hears changes to this node and its descendants until it is removed, the wrapper drops,
    /// or the listener itself is dropped; the wrapper holds it weakly. Adding it twice does
    /// nothing. On an invalid tree it waits until [`ValueTree::assign`] makes the tree valid.
    ///
    /// ```
    /// # use std::{cell::RefCell, rc::Rc};
    /// # use syrp_compat_state::{Listener, ValueTree};
    /// struct Quiet;
    /// impl Listener for Quiet {}
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let mut tree = ValueTree::new(mt, "A");
    /// let quiet = Rc::new(RefCell::new(Quiet));
    /// tree.add_listener(mt, &quiet);
    /// assert_eq!(tree.reference_count(), 2); // the subscription counts, as in JUCE
    /// # });
    /// ```
    #[doc(alias = "addListener")]
    pub fn add_listener<L: Listener + 'static>(
        &mut self,
        mt: &MessageThread<'_>,
        listener: &Rc<RefCell<L>>,
    ) {
        self.listeners.retain(|a| a.listener.strong_count() > 0);
        if self
            .listeners
            .iter()
            .any(|a| a.is(Rc::as_ptr(listener).cast()))
        {
            return;
        }
        let dynamic: Rc<RefCell<dyn Listener>> = listener.clone();
        let attached = subscribe(mt, self.node.as_ref(), &Rc::downgrade(&dynamic));
        self.listeners.push(attached);
    }

    /// Removes `listener` from this wrapper; it hears nothing more, even for a change being
    /// delivered now. Removing a listener that was not added does nothing.
    ///
    /// ```
    /// # use std::{cell::RefCell, rc::Rc};
    /// # use syrp_compat_state::{Listener, ValueTree};
    /// # struct Quiet;
    /// # impl Listener for Quiet {}
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let mut tree = ValueTree::new(mt, "A");
    /// let quiet = Rc::new(RefCell::new(Quiet));
    /// tree.add_listener(mt, &quiet);
    /// tree.remove_listener(&quiet);
    /// assert_eq!(tree.reference_count(), 1);
    /// # });
    /// ```
    #[doc(alias = "removeListener")]
    pub fn remove_listener<L: Listener + 'static>(&mut self, listener: &Rc<RefCell<L>>) {
        let ptr = Rc::as_ptr(listener).cast();
        self.listeners.retain(|a| !a.is(ptr));
    }

    /// Makes this wrapper refer to `other`'s node, as JUCE's `operator=`. Its listeners move
    /// with it and each hears [`Listener::redirected`] once they are all attached; `other`'s
    /// listeners stay with `other`. Assigning the node it already refers to does nothing.
    ///
    /// ```
    /// # use syrp_compat_state::ValueTree;
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let mut tree = ValueTree::default();
    /// let target = ValueTree::new(mt, "B");
    /// tree.assign(mt, &target);
    /// assert_eq!(tree, target);
    /// # });
    /// ```
    #[doc(alias = "operator=")]
    pub fn assign(&mut self, mt: &MessageThread<'_>, other: &ValueTree) {
        if *self == *other {
            return;
        }
        self.node.clone_from(&other.node);
        let old = std::mem::take(&mut self.listeners);
        let live = old
            .iter()
            .filter_map(|a| a.listener.upgrade().map(|_| a.listener.clone()));
        let live: Vec<_> = live.collect();
        drop(old);
        for l in &live {
            let attached = subscribe(mt, self.node.as_ref(), l);
            self.listeners.push(attached);
        }
        for l in live.iter().filter_map(Weak::upgrade) {
            if let Ok(mut l) = l.try_borrow_mut() {
                l.redirected(mt, self);
            }
        }
    }
}

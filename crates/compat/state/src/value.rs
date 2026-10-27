//! [`Value`]: JUCE's `Value`, an observable cell over one property of a node.

use std::cell::RefCell;
use std::fmt;
use std::rc::{Rc, Weak};

use syrp_loop_api::MessageThread;
use syrp_state::{Change, NodeKey, StateError, Store, Subscription, UndoManager};
use syrp_var::{Identifier, Var};

use crate::tree::ValueTree;

/// Callback for changes to a [`Value`], as JUCE's `Value::Listener`.
///
/// ```
/// use syrp_compat_state::{Value, ValueListener};
/// struct Print;
/// impl ValueListener for Print {
///     fn value_changed(&mut self, _: &syrp_loop_api::MessageThread<'_>, value: &Value) {
///         println!("now {}", value.get().to_text());
///     }
/// }
/// ```
#[doc(alias = "Value::Listener")]
pub trait ValueListener {
    /// The property the value refers to was set to a different value or removed.
    ///
    /// ```
    /// # use syrp_compat_state::{Value, ValueListener};
    /// struct Count(u32);
    /// impl ValueListener for Count {
    ///     fn value_changed(&mut self, _: &syrp_loop_api::MessageThread<'_>, _: &Value) { self.0 += 1 }
    /// }
    /// ```
    #[doc(alias = "valueChanged")]
    fn value_changed(&mut self, mt: &MessageThread<'_>, value: &Value);
}

/// A reference to one property of one node, as JUCE's `Value`: reading gives the property
/// (void when absent), writing sets it, and every `Value` referring to the same property sees
/// the change. [`ValueTree::property_as_value`] makes one over a tree; [`Value::new`] makes a
/// standalone one in a hidden node.
///
/// Clones refer to the same property but do not carry listeners, which belong to the wrapper
/// (held weakly, like [`crate::Listener`]s). Listeners are called synchronously from the
/// store's change log, not asynchronously as in JUCE, and only when the value really changes.
///
/// ```
/// use syrp_compat_state::ValueTree;
/// syrp_loop_api::ManualLoop::new().with_token(|mt| {
///     let synth = ValueTree::new(mt, "Synth");
///     let cutoff = synth.property_as_value("cutoff");
///     cutoff.set(mt, 1200.0, None).unwrap();
///     assert_eq!(synth.property("cutoff").as_f64(), 1200.0);
/// });
/// ```
#[doc(alias = "juce::Value")]
pub struct Value {
    tree: ValueTree,
    id: Identifier,
    listeners: Vec<(WeakListener, Option<Subscription>)>,
}

/// A value listener, held weakly: callers own their listeners.
type WeakListener = Weak<RefCell<dyn ValueListener>>;

impl Clone for Value {
    fn clone(&self) -> Self {
        Self {
            tree: self.tree.clone(),
            id: self.id,
            listeners: Vec::new(),
        }
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Value({:?}.{})", self.tree, self.id.as_str())
    }
}

impl ValueTree {
    /// A [`Value`] referring to property `name` of this tree.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let tree = syrp_compat_state::ValueTree::new(mt, "A");
    /// tree.set_property(mt, "x", 3, None).unwrap();
    /// assert_eq!(tree.property_as_value("x").get(), 3.into());
    /// # });
    /// ```
    #[doc(alias = "getPropertyAsValue")]
    #[must_use]
    pub fn property_as_value(&self, name: impl Into<Identifier>) -> Value {
        Value {
            tree: self.clone(),
            id: name.into(),
            listeners: Vec::new(),
        }
    }
}

impl Value {
    /// A standalone value holding `initial`, in a hidden node of this thread's store.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let v = syrp_compat_state::Value::new(mt, "hello");
    /// assert_eq!(v.get().to_text(), "hello");
    /// # });
    /// ```
    pub fn new(mt: &MessageThread<'_>, initial: impl Into<Var>) -> Self {
        let tree = ValueTree::new(mt, "Value");
        let value = tree.property_as_value("value");
        let _ = value.set(mt, initial, None); // a fresh valid node cannot fail
        value
    }

    /// The current value: the property, or a void `Var` if it is absent.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let tree = syrp_compat_state::ValueTree::new(mt, "A");
    /// assert!(tree.property_as_value("x").get().is_void());
    /// # });
    /// ```
    #[doc(alias = "getValue")]
    #[must_use]
    pub fn get(&self) -> Var {
        self.tree.property(self.id)
    }

    /// Sets the property, recording it in `undo` if given. Setting an equal value does nothing.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let v = syrp_compat_state::Value::new(mt, 1);
    /// v.set(mt, 2, None).unwrap();
    /// assert_eq!(v.get(), 2.into());
    /// # });
    /// ```
    #[doc(alias = "setValue")]
    pub fn set(
        &self,
        mt: &MessageThread<'_>,
        value: impl Into<Var>,
        undo: Option<&mut UndoManager>,
    ) -> Result<(), StateError> {
        self.tree.set_property(mt, self.id, value, undo)
    }

    /// Makes this value refer to the same property as `other`. Its listeners stay registered
    /// and now hear `other`'s property; nothing is notified by the switch itself.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let (mut a, b) = (syrp_compat_state::Value::new(mt, 1), syrp_compat_state::Value::new(mt, 2));
    /// a.refer_to(mt, &b);
    /// assert!(a.refers_to_same_source_as(&b) && a.get() == 2.into());
    /// # });
    /// ```
    #[doc(alias = "referTo")]
    pub fn refer_to(&mut self, mt: &MessageThread<'_>, other: &Value) {
        self.tree = other.tree.clone();
        self.id = other.id;
        let listeners = std::mem::take(&mut self.listeners);
        for (l, sub) in listeners {
            drop(sub);
            let sub = self.subscribe(mt, &l);
            self.listeners.push((l, sub));
        }
    }

    /// Whether both refer to the same property of the same node.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let v = syrp_compat_state::Value::new(mt, 1);
    /// assert!(v.refers_to_same_source_as(&v.clone()));
    /// # });
    /// ```
    #[doc(alias = "refersToSameSourceAs")]
    #[must_use]
    pub fn refers_to_same_source_as(&self, other: &Value) -> bool {
        self.tree == other.tree && self.id == other.id
    }

    /// The tree holding the property (hidden for a standalone value).
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let tree = syrp_compat_state::ValueTree::new(mt, "A");
    /// assert_eq!(tree.property_as_value("x").tree(), tree);
    /// # });
    /// ```
    #[must_use]
    pub fn tree(&self) -> ValueTree {
        self.tree.clone()
    }

    /// The property this value refers to.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let v = syrp_compat_state::ValueTree::new(mt, "A").property_as_value("x");
    /// assert_eq!(v.property_id().as_str(), "x");
    /// # });
    /// ```
    #[must_use]
    pub fn property_id(&self) -> Identifier {
        self.id
    }

    /// The native key of the node holding the property (ADR 0003 D1).
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let tree = syrp_compat_state::ValueTree::new(mt, "A");
    /// assert_eq!(tree.property_as_value("x").handle(), tree.handle());
    /// # });
    /// ```
    #[must_use]
    pub fn handle(&self) -> Option<NodeKey> {
        self.tree.handle()
    }

    /// Adds `listener` to this wrapper; it hears every change of the property until removed,
    /// the wrapper drops or the listener drops. Adding it twice does nothing.
    ///
    /// **The caller owns the listener:** the wrapper holds it weakly, so passing a temporary
    /// `Rc` does nothing. Keep the `Rc` alive while it should hear changes.
    ///
    /// ```
    /// # use std::{cell::RefCell, rc::Rc};
    /// # use syrp_compat_state::{Value, ValueListener};
    /// struct Count(u32);
    /// impl ValueListener for Count {
    ///     fn value_changed(&mut self, _: &syrp_loop_api::MessageThread<'_>, _: &Value) { self.0 += 1 }
    /// }
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let mut v = Value::new(mt, 1);
    /// let count = Rc::new(RefCell::new(Count(0)));
    /// v.add_listener(mt, &count);
    /// v.set(mt, 2, None).unwrap();
    /// v.set(mt, 2, None).unwrap(); // unchanged: no call
    /// assert_eq!(count.borrow().0, 1);
    /// # });
    /// ```
    #[doc(alias = "addListener")]
    pub fn add_listener<L: ValueListener + 'static>(
        &mut self,
        mt: &MessageThread<'_>,
        listener: &Rc<RefCell<L>>,
    ) {
        self.listeners.retain(|(l, _)| l.strong_count() > 0);
        let ptr = Rc::as_ptr(listener).cast::<()>();
        if self
            .listeners
            .iter()
            .any(|(l, _)| l.as_ptr().cast::<()>() == ptr)
        {
            return;
        }
        let dynamic: Rc<RefCell<dyn ValueListener>> = listener.clone();
        let weak = Rc::downgrade(&dynamic);
        let sub = self.subscribe(mt, &weak);
        self.listeners.push((weak, sub));
    }

    /// Removes `listener` from this wrapper.
    ///
    /// ```
    /// # use std::{cell::RefCell, rc::Rc};
    /// # use syrp_compat_state::{Value, ValueListener};
    /// # struct Quiet;
    /// # impl ValueListener for Quiet { fn value_changed(&mut self, _: &syrp_loop_api::MessageThread<'_>, _: &Value) {} }
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let mut v = Value::new(mt, 1);
    /// let quiet = Rc::new(RefCell::new(Quiet));
    /// v.add_listener(mt, &quiet);
    /// v.remove_listener(&quiet);
    /// assert_eq!(v.tree().reference_count(), 2); // `v`'s tree and the clone
    /// # });
    /// ```
    #[doc(alias = "removeListener")]
    pub fn remove_listener<L: ValueListener + 'static>(&mut self, listener: &Rc<RefCell<L>>) {
        let ptr = Rc::as_ptr(listener).cast::<()>();
        self.listeners
            .retain(|(l, _)| l.as_ptr().cast::<()>() != ptr);
    }

    /// Subscribes `listener` to this value's property, holding it and the store weakly.
    fn subscribe(&self, mt: &MessageThread<'_>, listener: &WeakListener) -> Option<Subscription> {
        let (store, node) = (self.tree.store()?, self.tree.node_ref()?);
        let (weak, l, key, id) = (Rc::downgrade(store), listener.clone(), node.key(), self.id);
        let deliver = move |_: &Store, mt: &MessageThread<'_>, change: &Change| {
            let hit = match change {
                Change::PropertyChanged {
                    node, id: changed, ..
                }
                | Change::PropertyRemoved {
                    node, id: changed, ..
                } => *node == key && *changed == id,
                _ => false,
            };
            let (Some(store), Some(l)) = (weak.upgrade(), l.upgrade()) else {
                return;
            };
            if let (true, Ok(mut l)) = (hit, l.try_borrow_mut()) {
                let tree = ValueTree::from_handle(&store, key);
                l.value_changed(mt, &tree.property_as_value(id));
            }
        };
        store.subscribe(mt, node, deliver).ok()
    }
}

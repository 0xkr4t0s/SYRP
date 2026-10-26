//! [`CachedValue`]: JUCE's `CachedValue<T>`, a typed, cached view of one property, and the
//! [`VarConvert`] conversions it uses.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use syrp_loop_api::MessageThread;
use syrp_state::{Change, NodeKey, StateError, Store, Subscription, UndoManager};
use syrp_var::{Identifier, Var};

use crate::tree::ValueTree;

/// Conversion between a property's `Var` and a typed value, as JUCE's `VariantConverter`.
/// Reading uses `Var`'s loose conversions (`"12"` reads as `12`).
///
/// ```
/// use syrp_compat_state::VarConvert;
/// use syrp_var::Var;
/// assert_eq!(i32::from_var(&Var::from("12")), 12);
/// assert_eq!(2.5_f64.to_var(), Var::from(2.5));
/// ```
#[doc(alias = "VariantConverter")]
pub trait VarConvert: Sized {
    /// Reads the typed value from `v`.
    ///
    /// ```
    /// # use syrp_compat_state::VarConvert;
    /// assert!(bool::from_var(&syrp_var::Var::from(1)));
    /// ```
    fn from_var(v: &Var) -> Self;

    /// The `Var` stored for this value.
    ///
    /// ```
    /// # use syrp_compat_state::VarConvert;
    /// assert_eq!("a".to_owned().to_var().to_text(), "a");
    /// ```
    fn to_var(&self) -> Var;
}

macro_rules! convert {
    ($($t:ty => $read:expr, $write:expr;)*) => {$(
        impl VarConvert for $t {
            fn from_var(v: &Var) -> Self {
                $read(v)
            }
            fn to_var(&self) -> Var {
                $write(self)
            }
        }
    )*};
}

convert! {
    bool => Var::as_bool, |v: &bool| Var::from(*v);
    i32 => Var::as_i32, |v: &i32| Var::from(*v);
    i64 => Var::as_i64, |v: &i64| Var::from(*v);
    f64 => Var::as_f64, |v: &f64| Var::from(*v);
    // A property read as `f32` rounds to nearest, as JUCE's float conversion does.
    f32 => |v: &Var| v.as_f64() as f32, |v: &f32| Var::from(f64::from(*v));
    String => Var::to_text, |v: &String| Var::from(v.as_str());
    Var => Var::clone, Var::clone;
}

/// A typed, cached view of one property, as JUCE's `CachedValue<T>`: [`CachedValue::get`]
/// returns the cached property, or the default while the property is absent. The cache is
/// refreshed from the store's change log as soon as the property changes, by any route; it is a
/// store listener added when the value is bound, so it runs after listeners added before it.
///
/// ```
/// use syrp_compat_state::{CachedValue, ValueTree};
/// syrp_loop_api::ManualLoop::new().with_token(|mt| {
///     let synth = ValueTree::new(mt, "Synth");
///     let cutoff = CachedValue::bound(mt, &synth, "cutoff", 1000.0);
///     assert!(cutoff.is_using_default() && cutoff.get() == 1000.0);
///     synth.set_property(mt, "cutoff", 1200.0, None).unwrap();
///     assert_eq!(cutoff.get(), 1200.0);
/// });
/// ```
#[doc(alias = "juce::CachedValue")]
pub struct CachedValue<T> {
    tree: ValueTree,
    id: Identifier,
    default: T,
    cache: Rc<RefCell<Option<T>>>,
    _sub: Option<Subscription>,
}

impl<T: VarConvert + Default> Default for CachedValue<T> {
    fn default() -> Self {
        let (tree, id, default) = (ValueTree::default(), Identifier::default(), T::default());
        Self {
            tree,
            id,
            default,
            cache: Rc::default(),
            _sub: None,
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for CachedValue<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let cache = self.cache.borrow();
        write!(
            f,
            "CachedValue({:?}.{} = {:?}, default {:?})",
            self.tree,
            self.id.as_str(),
            cache,
            self.default
        )
    }
}

impl<T: VarConvert + Clone + 'static> CachedValue<T> {
    /// A cached value bound to property `id` of `tree`, with `default`.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let tree = syrp_compat_state::ValueTree::new(mt, "A");
    /// let on = syrp_compat_state::CachedValue::bound(mt, &tree, "on", true);
    /// assert!(on.get());
    /// # });
    /// ```
    pub fn bound(
        mt: &MessageThread<'_>,
        tree: &ValueTree,
        id: impl Into<Identifier>,
        default: T,
    ) -> Self {
        let cache = Rc::default();
        let tree = tree.clone();
        let mut me = Self {
            tree,
            id: id.into(),
            default,
            cache,
            _sub: None,
        };
        me.subscribe(mt);
        me
    }

    /// Rebinds to property `id` of `tree` with a new `default`.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let tree = syrp_compat_state::ValueTree::new(mt, "A");
    /// tree.set_property(mt, "n", 4, None).unwrap();
    /// let mut n = syrp_compat_state::CachedValue::<i32>::default();
    /// n.refer_to(mt, &tree, "n", 0);
    /// assert_eq!(n.get(), 4);
    /// # });
    /// ```
    #[doc(alias = "referTo")]
    pub fn refer_to(
        &mut self,
        mt: &MessageThread<'_>,
        tree: &ValueTree,
        id: impl Into<Identifier>,
        default: T,
    ) {
        *self = Self::bound(mt, tree, id, default);
    }

    /// The cached property, or the default while it is absent.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let tree = syrp_compat_state::ValueTree::new(mt, "A");
    /// assert_eq!(syrp_compat_state::CachedValue::bound(mt, &tree, "s", String::from("d")).get(), "d");
    /// # });
    /// ```
    #[must_use]
    pub fn get(&self) -> T {
        self.cache
            .borrow()
            .clone()
            .unwrap_or_else(|| self.default.clone())
    }

    /// Whether the property is absent, so [`CachedValue::get`] returns the default.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let tree = syrp_compat_state::ValueTree::new(mt, "A");
    /// assert!(syrp_compat_state::CachedValue::bound(mt, &tree, "x", 0).is_using_default());
    /// # });
    /// ```
    #[doc(alias = "isUsingDefault")]
    #[must_use]
    pub fn is_using_default(&self) -> bool {
        self.cache.borrow().is_none()
    }

    /// The default value.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let tree = syrp_compat_state::ValueTree::new(mt, "A");
    /// assert_eq!(*syrp_compat_state::CachedValue::bound(mt, &tree, "x", 5).default_value(), 5);
    /// # });
    /// ```
    #[doc(alias = "getDefault")]
    #[must_use]
    pub fn default_value(&self) -> &T {
        &self.default
    }

    /// Replaces the default, which [`CachedValue::get`] returns while the property is absent.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let tree = syrp_compat_state::ValueTree::new(mt, "A");
    /// let mut x = syrp_compat_state::CachedValue::bound(mt, &tree, "x", 5);
    /// x.set_default(6);
    /// assert_eq!(x.get(), 6);
    /// # });
    /// ```
    #[doc(alias = "setDefault")]
    pub fn set_default(&mut self, default: T) {
        self.default = default;
    }

    /// Sets the property to `value`, recording it in `undo` if given.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let tree = syrp_compat_state::ValueTree::new(mt, "A");
    /// let x = syrp_compat_state::CachedValue::bound(mt, &tree, "x", 0);
    /// x.set(mt, 9, None).unwrap();
    /// assert_eq!((x.get(), tree.property("x")), (9, 9.into()));
    /// # });
    /// ```
    #[doc(alias = "setValue")]
    pub fn set(
        &self,
        mt: &MessageThread<'_>,
        value: T,
        undo: Option<&mut UndoManager>,
    ) -> Result<(), StateError> {
        self.tree.set_property(mt, self.id, value.to_var(), undo)
    }

    /// Removes the property, so the default applies again.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let tree = syrp_compat_state::ValueTree::new(mt, "A");
    /// let x = syrp_compat_state::CachedValue::bound(mt, &tree, "x", 0);
    /// x.set(mt, 9, None).unwrap();
    /// x.reset_to_default(mt, None).unwrap();
    /// assert!(x.is_using_default() && !tree.has_property("x"));
    /// # });
    /// ```
    #[doc(alias = "resetToDefault")]
    pub fn reset_to_default(
        &self,
        mt: &MessageThread<'_>,
        undo: Option<&mut UndoManager>,
    ) -> Result<(), StateError> {
        self.tree.remove_property(mt, self.id, undo)
    }

    /// Re-reads the property into the cache. The cache already follows every change, so this
    /// is only for parity with JUCE.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let tree = syrp_compat_state::ValueTree::new(mt, "A");
    /// let x = syrp_compat_state::CachedValue::bound(mt, &tree, "x", 0);
    /// x.force_update_of_cached_value();
    /// assert_eq!(x.get(), 0);
    /// # });
    /// ```
    #[doc(alias = "forceUpdateOfCachedValue")]
    pub fn force_update_of_cached_value(&self) {
        let value = self
            .tree
            .has_property(self.id)
            .then(|| T::from_var(&self.tree.property(self.id)));
        *self.cache.borrow_mut() = value;
    }

    /// The property this value caches.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let tree = syrp_compat_state::ValueTree::new(mt, "A");
    /// assert_eq!(syrp_compat_state::CachedValue::bound(mt, &tree, "x", 0).property_id().as_str(), "x");
    /// # });
    /// ```
    #[doc(alias = "getPropertyID")]
    #[must_use]
    pub fn property_id(&self) -> Identifier {
        self.id
    }

    /// The tree holding the property.
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let tree = syrp_compat_state::ValueTree::new(mt, "A");
    /// assert_eq!(syrp_compat_state::CachedValue::bound(mt, &tree, "x", 0).value_tree(), tree);
    /// # });
    /// ```
    #[doc(alias = "getValueTree")]
    #[must_use]
    pub fn value_tree(&self) -> ValueTree {
        self.tree.clone()
    }

    /// The native key of the node holding the property (ADR 0003 D1).
    ///
    /// ```
    /// # syrp_loop_api::ManualLoop::new().with_token(|mt| {
    /// let tree = syrp_compat_state::ValueTree::new(mt, "A");
    /// assert_eq!(syrp_compat_state::CachedValue::bound(mt, &tree, "x", 0).handle(), tree.handle());
    /// # });
    /// ```
    #[must_use]
    pub fn handle(&self) -> Option<NodeKey> {
        self.tree.handle()
    }

    /// Fills the cache and subscribes it to the property.
    fn subscribe(&mut self, mt: &MessageThread<'_>) {
        self.force_update_of_cached_value();
        let (Some(store), Some(node)) = (self.tree.store(), self.tree.node_ref()) else {
            return;
        };
        let (cache, key, id) = (Rc::downgrade(&self.cache), node.key(), self.id);
        let refresh = move |_: &Store, _: &MessageThread<'_>, change: &Change| {
            let value = match change {
                Change::PropertyChanged {
                    node, id: c, new, ..
                } if *node == key && *c == id => Some(T::from_var(new)),
                Change::PropertyRemoved { node, id: c, .. } if *node == key && *c == id => None,
                _ => return,
            };
            if let Some(cache) = cache.upgrade() {
                *cache.borrow_mut() = value;
            }
        };
        self._sub = store.subscribe(mt, node, refresh).ok();
    }
}

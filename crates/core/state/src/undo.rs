//! [`UndoManager`]: transactions of reversible log records, with coalescing.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use syrp_loop_api::MessageThread;
use syrp_var::{Identifier, Var};

use crate::error::{StateError, StateErrorKind};
use crate::log::{Change, LogRecord};
use crate::node::NodeRef;
use crate::store::{Inner, Store};

/// One undo step: a name and the records it groups, oldest first.
struct Transaction {
    name: String,
    records: Vec<Change>,
}

impl Transaction {
    /// Whether undoing it would change nothing: every record is a property set back to the
    /// value it started from.
    fn is_no_op(&self) -> bool {
        self.records.iter().all(|c| {
            matches!(c, Change::PropertyChanged { old: Some(old), new, .. } if old.eq_same_type(new))
        })
    }
}

/// Undo history for one [`Store`]: transactions of the store's reversible [`Change`] records.
///
/// Mutations made through a store's `*_with_undo` methods add their records to the current
/// transaction; the plain methods record nothing. [`UndoManager::begin_new_transaction`] starts
/// a new transaction, which is created by its first record. The manager holds one count on
/// every node its records name, so a detached subtree it may need to restore stays alive until
/// the transaction is discarded. A manager binds to the first store it records from; using it
/// with another store is [`StateErrorKind::ForeignUndoManager`].
///
/// Coalescing (a Syrp rule): a `PropertyChanged` record for the same node and property as the
/// record just before it in the same transaction merges into it, keeping the first old value
/// and the last new value, so a slider drag is one step. A transaction whose records all cancel
/// out (a value set back to where it started) is kept, with no effect, until the next
/// transaction starts, and is then dropped.
///
/// ```
/// use syrp_state::{Store, UndoManager};
/// use syrp_var::{Identifier, Var};
///
/// syrp_loop_api::ManualLoop::new().with_token(|mt| {
///     let store = Store::new();
///     let mut undo = UndoManager::new();
///     let node = store.create(mt, Identifier::new("Synth"));
///     let gain = Identifier::new("gain");
///     undo.begin_new_transaction("drag gain");
///     for v in [0.1, 0.2, 0.3] {
///         store.set_property_with_undo(mt, &mut undo, &node, gain, v).unwrap();
///     }
///     assert!(undo.undo(mt).unwrap()); // one step undoes the whole drag
///     assert_eq!(store.property(&node, gain).unwrap(), None);
///     assert!(undo.redo(mt).unwrap());
///     assert_eq!(store.property(&node, gain).unwrap(), Some(Var::from(0.3)));
/// });
/// ```
#[derive(Default)]
pub struct UndoManager {
    store: Option<Rc<RefCell<Inner>>>,
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
    /// The name for the next transaction, once `begin_new_transaction` was called.
    next_name: Option<String>,
}

impl fmt::Debug for UndoManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UndoManager")
            .field("undo", &self.undo.len())
            .field("redo", &self.redo.len())
            .finish()
    }
}

impl Drop for UndoManager {
    fn drop(&mut self) {
        let undo = std::mem::take(&mut self.undo);
        let redo = std::mem::take(&mut self.redo);
        self.discard(undo.into_iter().chain(redo));
    }
}

impl UndoManager {
    /// An empty manager, not yet bound to a store.
    ///
    /// ```
    /// assert!(!syrp_state::UndoManager::new().can_undo());
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Makes the next recorded change start a new transaction called `name`.
    ///
    /// ```
    /// # use syrp_var::Identifier; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let mut undo = syrp_state::UndoManager::new();
    /// let node = store.create(mt, Identifier::new("A"));
    /// for name in ["first", "second"] {
    ///     undo.begin_new_transaction(name);
    ///     store.set_property_with_undo(mt, &mut undo, &node, Identifier::new(name), 1).unwrap();
    /// }
    /// undo.undo(mt).unwrap();
    /// assert!(store.has_property(&node, Identifier::new("first")).unwrap());
    /// # });
    /// ```
    #[doc(alias = "beginNewTransaction")]
    pub fn begin_new_transaction(&mut self, name: impl Into<String>) {
        self.next_name = Some(name.into());
    }

    /// Whether there is a transaction to undo.
    ///
    /// ```
    /// assert!(!syrp_state::UndoManager::new().can_undo());
    /// ```
    #[doc(alias = "canUndo")]
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether there is an undone transaction to redo.
    ///
    /// ```
    /// assert!(!syrp_state::UndoManager::new().can_redo());
    /// ```
    #[doc(alias = "canRedo")]
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// The name of the transaction [`UndoManager::undo`] would reverse, if any. A transaction
    /// started without [`UndoManager::begin_new_transaction`] has an empty name.
    ///
    /// ```
    /// # use syrp_var::Identifier; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let mut undo = syrp_state::UndoManager::new();
    /// let node = store.create(mt, Identifier::new("A"));
    /// undo.begin_new_transaction("rename");
    /// store.set_property_with_undo(mt, &mut undo, &node, Identifier::new("name"), "a").unwrap();
    /// assert_eq!(undo.undo_description(), Some("rename"));
    /// # });
    /// ```
    #[doc(alias = "getUndoDescription")]
    #[must_use]
    pub fn undo_description(&self) -> Option<&str> {
        self.undo.last().map(|t| t.name.as_str())
    }

    /// The name of the transaction [`UndoManager::redo`] would re-apply, if any.
    ///
    /// ```
    /// assert_eq!(syrp_state::UndoManager::new().redo_description(), None);
    /// ```
    #[doc(alias = "getRedoDescription")]
    #[must_use]
    pub fn redo_description(&self) -> Option<&str> {
        self.redo.last().map(|t| t.name.as_str())
    }

    /// Reverses the last transaction's records, newest first, and moves it to the redo list.
    /// Returns `false` if there was nothing to undo.
    ///
    /// The reversal is made of ordinary store mutations: they are logged and delivered to
    /// listeners, and not recorded. If the store was changed since without this manager so that
    /// a record no longer applies (say a removed child now has another parent), the error is
    /// returned and the records reversed before it stay reversed.
    ///
    /// ```
    /// # use syrp_var::Identifier; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let mut undo = syrp_state::UndoManager::new();
    /// assert!(!undo.undo(mt).unwrap());
    /// let (p, c) = (store.create(mt, Identifier::new("P")), store.create(mt, Identifier::new("C")));
    /// store.add_child_with_undo(mt, &mut undo, &p, &c, None).unwrap();
    /// assert!(undo.undo(mt).unwrap());
    /// assert_eq!(store.parent(&c).unwrap(), None);
    /// # });
    /// ```
    pub fn undo(&mut self, mt: &MessageThread<'_>) -> Result<bool, StateError> {
        let (Some(store), Some(t)) = (self.store(), self.undo.pop()) else {
            return Ok(false);
        };
        let result = t
            .records
            .iter()
            .rev()
            .try_for_each(|c| apply(&store, mt, c, true));
        self.redo.push(t);
        result.map(|()| true)
    }

    /// Re-applies the last undone transaction's records, oldest first, and moves it back to
    /// the undo list. Returns `false` if there was nothing to redo. Errors as for
    /// [`UndoManager::undo`].
    ///
    /// ```
    /// # use syrp_var::Identifier; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let mut undo = syrp_state::UndoManager::new();
    /// let node = store.create(mt, Identifier::new("A"));
    /// store.set_property_with_undo(mt, &mut undo, &node, Identifier::new("x"), 1).unwrap();
    /// undo.undo(mt).unwrap();
    /// assert!(undo.redo(mt).unwrap() && !undo.redo(mt).unwrap());
    /// assert!(store.has_property(&node, Identifier::new("x")).unwrap());
    /// # });
    /// ```
    pub fn redo(&mut self, mt: &MessageThread<'_>) -> Result<bool, StateError> {
        let (Some(store), Some(t)) = (self.store(), self.redo.pop()) else {
            return Ok(false);
        };
        let result = t
            .records
            .iter()
            .try_for_each(|c| apply(&store, mt, c, false));
        self.undo.push(t);
        result.map(|()| true)
    }

    /// A store value sharing the bound store's interior, for applying records.
    fn store(&self) -> Option<Store> {
        self.store.as_ref().map(Store::from_rc)
    }

    /// Binds to `store` on first use; fails if bound to another store.
    pub(crate) fn bind(&mut self, store: &Rc<RefCell<Inner>>) -> Result<(), StateError> {
        match &self.store {
            Some(bound) if !Rc::ptr_eq(bound, store) => {
                Err(StateError::new(StateErrorKind::ForeignUndoManager))
            }
            Some(_) => Ok(()),
            None => {
                self.store = Some(Rc::clone(store));
                Ok(())
            }
        }
    }

    /// Adds records captured from one mutation, each already holding its counts.
    pub(crate) fn add(&mut self, records: Vec<Change>) {
        if records.is_empty() {
            return;
        }
        let redo = std::mem::take(&mut self.redo);
        self.discard(redo);
        let mut dropped = Vec::new();
        if let Some(name) = self.next_name.take() {
            if self.undo.last().is_some_and(Transaction::is_no_op) {
                dropped.extend(self.undo.pop());
            }
            self.undo.push(Transaction {
                name,
                records: Vec::new(),
            });
        } else if self.undo.is_empty() {
            self.undo.push(Transaction {
                name: String::new(),
                records: Vec::new(),
            });
        }
        let records_now = &mut self.undo.last_mut().expect("just ensured").records;
        let mut merged = Vec::new();
        for change in records {
            match (records_now.last_mut(), change) {
                (
                    Some(Change::PropertyChanged {
                        node: n0,
                        id: i0,
                        new: last,
                        ..
                    }),
                    Change::PropertyChanged { node, id, old, new },
                ) if *n0 == node && *i0 == id => {
                    *last = new;
                    merged.push(Change::PropertyChanged {
                        node,
                        id,
                        old,
                        new: Var::default(),
                    });
                }
                (_, change) => records_now.push(change),
            }
        }
        self.discard(dropped);
        if let Some(store) = &self.store {
            let mut inner = store.borrow_mut();
            merged.iter().for_each(|c| inner.unpin(c));
        }
    }

    /// Releases the counts held by `transactions`' records.
    fn discard(&self, transactions: impl IntoIterator<Item = Transaction>) {
        let Some(store) = &self.store else { return };
        let mut inner = store.borrow_mut();
        for t in transactions {
            t.records.iter().for_each(|c| inner.unpin(c));
        }
    }
}

/// Applies one record backwards (`reverse`) or forwards, through the ordinary mutations.
fn apply(
    store: &Store,
    mt: &MessageThread<'_>,
    change: &Change,
    reverse: bool,
) -> Result<(), StateError> {
    let set = |node, id: Identifier, value: Option<&Var>| -> Result<(), StateError> {
        let node: NodeRef = store.upgrade(node)?;
        match value {
            Some(v) => store.set_property(mt, &node, id, v.clone()),
            None => store.remove_property(mt, &node, id).map(drop),
        }
    };
    let link = |parent, child, index: Option<usize>, add: bool| -> Result<(), StateError> {
        let (p, c) = (store.upgrade(parent)?, store.upgrade(child)?);
        if add {
            store.add_child(mt, &p, &c, index)
        } else {
            store.remove_child(mt, &p, &c)
        }
    };
    match change {
        Change::PropertyChanged { node, id, old, new } => {
            set(*node, *id, if reverse { old.as_ref() } else { Some(new) })
        }
        Change::PropertyRemoved { node, id, old } => set(*node, *id, reverse.then_some(old)),
        Change::ChildAdded {
            parent,
            child,
            index,
        } => link(*parent, *child, Some(*index), !reverse),
        Change::ChildRemoved {
            parent,
            child,
            index,
        } => link(*parent, *child, Some(*index), reverse),
        Change::ChildMoved { parent, from, to } => {
            let (from, to) = if reverse { (*to, *from) } else { (*from, *to) };
            store.move_child(mt, &store.upgrade(*parent)?, from, to)
        }
        // Implied by the child records, whose replay logs its own `ParentChanged`.
        Change::ParentChanged { .. } => Ok(()),
    }
}

impl Store {
    /// Runs one mutation with `undo` capturing the records it appends itself (not those of
    /// listeners it triggers). Capture happens when the mutation starts delivery.
    fn recorded<T>(
        &self,
        undo: &mut UndoManager,
        f: impl FnOnce() -> Result<T, StateError>,
    ) -> Result<T, StateError> {
        undo.bind(self.rc())?;
        {
            let mut inner = self.rc().borrow_mut();
            inner.capture_from = Some(inner.next_seq);
        }
        let result = f();
        let records = {
            let mut inner = self.rc().borrow_mut();
            inner.capture_from = None;
            std::mem::take(&mut inner.captured)
        };
        undo.add(records);
        result
    }

    /// [`Store::set_property`], recorded in `undo`'s current transaction.
    ///
    /// ```
    /// # use syrp_var::Identifier; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let mut undo = syrp_state::UndoManager::new();
    /// let node = store.create(mt, Identifier::new("A"));
    /// store.set_property_with_undo(mt, &mut undo, &node, Identifier::new("x"), 1).unwrap();
    /// assert!(undo.can_undo());
    /// # });
    /// ```
    pub fn set_property_with_undo(
        &self,
        mt: &MessageThread<'_>,
        undo: &mut UndoManager,
        node: &NodeRef,
        id: Identifier,
        value: impl Into<Var>,
    ) -> Result<(), StateError> {
        self.recorded(undo, || self.set_property(mt, node, id, value))
    }

    /// [`Store::remove_property`], recorded in `undo`'s current transaction.
    ///
    /// ```
    /// # use syrp_var::Identifier; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let mut undo = syrp_state::UndoManager::new();
    /// let node = store.create(mt, Identifier::new("A"));
    /// store.set_property(mt, &node, Identifier::new("x"), 1).unwrap();
    /// store.remove_property_with_undo(mt, &mut undo, &node, Identifier::new("x")).unwrap();
    /// undo.undo(mt).unwrap();
    /// assert!(store.has_property(&node, Identifier::new("x")).unwrap());
    /// # });
    /// ```
    pub fn remove_property_with_undo(
        &self,
        mt: &MessageThread<'_>,
        undo: &mut UndoManager,
        node: &NodeRef,
        id: Identifier,
    ) -> Result<Option<Var>, StateError> {
        self.recorded(undo, || self.remove_property(mt, node, id))
    }

    /// [`Store::remove_all_properties`], recorded in `undo`'s current transaction.
    ///
    /// ```
    /// # use syrp_var::Identifier; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let mut undo = syrp_state::UndoManager::new();
    /// let node = store.create(mt, Identifier::new("A"));
    /// store.set_property(mt, &node, Identifier::new("x"), 1).unwrap();
    /// store.remove_all_properties_with_undo(mt, &mut undo, &node).unwrap();
    /// undo.undo(mt).unwrap();
    /// assert_eq!(store.num_properties(&node).unwrap(), 1);
    /// # });
    /// ```
    pub fn remove_all_properties_with_undo(
        &self,
        mt: &MessageThread<'_>,
        undo: &mut UndoManager,
        node: &NodeRef,
    ) -> Result<(), StateError> {
        self.recorded(undo, || self.remove_all_properties(mt, node))
    }

    /// [`Store::add_child`], recorded in `undo`'s current transaction.
    ///
    /// ```
    /// # use syrp_var::Identifier; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let mut undo = syrp_state::UndoManager::new();
    /// let (p, c) = (store.create(mt, Identifier::new("P")), store.create(mt, Identifier::new("C")));
    /// store.add_child_with_undo(mt, &mut undo, &p, &c, None).unwrap();
    /// assert!(undo.can_undo());
    /// # });
    /// ```
    pub fn add_child_with_undo(
        &self,
        mt: &MessageThread<'_>,
        undo: &mut UndoManager,
        parent: &NodeRef,
        child: &NodeRef,
        index: Option<usize>,
    ) -> Result<(), StateError> {
        self.recorded(undo, || self.add_child(mt, parent, child, index))
    }

    /// [`Store::remove_child`], recorded in `undo`'s current transaction. Undo re-attaches the
    /// child at its old index; the manager keeps it alive until then.
    ///
    /// ```
    /// # use syrp_var::Identifier; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let mut undo = syrp_state::UndoManager::new();
    /// let (p, c) = (store.create(mt, Identifier::new("P")), store.create(mt, Identifier::new("C")));
    /// store.add_child(mt, &p, &c, None).unwrap();
    /// store.remove_child_with_undo(mt, &mut undo, &p, &c).unwrap();
    /// undo.undo(mt).unwrap();
    /// assert_eq!(store.index_of(&p, &c).unwrap(), Some(0));
    /// # });
    /// ```
    pub fn remove_child_with_undo(
        &self,
        mt: &MessageThread<'_>,
        undo: &mut UndoManager,
        parent: &NodeRef,
        child: &NodeRef,
    ) -> Result<(), StateError> {
        self.recorded(undo, || self.remove_child(mt, parent, child))
    }

    /// [`Store::remove_all_children`], recorded in `undo`'s current transaction.
    ///
    /// ```
    /// # use syrp_var::Identifier; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let mut undo = syrp_state::UndoManager::new();
    /// let p = store.create(mt, Identifier::new("P"));
    /// store.add_child(mt, &p, &store.create(mt, Identifier::new("C")), None).unwrap();
    /// store.remove_all_children_with_undo(mt, &mut undo, &p).unwrap();
    /// undo.undo(mt).unwrap();
    /// assert_eq!(store.num_children(&p).unwrap(), 1);
    /// # });
    /// ```
    pub fn remove_all_children_with_undo(
        &self,
        mt: &MessageThread<'_>,
        undo: &mut UndoManager,
        node: &NodeRef,
    ) -> Result<(), StateError> {
        self.recorded(undo, || self.remove_all_children(mt, node))
    }

    /// [`Store::move_child`], recorded in `undo`'s current transaction.
    ///
    /// ```
    /// # use syrp_var::Identifier; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let mut undo = syrp_state::UndoManager::new();
    /// let p = store.create(mt, Identifier::new("P"));
    /// let a = store.create(mt, Identifier::new("A"));
    /// store.add_child(mt, &p, &a, None).unwrap();
    /// store.add_child(mt, &p, &store.create(mt, Identifier::new("B")), None).unwrap();
    /// store.move_child_with_undo(mt, &mut undo, &p, 0, 1).unwrap();
    /// undo.undo(mt).unwrap();
    /// assert_eq!(store.index_of(&p, &a).unwrap(), Some(0));
    /// # });
    /// ```
    pub fn move_child_with_undo(
        &self,
        mt: &MessageThread<'_>,
        undo: &mut UndoManager,
        parent: &NodeRef,
        from: usize,
        to: usize,
    ) -> Result<(), StateError> {
        self.recorded(undo, || self.move_child(mt, parent, from, to))
    }
}

impl Inner {
    /// Copies the records appended since `capture_from` into `captured`, counting the nodes
    /// they name for the undo manager. Called when a mutation starts delivery.
    pub(crate) fn capture(&mut self) {
        let Some(start) = self.capture_from.take() else {
            return;
        };
        let Some(first) = self.log.front().map(LogRecord::seq) else {
            return;
        };
        for seq in start.max(first)..self.next_seq {
            let at = usize::try_from(seq - first).expect("retained record");
            let change = self.log[at].change().clone();
            self.pin(&change);
            self.captured.push(change);
        }
    }
}

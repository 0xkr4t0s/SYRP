//! [`UndoManager`]: transactions of reversible log records and actions, with coalescing.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use syrp_loop_api::MessageThread;
use syrp_var::{Identifier, Var};

use crate::action::{Item, UndoableAction};
use crate::error::{StateError, StateErrorKind};
use crate::log::{Change, LogRecord};
use crate::node::NodeRef;
use crate::store::{Inner, Store};

/// One undo step: a name and its items, oldest first.
struct Transaction {
    name: String,
    items: Vec<Item>,
}

impl Transaction {
    /// Whether undoing it would change nothing: every item is a property set back to the
    /// value it started from.
    fn is_no_op(&self) -> bool {
        self.items.iter().all(|i| {
            matches!(i, Item::Record(Change::PropertyChanged { old: Some(old), new, .. }) if old.eq_same_type(new))
        })
    }

    fn units(&self) -> usize {
        self.items.iter().map(Item::units).sum()
    }
}

/// Undo history for one [`Store`]: transactions of the store's reversible [`Change`] records
/// and of [`UndoableAction`]s.
///
/// Mutations made through a store's `*_with_undo` methods add their records to the current
/// transaction; the plain methods record nothing. [`UndoManager::perform`] adds an action.
/// [`UndoManager::begin_new_transaction`] starts a new transaction, which is created by its
/// first item; work recorded after an undo or redo always starts a new transaction. The
/// manager holds one count on every node its records name, so a detached
/// subtree it may need to restore stays alive until the transaction is discarded. A manager
/// binds to the first store it records from; using it with another store is
/// [`StateErrorKind::ForeignUndoManager`].
///
/// Coalescing (a Syrp rule): a `PropertyChanged` record for the same node and property as the
/// record just before it in the same transaction merges into it, keeping the first old value
/// and the last new value, so a slider drag is one step. A transaction whose records all cancel
/// out (a value set back to where it started) is kept, with no effect, until the next
/// transaction starts, and is then dropped.
///
/// Size: a store record counts 1 unit plus 1 per 64 bytes of `Var` payload (text and binary
/// bytes; 16 bytes per array element plus its payload; objects 0); an action counts its
/// [`UndoableAction::size_in_units`]. The defaults are 30,000 units and 30 transactions (see
/// [`UndoManager::set_max_number_of_stored_units`]).
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
pub struct UndoManager {
    store: Option<Rc<RefCell<Inner>>>,
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
    /// The name for the next transaction, once `begin_new_transaction` was called.
    next_name: Option<String>,
    max_units: usize,
    min_transactions: usize,
    performing: bool,
    /// Whether new items may join the last undoable transaction. Undo and redo close it, so
    /// work after them always starts a new transaction.
    open: bool,
}

impl Default for UndoManager {
    fn default() -> Self {
        Self {
            store: None,
            undo: Vec::new(),
            redo: Vec::new(),
            next_name: None,
            max_units: 30_000,
            min_transactions: 30,
            performing: false,
            open: false,
        }
    }
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
        self.clear_undo_history();
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

    /// Makes the next recorded change or action start a new transaction called `name`.
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

    /// Performs `action` and, if it succeeds, adds it to the current transaction (after any
    /// store records already there). If the transaction's last item is an action, it is first
    /// offered `action` through [`UndoableAction::coalesce_with`]. Returns what `perform`
    /// returned; a failed action is dropped.
    ///
    /// ```
    /// # use syrp_state::UndoableAction;
    /// struct Noop;
    /// impl UndoableAction for Noop {
    ///     fn perform(&mut self, _: &syrp_loop_api::MessageThread<'_>) -> bool { true }
    ///     fn undo(&mut self, _: &syrp_loop_api::MessageThread<'_>) -> bool { true }
    /// }
    /// syrp_loop_api::ManualLoop::new().with_token(|mt| {
    ///     let mut undo = syrp_state::UndoManager::new();
    ///     assert!(undo.perform(mt, Noop) && undo.can_undo());
    /// });
    /// ```
    pub fn perform(&mut self, mt: &MessageThread<'_>, action: impl UndoableAction) -> bool {
        let mut action: Box<dyn UndoableAction> = Box::new(action);
        if !action.perform(mt) {
            return false;
        }
        let items = self.current();
        if let Some(Item::Action(last)) = items.last_mut()
            && last.coalesce_with(&*action)
        {
            return true;
        }
        items.push(Item::Action(action));
        true
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

    /// The names of every undoable transaction, the one [`UndoManager::undo`] would reverse
    /// first.
    ///
    /// ```
    /// assert!(syrp_state::UndoManager::new().undo_descriptions().is_empty());
    /// ```
    #[doc(alias = "getUndoDescriptions")]
    #[must_use]
    pub fn undo_descriptions(&self) -> Vec<&str> {
        self.undo.iter().rev().map(|t| t.name.as_str()).collect()
    }

    /// Number of undoable transactions.
    ///
    /// ```
    /// assert_eq!(syrp_state::UndoManager::new().transaction_count(), 0);
    /// ```
    #[doc(alias = "getNumberOfTransactions")]
    #[must_use]
    pub fn transaction_count(&self) -> usize {
        self.undo.len()
    }

    /// The units held by every stored transaction, undoable and redoable (see the type docs).
    ///
    /// ```
    /// # use syrp_var::Identifier; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let mut undo = syrp_state::UndoManager::new();
    /// let node = store.create(mt, Identifier::new("A"));
    /// store.set_property_with_undo(mt, &mut undo, &node, Identifier::new("x"), 1).unwrap();
    /// assert_eq!(undo.number_of_units_taken_up(), 1);
    /// # });
    /// ```
    #[doc(alias = "getNumberOfUnitsTakenUpByStoredCommands")]
    #[must_use]
    pub fn number_of_units_taken_up(&self) -> usize {
        self.undo
            .iter()
            .chain(&self.redo)
            .map(Transaction::units)
            .sum()
    }

    /// Sets the size limit. When a new transaction starts (and now), the oldest undoable
    /// transactions are dropped while the units exceed `max_units`, but `min_transactions`
    /// (and the current transaction) are always kept.
    ///
    /// ```
    /// # use syrp_var::Identifier; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let mut undo = syrp_state::UndoManager::new();
    /// let node = store.create(mt, Identifier::new("A"));
    /// for i in 0..5 {
    ///     undo.begin_new_transaction(format!("t{i}"));
    ///     store.set_property_with_undo(mt, &mut undo, &node, Identifier::new("x"), i).unwrap();
    /// }
    /// undo.set_max_number_of_stored_units(2, 1);
    /// assert_eq!(undo.undo_descriptions(), ["t4", "t3"]);
    /// # });
    /// ```
    #[doc(alias = "setMaxNumberOfStoredUnits")]
    pub fn set_max_number_of_stored_units(&mut self, max_units: usize, min_transactions: usize) {
        (self.max_units, self.min_transactions) = (max_units, min_transactions);
        self.trim();
    }

    /// Discards every undoable and redoable transaction, releasing the nodes they kept alive.
    ///
    /// ```
    /// # use syrp_var::Identifier; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let mut undo = syrp_state::UndoManager::new();
    /// let node = store.create(mt, Identifier::new("A"));
    /// store.set_property_with_undo(mt, &mut undo, &node, Identifier::new("x"), 1).unwrap();
    /// undo.clear_undo_history();
    /// assert!(!undo.can_undo());
    /// # });
    /// ```
    #[doc(alias = "clearUndoHistory")]
    pub fn clear_undo_history(&mut self) {
        let all: Vec<_> = self.undo.drain(..).chain(self.redo.drain(..)).collect();
        self.discard(all);
    }

    /// Whether an undo or redo is running. Listeners, which cannot reach the manager then,
    /// can ask [`Store::is_performing_undo_redo`] instead.
    ///
    /// ```
    /// assert!(!syrp_state::UndoManager::new().is_performing_undo_redo());
    /// ```
    #[doc(alias = "isPerformingUndoRedo")]
    #[must_use]
    pub fn is_performing_undo_redo(&self) -> bool {
        self.performing
    }

    /// Reverses the last transaction's items, newest first, and moves it to the redo list.
    /// Returns `Ok(false)` if there was nothing to undo or an action's `undo` failed.
    ///
    /// `Ok(false)` therefore has two meanings. Tell them apart with [`UndoManager::can_undo`]
    /// before the call: if it was `true`, the transaction was attempted, an action failed
    /// part-way, the items before it stay reversed, and the transaction is now on the redo list.
    ///
    /// Records are reversed through ordinary store mutations: they are logged and delivered
    /// to listeners, and not recorded. If the store was changed since without this manager so
    /// that a record no longer applies (say a removed child now has another parent), the error
    /// is returned. After an error or a failed action, the items reversed before it stay
    /// reversed and the transaction is still moved to the redo list.
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
        let Some(mut t) = self.undo.pop() else {
            return Ok(false);
        };
        let result = self.replay(mt, &mut t, true);
        self.redo.push(t);
        self.open = false;
        result
    }

    /// Re-applies the last undone transaction's items, oldest first, and moves it back to the
    /// undo list. Returns `Ok(false)` if there was nothing to redo or an action failed
    /// part-way (check [`UndoManager::can_redo`] before the call to tell which; after a failure
    /// the transaction is back on the undo list, partly re-applied). Errors as for
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
        let Some(mut t) = self.redo.pop() else {
            return Ok(false);
        };
        let result = self.replay(mt, &mut t, false);
        self.undo.push(t);
        self.open = false;
        result
    }

    /// Undoes the current transaction and discards it, so it cannot be redone. Does nothing,
    /// returning `Ok(false)`, when there is no current transaction: when
    /// [`UndoManager::begin_new_transaction`] was called since its last item (the new
    /// transaction has no items yet), or after an [`UndoManager::undo`] or
    /// [`UndoManager::redo`] (the top transaction is then history, not current work).
    ///
    /// ```
    /// # use syrp_var::Identifier; syrp_loop_api::ManualLoop::new().with_token(|mt| { let store = syrp_state::Store::new();
    /// let mut undo = syrp_state::UndoManager::new();
    /// let node = store.create(mt, Identifier::new("A"));
    /// store.set_property_with_undo(mt, &mut undo, &node, Identifier::new("x"), 1).unwrap();
    /// assert!(undo.undo_current_transaction_only(mt).unwrap());
    /// assert!(!undo.can_undo() && !undo.can_redo());
    /// # });
    /// ```
    #[doc(alias = "undoCurrentTransactionOnly")]
    pub fn undo_current_transaction_only(
        &mut self,
        mt: &MessageThread<'_>,
    ) -> Result<bool, StateError> {
        if self.next_name.is_some() || !self.open {
            return Ok(false);
        }
        let Some(mut t) = self.undo.pop() else {
            return Ok(false);
        };
        let result = self.replay(mt, &mut t, true);
        self.discard([t]);
        self.open = false;
        result
    }

    /// Applies a transaction's items backwards (`reverse`) or forwards.
    fn replay(
        &mut self,
        mt: &MessageThread<'_>,
        t: &mut Transaction,
        reverse: bool,
    ) -> Result<bool, StateError> {
        /// Sets the store's undoing flag; clears it on drop, even if a listener panics.
        struct Undoing(Option<Store>);
        impl Drop for Undoing {
            fn drop(&mut self) {
                if let Some(s) = &self.0 {
                    s.rc().borrow_mut().undoing = false;
                }
            }
        }
        let guard = Undoing(self.store.as_ref().map(Store::from_rc));
        if let Some(s) = &guard.0 {
            s.rc().borrow_mut().undoing = true;
        }
        self.performing = true;
        let step = |item: &mut Item| match item {
            Item::Record(c) => {
                let s = guard.0.as_ref().expect("records imply a bound store");
                apply(s, mt, c, reverse).map(|()| true)
            }
            Item::Action(a) if reverse => Ok(a.undo(mt)),
            Item::Action(a) => Ok(a.perform(mt)),
        };
        let mut result = Ok(true);
        let mut run = |item: &mut Item| {
            result = step(item);
            matches!(result, Ok(true))
        };
        if reverse {
            t.items.iter_mut().rev().all(&mut run);
        } else {
            t.items.iter_mut().all(&mut run);
        }
        self.performing = false;
        result
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

    /// The items of the transaction new work goes into: clears the redo list, and starts a
    /// transaction if one was begun or none exists (dropping a previous one that cancelled
    /// out, then applying the size limit).
    fn current(&mut self) -> &mut Vec<Item> {
        let redo = std::mem::take(&mut self.redo);
        self.discard(redo);
        let name = match self.next_name.take() {
            Some(name) => Some(name),
            None if self.undo.is_empty() || !self.open => Some(String::new()),
            None => None,
        };
        if let Some(name) = name {
            if self.undo.last().is_some_and(Transaction::is_no_op) {
                let dropped = self.undo.pop();
                self.discard(dropped);
            }
            self.undo.push(Transaction {
                name,
                items: Vec::new(),
            });
            self.open = true;
            self.trim();
        }
        &mut self.undo.last_mut().expect("just ensured").items
    }

    /// Drops the oldest undoable transactions while over the unit limit, keeping at least
    /// `min_transactions` and the current one.
    fn trim(&mut self) {
        let keep = self.min_transactions.max(1);
        let mut units = self.number_of_units_taken_up();
        let mut dropped = 0;
        while self.undo.len() - dropped > keep && units > self.max_units {
            units -= self.undo[dropped].units();
            dropped += 1;
        }
        let old: Vec<_> = self.undo.drain(..dropped).collect();
        self.discard(old);
    }

    /// Adds records captured from one mutation, each already holding its counts.
    pub(crate) fn add(&mut self, records: Vec<Change>) {
        if records.is_empty() {
            return;
        }
        let items = self.current();
        let mut merged = Vec::new();
        for change in records {
            match (items.last_mut(), change) {
                (
                    Some(Item::Record(Change::PropertyChanged {
                        node: n0,
                        id: i0,
                        new: last,
                        ..
                    })),
                    Change::PropertyChanged { node, id, old, new },
                ) if *n0 == node && *i0 == id => {
                    *last = new;
                    merged.push(Item::Record(Change::PropertyChanged {
                        node,
                        id,
                        old,
                        new: Var::default(),
                    }));
                }
                (_, change) => items.push(Item::Record(change)),
            }
        }
        self.discard([Transaction {
            name: String::new(),
            items: merged,
        }]);
    }

    /// Releases the counts held by `transactions`' records, then drops their items outside
    /// the store's borrow (an action may own handles).
    fn discard(&self, transactions: impl IntoIterator<Item = Transaction>) {
        let all: Vec<Transaction> = transactions.into_iter().collect();
        if let Some(store) = &self.store {
            let mut inner = store.borrow_mut();
            for item in all.iter().flat_map(|t| &t.items) {
                if let Item::Record(c) = item {
                    inner.unpin(c);
                }
            }
        }
        drop(all);
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
        // A listener's nested `*_with_undo` call (with another manager) appends after `mark`,
        // so each call takes only its own records.
        let mark = {
            let mut inner = self.rc().borrow_mut();
            inner.capture_from = Some(inner.next_seq);
            inner.captured.len()
        };
        let result = f();
        let records = {
            let mut inner = self.rc().borrow_mut();
            inner.capture_from = None;
            inner.captured.split_off(mark)
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

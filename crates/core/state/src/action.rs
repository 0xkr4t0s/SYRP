//! [`UndoableAction`]: undoable work outside the store, and the unit sizes of undo items.

use std::any::Any;

use syrp_loop_api::MessageThread;
use syrp_var::{Var, VarView};

use crate::log::Change;

/// Undoable work that is not a store mutation, added to a transaction with
/// [`UndoManager::perform`](crate::UndoManager::perform).
///
/// Actions and store records interleave in the order they were made within a transaction.
/// [`UndoableAction::coalesce_with`] is how an action absorbs the action performed straight
/// after it in the same transaction; the trait requires `Any` so an implementation can
/// downcast `next` to its own type (`(next as &dyn Any).downcast_ref::<Self>()`).
///
/// ```
/// use std::{cell::Cell, rc::Rc};
/// use syrp_loop_api::MessageThread;
/// use syrp_state::{UndoManager, UndoableAction};
///
/// struct Add(Rc<Cell<i32>>, i32);
/// impl UndoableAction for Add {
///     fn perform(&mut self, _: &MessageThread<'_>) -> bool { self.0.set(self.0.get() + self.1); true }
///     fn undo(&mut self, _: &MessageThread<'_>) -> bool { self.0.set(self.0.get() - self.1); true }
/// }
///
/// syrp_loop_api::ManualLoop::new().with_token(|mt| {
///     let total = Rc::new(Cell::new(0));
///     let mut undo = UndoManager::new();
///     assert!(undo.perform(mt, Add(total.clone(), 5)));
///     assert_eq!(total.get(), 5);
///     undo.undo(mt).unwrap();
///     assert_eq!(total.get(), 0);
/// });
/// ```
pub trait UndoableAction: Any {
    /// Does the work. Returning `false` means it failed, and the action is not recorded.
    ///
    /// ```
    /// # use syrp_state::UndoableAction;
    /// struct Never;
    /// impl UndoableAction for Never {
    ///     fn perform(&mut self, _: &syrp_loop_api::MessageThread<'_>) -> bool { false }
    ///     fn undo(&mut self, _: &syrp_loop_api::MessageThread<'_>) -> bool { false }
    /// }
    /// syrp_loop_api::ManualLoop::new().with_token(|mt| {
    ///     let mut undo = syrp_state::UndoManager::new();
    ///     assert!(!undo.perform(mt, Never) && !undo.can_undo());
    /// });
    /// ```
    fn perform(&mut self, mt: &MessageThread<'_>) -> bool;

    /// Reverses [`UndoableAction::perform`]. Returning `false` stops the undo or redo.
    ///
    /// ```
    /// # use syrp_state::UndoableAction;
    /// struct Noop;
    /// impl UndoableAction for Noop {
    ///     fn perform(&mut self, _: &syrp_loop_api::MessageThread<'_>) -> bool { true }
    ///     fn undo(&mut self, _: &syrp_loop_api::MessageThread<'_>) -> bool { true }
    /// }
    /// syrp_loop_api::ManualLoop::new().with_token(|mt| assert!(Noop.undo(mt)));
    /// ```
    fn undo(&mut self, mt: &MessageThread<'_>) -> bool;

    /// The space it takes, in the units of
    /// [`UndoManager::set_max_number_of_stored_units`](crate::UndoManager::set_max_number_of_stored_units).
    /// Defaults to 10.
    ///
    /// ```
    /// # use syrp_state::UndoableAction;
    /// struct Noop;
    /// impl UndoableAction for Noop {
    ///     fn perform(&mut self, _: &syrp_loop_api::MessageThread<'_>) -> bool { true }
    ///     fn undo(&mut self, _: &syrp_loop_api::MessageThread<'_>) -> bool { true }
    /// }
    /// assert_eq!(Noop.size_in_units(), 10);
    /// ```
    fn size_in_units(&self) -> usize {
        10
    }

    /// Tries to absorb `next`, the action performed just after this one in the same
    /// transaction, so the two undo as one. Returns `true` if this action now covers both
    /// (`next` is then dropped without being recorded). Defaults to `false`.
    ///
    /// ```
    /// # use std::any::Any;
    /// # use syrp_state::UndoableAction;
    /// struct Nudge(i32);
    /// impl UndoableAction for Nudge {
    ///     fn perform(&mut self, _: &syrp_loop_api::MessageThread<'_>) -> bool { true }
    ///     fn undo(&mut self, _: &syrp_loop_api::MessageThread<'_>) -> bool { true }
    ///     fn coalesce_with(&mut self, next: &dyn UndoableAction) -> bool {
    ///         let Some(next) = (next as &dyn Any).downcast_ref::<Nudge>() else { return false };
    ///         self.0 += next.0;
    ///         true
    ///     }
    /// }
    /// let mut a = Nudge(1);
    /// assert!(a.coalesce_with(&Nudge(2)) && a.0 == 3);
    /// ```
    #[doc(alias = "createCoalescedAction")]
    fn coalesce_with(&mut self, next: &dyn UndoableAction) -> bool {
        let _ = next;
        false
    }
}

/// One item of a transaction.
pub(crate) enum Item {
    /// A store record, counting the nodes it names.
    Record(Change),
    /// Work outside the store.
    Action(Box<dyn UndoableAction>),
}

impl Item {
    /// Units: an action's own size; a record 1 plus 1 per 64 bytes of `Var` payload.
    pub(crate) fn units(&self) -> usize {
        let record = |vars: &[&Var]| 1 + vars.iter().map(|v| payload(v)).sum::<usize>() / 64;
        match self {
            Self::Action(a) => a.size_in_units(),
            Self::Record(Change::PropertyChanged { old, new, .. }) => match old {
                Some(old) => record(&[old, new]),
                None => record(&[new]),
            },
            Self::Record(Change::PropertyRemoved { old, .. }) => record(&[old]),
            Self::Record(_) => 1,
        }
    }
}

/// Bytes a value carries beyond its 16-byte slot: text and binary lengths, and for arrays
/// 16 bytes per element plus the elements' own payload. Objects are shared and count 0.
fn payload(v: &Var) -> usize {
    let mut stack = vec![v];
    let mut bytes = 0;
    while let Some(v) = stack.pop() {
        bytes += match v.as_view() {
            VarView::Str(s) => s.len(),
            VarView::Binary(b) => b.len(),
            VarView::Array(items) => {
                stack.extend(items);
                16 * items.len()
            }
            _ => 0,
        };
    }
    bytes
}

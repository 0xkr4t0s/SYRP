//! The Syrp state store (master plan §5.9, ADR 0003 D2): the data-oriented core that
//! `syrp-compat-state`'s `ValueTree` is later built over.
//!
//! A [`Store`] is one state document: a slotmap arena of nodes. Each node has a type
//! [`Identifier`](syrp_var::Identifier), properties as a small vector of
//! `(Identifier, Var)` pairs kept sorted by identifier, and an ordered list of children.
//! Nodes are reached through counted [`NodeRef`] handles (shallow clone, identity equality) or
//! uncounted `Copy` [`NodeKey`]s that [`Store::upgrade`] turns back into handles.
//!
//! Every operation is a [`Store`] method that takes the node as `&NodeRef`; mutations also take
//! the [`MessageThread`](syrp_loop_api::MessageThread) token. Misuse returns a coded
//! [`StateError`] and leaves the store unchanged.
//!
//! ## Reference counts and the change log
//! A node's count is its live handles and subscriptions, one for its parent link and one per
//! undelivered [`LogRecord`] naming it. At zero the slot is freed, and each child loses its
//! parent link and that count. A detached subtree lives while held and is reclaimed after. A
//! parent freed while a child is held leaves the child parentless (a Syrp choice).
//!
//! Every mutation appends [`Change`] records, then delivers them to [`Subscription`] listeners
//! once its borrow is released. [`LogCursor`]s read the log; see [`Store::subscribe`] for the
//! delivery rules. [`Store::deep_copy`] and [`Store::is_equivalent`] work on whole subtrees.
//!
//! Each logged mutation has a `*_with_undo` sibling that adds its records to an
//! [`UndoManager`] transaction, for [`UndoManager::undo`] and [`UndoManager::redo`]; work outside
//! the store joins a transaction as an [`UndoableAction`].
//!
//! ```
//! use syrp_state::Store;
//! use syrp_var::{Identifier, Var};
//!
//! syrp_loop_api::ManualLoop::new().with_token(|mt| {
//!     let store = Store::new();
//!     let root = store.create(mt, Identifier::new("Root"));
//!     let voice = store.create(mt, Identifier::new("Voice"));
//!     store.add_child(mt, &root, &voice, None).unwrap();
//!     store.set_property(mt, &voice, Identifier::new("gain"), 0.5).unwrap();
//!     let first = store.child(&root, 0).unwrap().unwrap();
//!     assert_eq!(store.property(&first, Identifier::new("gain")).unwrap(), Some(Var::from(0.5)));
//! });
//! ```
//!
//! The store is `!Send`:
//! ```compile_fail
//! fn needs_send<T: Send>(_: T) {}
//! needs_send(syrp_state::Store::new());
//! ```

mod action;
mod copy;
mod delivery;
mod error;
mod log;
mod node;
mod store;
mod undo;

pub use action::UndoableAction;
pub use delivery::Subscription;
pub use error::{StateError, StateErrorKind};
pub use log::{Change, LogCursor, LogRecord};
pub use node::{NodeKey, NodeRef, Properties};
pub use store::Store;
pub use syrp_error::Diagnostic;
pub use undo::UndoManager;

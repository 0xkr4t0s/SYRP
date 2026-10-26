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
//! ## Reference counts
//! A node's count is the number of its live handles plus one for its parent link. At zero the
//! slot is freed, and each child loses its parent link and that link's count, which may free
//! it in turn. So a detached subtree lives while something holds it and is reclaimed after. A
//! parent freed while one of its children is still held leaves that child parentless; this is
//! a Syrp choice, not yet checked against JUCE.
//!
//! This is increment 1 of WP-1.2-03 (spec B1 to B3, minus the B1 operations listed in the
//! crate's `AGENTS.md`). The change log, listeners, deep copies and equivalence come later.
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

mod error;
mod node;
mod store;

pub use error::{StateError, StateErrorKind};
pub use node::{NodeKey, NodeRef, Properties};
pub use store::Store;
pub use syrp_error::Diagnostic;

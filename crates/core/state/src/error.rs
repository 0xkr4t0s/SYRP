//! [`StateError`] and its `SYRP-STATE-*` codes (catalogued in `errors/state.toml`).

use std::fmt;

use syrp_error::{Diagnostic, ErrorCode};

/// What went wrong in a [`Store`](crate::Store) call. Each kind has its own catalogued code.
///
/// ```
/// use syrp_state::{NodeKey, StateErrorKind, Store};
/// let err = Store::new().upgrade(NodeKey::default()).unwrap_err();
/// assert!(matches!(err.kind(), StateErrorKind::DeadKey));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StateErrorKind {
    /// `SYRP-STATE-0001`: the child passed to `add_child` already has a parent.
    ChildHasParent,
    /// `SYRP-STATE-0002`: `add_child` would put a node under itself or its own descendant.
    WouldCreateCycle,
    /// `SYRP-STATE-0003`: a `NodeRef` from another store was passed in.
    ForeignNode,
    /// `SYRP-STATE-0004`: `upgrade` was given the key of a freed node.
    DeadKey,
    /// `SYRP-STATE-0005`: `remove_child` was given a node that is not a child of the parent.
    NotAChild,
    /// `SYRP-STATE-0006`: `move_child` was given a source index past the last child.
    IndexOutOfRange,
}

impl StateErrorKind {
    /// The catalogued code for this kind.
    ///
    /// ```
    /// use syrp_state::StateErrorKind;
    /// assert_eq!(StateErrorKind::DeadKey.code().as_str(), "SYRP-STATE-0004");
    /// ```
    #[must_use]
    pub const fn code(self) -> ErrorCode {
        ErrorCode::new(match self {
            Self::ChildHasParent => "SYRP-STATE-0001",
            Self::WouldCreateCycle => "SYRP-STATE-0002",
            Self::ForeignNode => "SYRP-STATE-0003",
            Self::DeadKey => "SYRP-STATE-0004",
            Self::NotAChild => "SYRP-STATE-0005",
            Self::IndexOutOfRange => "SYRP-STATE-0006",
        })
    }

    fn message(self) -> &'static str {
        match self {
            Self::ChildHasParent => "the child already has a parent",
            Self::WouldCreateCycle => "adding the child would make a node its own ancestor",
            Self::ForeignNode => "the node belongs to another store",
            Self::DeadKey => "the node key refers to a freed node",
            Self::NotAChild => "the node is not a child of this parent",
            Self::IndexOutOfRange => "the child index is out of range",
        }
    }
}

/// An error from a [`Store`](crate::Store) call. The store is unchanged after any error.
///
/// ```
/// use syrp_state::{Diagnostic, NodeKey, Store};
/// let err = Store::new().upgrade(NodeKey::default()).unwrap_err();
/// assert_eq!(err.code().as_str(), "SYRP-STATE-0004");
/// assert!(err.to_string().starts_with("SYRP-STATE-0004: "));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StateError {
    kind: StateErrorKind,
}

impl StateError {
    pub(crate) const fn new(kind: StateErrorKind) -> Self {
        Self { kind }
    }

    /// What went wrong.
    ///
    /// ```
    /// use syrp_state::{StateErrorKind, Store};
    /// let store = Store::new();
    /// let err = store.upgrade(syrp_state::NodeKey::default()).unwrap_err();
    /// assert_eq!(err.kind(), StateErrorKind::DeadKey);
    /// ```
    #[must_use]
    pub const fn kind(&self) -> StateErrorKind {
        self.kind
    }
}

impl fmt::Display for StateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind.code(), self.kind.message())
    }
}

impl std::error::Error for StateError {}

impl Diagnostic for StateError {
    fn code(&self) -> ErrorCode {
        self.kind.code()
    }
}

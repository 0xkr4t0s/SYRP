//! Property containers: [`NamedValueSet`] and [`DynamicObject`].
//!
//! This increment defines the types so that [`Var`] can hold objects; the property API
//! (`get`, `set`, `remove`, iteration and the `DynamicObject` property methods) arrives with
//! B5 in the next increment.

use std::cell::RefCell;
use std::fmt;

use crate::{Identifier, Var};

/// An ordered set of `(Identifier, Var)` pairs, kept in insertion order.
///
/// It is a small vector, not a hash map: property lists are short and a linear scan of 4-byte
/// identifiers is fast. Like [`Var`], it is neither `Send` nor `Sync`.
///
/// ```
/// use syrp_var::NamedValueSet;
/// let set = NamedValueSet::new();
/// assert!(set.is_empty());
/// assert_eq!(set.len(), 0);
/// ```
#[derive(Clone, Default)]
pub struct NamedValueSet {
    values: Vec<(Identifier, Var)>,
}

impl NamedValueSet {
    /// Returns an empty set. Does not allocate.
    ///
    /// ```
    /// assert!(syrp_var::NamedValueSet::new().is_empty());
    /// ```
    #[must_use]
    pub const fn new() -> Self {
        Self { values: Vec::new() }
    }

    /// Returns the number of named values.
    ///
    /// ```
    /// assert_eq!(syrp_var::NamedValueSet::new().len(), 0);
    /// ```
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns whether the set holds no values.
    ///
    /// ```
    /// assert!(syrp_var::NamedValueSet::new().is_empty());
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

impl fmt::Debug for NamedValueSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map()
            .entries(self.values.iter().map(|(name, value)| (name, value)))
            .finish()
    }
}

/// An object with named properties, shared by reference through `Rc<DynamicObject>`.
///
/// A [`Var`] holding an object refers to it: cloning the `Var` shares the same object.
/// Properties sit behind a `RefCell`, so the object can be changed through a shared `Rc`; this
/// also makes it neither `Send` nor `Sync` (see the crate documentation on threads).
///
/// ```
/// use std::rc::Rc;
/// use syrp_var::{DynamicObject, Var};
///
/// let object = Rc::new(DynamicObject::new());
/// let value = Var::from(Rc::clone(&object));
/// assert!(value.is_object());
/// assert!(Rc::ptr_eq(value.as_object().unwrap(), &object));
/// ```
#[derive(Default)]
pub struct DynamicObject {
    properties: RefCell<NamedValueSet>,
}

impl DynamicObject {
    /// Returns an object with no properties.
    ///
    /// ```
    /// assert_eq!(syrp_var::DynamicObject::new().property_count(), 0);
    /// ```
    #[must_use]
    pub const fn new() -> Self {
        Self {
            properties: RefCell::new(NamedValueSet::new()),
        }
    }

    /// Returns the number of properties.
    ///
    /// ```
    /// assert_eq!(syrp_var::DynamicObject::new().property_count(), 0);
    /// ```
    #[must_use]
    pub fn property_count(&self) -> usize {
        self.properties.borrow().len()
    }
}

impl fmt::Debug for DynamicObject {
    /// Prints the property count only: objects can refer to themselves, so printing their
    /// contents could recurse forever.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = f.debug_struct("DynamicObject");
        match self.properties.try_borrow() {
            Ok(properties) => out.field("properties", &properties.len()),
            Err(_) => out.field("properties", &"<borrowed>"),
        };
        out.finish_non_exhaustive()
    }
}

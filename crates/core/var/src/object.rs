//! B5: property containers, [`NamedValueSet`] and [`DynamicObject`].

use std::cell::RefCell;
use std::fmt;

use crate::{Identifier, Var};

/// An ordered set of `(Identifier, Var)` pairs, kept in insertion order.
///
/// It is a small vector, not a hash map: property lists are short and a linear scan of 4-byte
/// identifiers is fast. Like [`Var`], it is neither `Send` nor `Sync`.
///
/// ```
/// use syrp_var::{Identifier, NamedValueSet, Var};
///
/// let (width, height) = (Identifier::new("width"), Identifier::new("height"));
/// let mut set = NamedValueSet::new();
/// assert!(set.set(width, 640));
/// assert!(set.set(height, 480));
/// assert!(!set.set(width, 640), "unchanged");
/// assert_eq!(set.get(width).map(Var::as_i32), Some(640));
/// let names: Vec<_> = set.iter().map(|(name, _)| name.as_str()).collect();
/// assert_eq!(names, ["width", "height"]);
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
    /// let mut set = syrp_var::NamedValueSet::new();
    /// set.set(syrp_var::Identifier::new("a"), 1);
    /// assert_eq!(set.len(), 1);
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

    /// Returns the value named `name`, if present.
    ///
    /// ```
    /// use syrp_var::{Identifier, NamedValueSet};
    /// let mut set = NamedValueSet::new();
    /// set.set(Identifier::new("gain"), 0.5);
    /// assert_eq!(set.get(Identifier::new("gain")).unwrap().as_f64(), 0.5);
    /// assert!(set.get(Identifier::new("pan")).is_none());
    /// ```
    #[must_use]
    pub fn get(&self, name: Identifier) -> Option<&Var> {
        self.values.iter().find(|(n, _)| *n == name).map(|(_, v)| v)
    }

    /// Returns whether a value named `name` is present.
    ///
    /// ```
    /// use syrp_var::{Identifier, NamedValueSet};
    /// let mut set = NamedValueSet::new();
    /// set.set(Identifier::new("on"), true);
    /// assert!(set.contains(Identifier::new("on")));
    /// ```
    #[must_use]
    pub fn contains(&self, name: Identifier) -> bool {
        self.get(name).is_some()
    }

    /// Sets the value named `name`, appending it if absent and replacing it in place if
    /// present. Returns whether anything changed: false only when the old value had the same
    /// variant and was equal ([`Var::eq_same_type`]), so `2` replaced by `2.0` is a change.
    ///
    /// ```
    /// use syrp_var::{Identifier, NamedValueSet};
    /// let mut set = NamedValueSet::new();
    /// let n = Identifier::new("n");
    /// assert!(set.set(n, 2));
    /// assert!(!set.set(n, 2));
    /// assert!(set.set(n, 2.0));
    /// ```
    pub fn set(&mut self, name: Identifier, value: impl Into<Var>) -> bool {
        let value = value.into();
        match self.values.iter_mut().find(|(n, _)| *n == name) {
            Some((_, old)) if old.eq_same_type(&value) => false,
            Some((_, old)) => {
                *old = value;
                true
            }
            None => {
                self.values.push((name, value));
                true
            }
        }
    }

    /// Removes the value named `name` and returns it; the others keep their order.
    ///
    /// ```
    /// use syrp_var::{Identifier, NamedValueSet};
    /// let mut set = NamedValueSet::new();
    /// set.set(Identifier::new("a"), 1);
    /// assert_eq!(set.remove(Identifier::new("a")).unwrap().as_i32(), 1);
    /// assert!(set.remove(Identifier::new("a")).is_none());
    /// ```
    pub fn remove(&mut self, name: Identifier) -> Option<Var> {
        let index = self.values.iter().position(|(n, _)| *n == name)?;
        Some(self.values.remove(index).1)
    }

    /// Iterates over `(name, value)` pairs in insertion order.
    ///
    /// ```
    /// use syrp_var::{Identifier, NamedValueSet};
    /// let mut set = NamedValueSet::new();
    /// set.set(Identifier::new("b"), 2);
    /// set.set(Identifier::new("a"), 1);
    /// let total: i32 = set.iter().map(|(_, v)| v.as_i32()).sum();
    /// assert_eq!(total, 3);
    /// ```
    pub fn iter(&self) -> NamedValueIter<'_> {
        NamedValueIter {
            inner: self.values.iter(),
        }
    }
}

impl fmt::Debug for NamedValueSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

/// The iterator returned by [`NamedValueSet::iter`], yielding `(Identifier, &Var)`.
///
/// ```
/// use syrp_var::{Identifier, NamedValueSet};
/// let mut set = NamedValueSet::new();
/// set.set(Identifier::new("k"), "v");
/// let mut iter: syrp_var::NamedValueIter<'_> = set.iter();
/// assert_eq!(iter.next().unwrap().1.as_str(), Some("v"));
/// assert!(iter.next().is_none());
/// ```
#[derive(Clone, Debug)]
pub struct NamedValueIter<'a> {
    inner: std::slice::Iter<'a, (Identifier, Var)>,
}

impl<'a> Iterator for NamedValueIter<'a> {
    type Item = (Identifier, &'a Var);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(name, value)| (*name, value))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl ExactSizeIterator for NamedValueIter<'_> {}

impl<'a> IntoIterator for &'a NamedValueSet {
    type Item = (Identifier, &'a Var);
    type IntoIter = NamedValueIter<'a>;

    fn into_iter(self) -> NamedValueIter<'a> {
        self.iter()
    }
}

/// An object with named properties, shared by reference through `Rc<DynamicObject>`.
///
/// A [`Var`] holding an object refers to it: cloning the `Var` shares the same object, and a
/// property set through one handle is seen through all of them. Use [`Var::deep_clone`] for
/// an independent copy. Properties sit behind a `RefCell`, so they change through `&self`;
/// this also makes the object neither `Send` nor `Sync` (see the crate documentation on
/// threads). An object that holds a `Var` referring to itself is a reference cycle and is
/// never freed.
///
/// ```
/// use syrp_var::{Identifier, Var};
///
/// let a = Var::new_object();
/// let b = a.clone();
/// a.as_object().unwrap().set_property(Identifier::new("x"), 1);
/// assert_eq!(b.as_object().unwrap().get_property(Identifier::new("x")).as_i32(), 1);
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

    /// Returns whether the object has a property named `name`.
    ///
    /// ```
    /// use syrp_var::{DynamicObject, Identifier};
    /// let object = DynamicObject::new();
    /// object.set_property(Identifier::new("id"), 7);
    /// assert!(object.has_property(Identifier::new("id")));
    /// ```
    #[must_use]
    pub fn has_property(&self, name: Identifier) -> bool {
        self.properties.borrow().contains(name)
    }

    /// Returns a copy of the property named `name` (O(1)), or void if there is none.
    ///
    /// ```
    /// use syrp_var::{DynamicObject, Identifier};
    /// let object = DynamicObject::new();
    /// object.set_property(Identifier::new("name"), "osc");
    /// assert_eq!(object.get_property(Identifier::new("name")).as_str(), Some("osc"));
    /// assert!(object.get_property(Identifier::new("missing")).is_void());
    /// ```
    #[must_use]
    pub fn get_property(&self, name: Identifier) -> Var {
        self.properties
            .borrow()
            .get(name)
            .cloned()
            .unwrap_or_default()
    }

    /// Sets a property; returns whether it changed, as [`NamedValueSet::set`] does.
    ///
    /// ```
    /// use syrp_var::{DynamicObject, Identifier};
    /// let object = DynamicObject::new();
    /// assert!(object.set_property(Identifier::new("n"), 1));
    /// assert!(!object.set_property(Identifier::new("n"), 1));
    /// ```
    pub fn set_property(&self, name: Identifier, value: impl Into<Var>) -> bool {
        self.properties.borrow_mut().set(name, value)
    }

    /// Removes the property named `name` and returns its value.
    ///
    /// ```
    /// use syrp_var::{DynamicObject, Identifier};
    /// let object = DynamicObject::new();
    /// object.set_property(Identifier::new("tmp"), true);
    /// assert!(object.remove_property(Identifier::new("tmp")).is_some());
    /// assert!(!object.has_property(Identifier::new("tmp")));
    /// ```
    pub fn remove_property(&self, name: Identifier) -> Option<Var> {
        self.properties.borrow_mut().remove(name)
    }

    /// Returns a snapshot of the properties, in insertion order. Each value is an O(1) copy;
    /// objects inside stay shared.
    ///
    /// ```
    /// use syrp_var::{DynamicObject, Identifier};
    /// let object = DynamicObject::new();
    /// object.set_property(Identifier::new("a"), 1);
    /// assert_eq!(object.properties().len(), 1);
    /// ```
    #[must_use]
    pub fn properties(&self) -> NamedValueSet {
        self.properties.borrow().clone()
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

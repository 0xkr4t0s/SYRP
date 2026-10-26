//! B4: copy-on-write arrays and deep copies.

use std::collections::HashMap;
use std::rc::Rc;

use crate::var::Repr;
use crate::{DynamicObject, Var};

impl Var {
    /// Mutably borrows the items of an array value; `None` for every other variant.
    ///
    /// Arrays have value semantics: if other `Var`s share these items, they are copied first
    /// (copy on write), so the change is visible through this value only.
    ///
    /// ```
    /// use syrp_var::Var;
    /// let mut a = Var::from(vec![Var::from(1)]);
    /// let b = a.clone();
    /// a.array_mut().unwrap().push(Var::from(2));
    /// assert_eq!(a.as_array().unwrap().len(), 2);
    /// assert_eq!(b.as_array().unwrap().len(), 1);
    /// ```
    #[doc(alias = "getArray")]
    #[must_use]
    pub fn array_mut(&mut self) -> Option<&mut Vec<Var>> {
        match &mut self.repr {
            Repr::Array(items) => Some(Rc::make_mut(items)),
            _ => None,
        }
    }

    /// Returns a copy in which every array and object is copied recursively.
    ///
    /// Plain [`Clone`] shares objects by reference; `deep_clone` makes new objects. An object
    /// reached twice is copied once, so sharing (and any cycle) inside the value is kept in the
    /// copy, which therefore never recurses forever. Strings and binary data are immutable and
    /// stay shared.
    ///
    /// ```
    /// use syrp_var::{Identifier, Var};
    /// let original = Var::new_object();
    /// let copy = original.deep_clone();
    /// copy.as_object().unwrap().set_property(Identifier::new("x"), 1);
    /// assert!(!original.as_object().unwrap().has_property(Identifier::new("x")));
    /// ```
    #[must_use]
    pub fn deep_clone(&self) -> Var {
        deep_clone(self, &mut HashMap::new())
    }
}

/// Copies `value`, mapping each source object to its single copy in `copies`.
fn deep_clone(value: &Var, copies: &mut HashMap<*const DynamicObject, Rc<DynamicObject>>) -> Var {
    match &value.repr {
        Repr::Array(items) => Var::from(
            items
                .iter()
                .map(|item| deep_clone(item, copies))
                .collect::<Vec<_>>(),
        ),
        Repr::Object(object) => {
            if let Some(copy) = copies.get(&Rc::as_ptr(object)) {
                return Var::from(Rc::clone(copy));
            }
            let copy = Rc::new(DynamicObject::new());
            copies.insert(Rc::as_ptr(object), Rc::clone(&copy));
            // A snapshot, so the source may be the copy's own property without a borrow clash.
            for (name, item) in &object.properties() {
                copy.set_property(name, deep_clone(item, copies));
            }
            Var::from(copy)
        }
        _ => value.clone(),
    }
}

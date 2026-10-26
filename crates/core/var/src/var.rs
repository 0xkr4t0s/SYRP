//! The [`Var`] value type and its borrowed [`VarView`].

use std::fmt;
use std::rc::Rc;

use crate::DynamicObject;

/// The storage behind a [`Var`]. Heap variants hold one thin `Rc` pointer, which keeps `Var` at
/// 16 bytes on 64-bit targets.
#[derive(Clone)]
pub(crate) enum Repr {
    Void,
    Undefined,
    Bool(bool),
    Int(i32),
    Int64(i64),
    Double(f64),
    Str(Rc<String>),
    Array(Rc<Vec<Var>>),
    Object(Rc<DynamicObject>),
    Binary(Rc<Vec<u8>>),
}

/// A dynamically typed value: void, undefined, a bool, an `i32`, an `i64`, an `f64`, a string,
/// an array of `Var`s, a shared [`DynamicObject`] or a block of bytes.
///
/// `Var` is 16 bytes on 64-bit targets and cloning it is O(1): heap variants share their data
/// through `Rc`. It is neither `Send` nor `Sync` (see the crate documentation on threads).
/// The default value is void.
///
/// Build one with a `From` impl or a named constructor, test its type with the `is_*`
/// predicates, and read it with the borrowing accessors (`as_str`, `as_array`, `as_object`,
/// `as_bytes`) or match on [`Var::as_view`], and convert it with the coercing accessors
/// (`as_bool`, `as_i32`, `as_i64`, `as_f64`, `to_text`), whose rules are in the crate
/// documentation.
///
/// ```
/// use syrp_var::Var;
///
/// let n = Var::from(42);
/// assert!(n.is_i32());
///
/// let s = Var::from("7 bars");
/// assert_eq!(s.as_str(), Some("7 bars"));
/// assert_eq!(std::mem::size_of::<Var>(), 16);
/// ```
#[doc(alias = "var")]
#[derive(Clone)]
pub struct Var {
    pub(crate) repr: Repr,
}

/// A borrowed, `Copy` view of a [`Var`], for matching on its variant.
///
/// ```
/// use syrp_var::{Var, VarView};
///
/// fn describe(value: &Var) -> String {
///     match value.as_view() {
///         VarView::Int(n) => format!("int {n}"),
///         VarView::Str(s) => format!("text {s}"),
///         VarView::Array(items) => format!("{} items", items.len()),
///         _ => "other".to_owned(),
///     }
/// }
/// assert_eq!(describe(&Var::from(3)), "int 3");
/// assert_eq!(describe(&Var::from("a")), "text a");
/// ```
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum VarView<'a> {
    /// No value (JUCE's default `var`).
    Void,
    /// An explicitly undefined value.
    Undefined,
    /// A boolean.
    Bool(bool),
    /// A 32-bit integer.
    Int(i32),
    /// A 64-bit integer.
    Int64(i64),
    /// A double-precision float.
    Double(f64),
    /// Text.
    Str(&'a str),
    /// An array of values.
    Array(&'a [Var]),
    /// A shared object.
    Object(&'a Rc<DynamicObject>),
    /// A block of bytes.
    Binary(&'a [u8]),
}

impl Var {
    /// Returns the void value, which is also [`Var::default`].
    ///
    /// ```
    /// use syrp_var::Var;
    /// assert!(Var::void().is_void());
    /// assert!(Var::default().is_void());
    /// ```
    #[must_use]
    pub const fn void() -> Self {
        Self { repr: Repr::Void }
    }

    /// Returns the undefined value, which is distinct from void.
    ///
    /// ```
    /// use syrp_var::Var;
    /// assert!(Var::undefined().is_undefined());
    /// assert!(!Var::undefined().is_void());
    /// ```
    #[must_use]
    pub const fn undefined() -> Self {
        Self {
            repr: Repr::Undefined,
        }
    }

    /// Returns a binary value holding `bytes`.
    ///
    /// ```
    /// use syrp_var::Var;
    /// let v = Var::from_bytes(vec![1, 2, 3]);
    /// assert_eq!(v.as_bytes(), Some(&[1u8, 2, 3][..]));
    /// ```
    #[must_use]
    pub fn from_bytes(bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            repr: Repr::Binary(Rc::new(bytes.into())),
        }
    }

    /// Returns a value holding a new, empty [`DynamicObject`].
    ///
    /// ```
    /// use syrp_var::Var;
    /// let v = Var::new_object();
    /// assert_eq!(v.as_object().unwrap().property_count(), 0);
    /// ```
    #[must_use]
    pub fn new_object() -> Self {
        Self::from(Rc::new(DynamicObject::new()))
    }

    /// Lends a `Copy` view of the value for matching.
    ///
    /// ```
    /// use syrp_var::{Var, VarView};
    /// assert!(matches!(Var::from(true).as_view(), VarView::Bool(true)));
    /// ```
    #[must_use]
    pub fn as_view(&self) -> VarView<'_> {
        match &self.repr {
            Repr::Void => VarView::Void,
            Repr::Undefined => VarView::Undefined,
            Repr::Bool(b) => VarView::Bool(*b),
            Repr::Int(n) => VarView::Int(*n),
            Repr::Int64(n) => VarView::Int64(*n),
            Repr::Double(x) => VarView::Double(*x),
            Repr::Str(s) => VarView::Str(s.as_str()),
            Repr::Array(items) => VarView::Array(items.as_slice()),
            Repr::Object(object) => VarView::Object(object),
            Repr::Binary(bytes) => VarView::Binary(bytes.as_slice()),
        }
    }

    /// Returns whether the value is void.
    ///
    /// ```
    /// assert!(syrp_var::Var::void().is_void());
    /// ```
    #[must_use]
    pub fn is_void(&self) -> bool {
        matches!(self.repr, Repr::Void)
    }

    /// Returns whether the value is undefined.
    ///
    /// ```
    /// assert!(syrp_var::Var::undefined().is_undefined());
    /// ```
    #[must_use]
    pub fn is_undefined(&self) -> bool {
        matches!(self.repr, Repr::Undefined)
    }

    /// Returns whether the value is a bool.
    ///
    /// ```
    /// assert!(syrp_var::Var::from(false).is_bool());
    /// ```
    #[must_use]
    pub fn is_bool(&self) -> bool {
        matches!(self.repr, Repr::Bool(_))
    }

    /// Returns whether the value is an `i32`.
    ///
    /// ```
    /// assert!(syrp_var::Var::from(1).is_i32());
    /// ```
    #[must_use]
    pub fn is_i32(&self) -> bool {
        matches!(self.repr, Repr::Int(_))
    }

    /// Returns whether the value is an `i64`.
    ///
    /// ```
    /// assert!(syrp_var::Var::from(1_i64).is_i64());
    /// ```
    #[must_use]
    pub fn is_i64(&self) -> bool {
        matches!(self.repr, Repr::Int64(_))
    }

    /// Returns whether the value is an `f64`.
    ///
    /// ```
    /// assert!(syrp_var::Var::from(0.5).is_f64());
    /// ```
    #[must_use]
    pub fn is_f64(&self) -> bool {
        matches!(self.repr, Repr::Double(_))
    }

    /// Returns whether the value is a string.
    ///
    /// ```
    /// assert!(syrp_var::Var::from("x").is_string());
    /// ```
    #[must_use]
    pub fn is_string(&self) -> bool {
        matches!(self.repr, Repr::Str(_))
    }

    /// Returns whether the value is an array.
    ///
    /// ```
    /// assert!(syrp_var::Var::from(Vec::new()).is_array());
    /// ```
    #[must_use]
    pub fn is_array(&self) -> bool {
        matches!(self.repr, Repr::Array(_))
    }

    /// Returns whether the value is an object.
    ///
    /// ```
    /// assert!(syrp_var::Var::new_object().is_object());
    /// ```
    #[must_use]
    pub fn is_object(&self) -> bool {
        matches!(self.repr, Repr::Object(_))
    }

    /// Returns whether the value is binary data.
    ///
    /// ```
    /// assert!(syrp_var::Var::from_bytes([0u8; 4]).is_binary());
    /// ```
    #[must_use]
    pub fn is_binary(&self) -> bool {
        matches!(self.repr, Repr::Binary(_))
    }

    /// Borrows the text of a string value; `None` for every other variant (no coercion).
    ///
    /// ```
    /// use syrp_var::Var;
    /// assert_eq!(Var::from("hi").as_str(), Some("hi"));
    /// assert_eq!(Var::from(1).as_str(), None);
    /// ```
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match &self.repr {
            Repr::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Borrows the items of an array value; `None` for every other variant.
    ///
    /// ```
    /// use syrp_var::Var;
    /// let v = Var::from(vec![Var::from(1), Var::from(2)]);
    /// assert_eq!(v.as_array().map(<[Var]>::len), Some(2));
    /// ```
    #[must_use]
    pub fn as_array(&self) -> Option<&[Var]> {
        match &self.repr {
            Repr::Array(items) => Some(items.as_slice()),
            _ => None,
        }
    }

    /// Borrows the shared object of an object value; `None` for every other variant.
    ///
    /// ```
    /// use syrp_var::Var;
    /// assert!(Var::new_object().as_object().is_some());
    /// assert!(Var::void().as_object().is_none());
    /// ```
    #[must_use]
    pub fn as_object(&self) -> Option<&Rc<DynamicObject>> {
        match &self.repr {
            Repr::Object(object) => Some(object),
            _ => None,
        }
    }

    /// Borrows the bytes of a binary value; `None` for every other variant.
    ///
    /// ```
    /// use syrp_var::Var;
    /// assert_eq!(Var::from_bytes(b"ab".to_vec()).as_bytes(), Some(&b"ab"[..]));
    /// ```
    #[must_use]
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match &self.repr {
            Repr::Binary(bytes) => Some(bytes.as_slice()),
            _ => None,
        }
    }
}

impl Default for Var {
    /// Returns void.
    fn default() -> Self {
        Self::void()
    }
}

impl From<bool> for Var {
    /// Wraps the value in the matching variant (see the `Var` example).
    fn from(b: bool) -> Self {
        Self {
            repr: Repr::Bool(b),
        }
    }
}

impl From<i32> for Var {
    /// Wraps the value in the matching variant (see the `Var` example).
    fn from(n: i32) -> Self {
        Self { repr: Repr::Int(n) }
    }
}

impl From<i64> for Var {
    /// Wraps the value in the matching variant (see the `Var` example).
    fn from(n: i64) -> Self {
        Self {
            repr: Repr::Int64(n),
        }
    }
}

impl From<f64> for Var {
    /// Wraps the value in the matching variant (see the `Var` example).
    fn from(x: f64) -> Self {
        Self {
            repr: Repr::Double(x),
        }
    }
}

impl From<&str> for Var {
    /// Wraps the value in the matching variant (see the `Var` example).
    fn from(s: &str) -> Self {
        Self {
            repr: Repr::Str(Rc::new(s.to_owned())),
        }
    }
}

impl From<String> for Var {
    /// Wraps the value in the matching variant (see the `Var` example).
    fn from(s: String) -> Self {
        Self {
            repr: Repr::Str(Rc::new(s)),
        }
    }
}

impl From<Vec<Var>> for Var {
    /// Wraps the value in the matching variant (see the `Var` example).
    fn from(items: Vec<Var>) -> Self {
        Self {
            repr: Repr::Array(Rc::new(items)),
        }
    }
}

impl From<Rc<DynamicObject>> for Var {
    /// Wraps the value in the matching variant (see the `Var` example).
    fn from(object: Rc<DynamicObject>) -> Self {
        Self {
            repr: Repr::Object(object),
        }
    }
}

impl fmt::Debug for Var {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.as_view() {
            VarView::Void => f.write_str("Void"),
            VarView::Undefined => f.write_str("Undefined"),
            VarView::Bool(b) => f.debug_tuple("Bool").field(&b).finish(),
            VarView::Int(n) => f.debug_tuple("Int").field(&n).finish(),
            VarView::Int64(n) => f.debug_tuple("Int64").field(&n).finish(),
            VarView::Double(x) => f.debug_tuple("Double").field(&x).finish(),
            VarView::Str(s) => f.debug_tuple("Str").field(&s).finish(),
            VarView::Array(items) => f.debug_tuple("Array").field(&items).finish(),
            VarView::Object(object) => f.debug_tuple("Object").field(object).finish(),
            VarView::Binary(bytes) => f.debug_tuple("Binary").field(&bytes).finish(),
        }
    }
}

impl fmt::Display for Var {
    /// Writes [`Var::to_text`].
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_text())
    }
}

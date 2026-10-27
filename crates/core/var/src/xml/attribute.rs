//! [`XmlAttribute`] and the attribute methods of [`XmlElement`].

use super::{XmlElement, check_name};
use crate::VarError;

/// One `name="value"` attribute of an [`XmlElement`]. The name is a valid XML name; the value
/// is any text (entities already decoded).
///
/// ```
/// use syrp_var::xml::XmlElement;
/// let mut e = XmlElement::new("a")?;
/// e.set_attribute("x", "1")?;
/// let attribute = e.attributes().next().unwrap();
/// assert_eq!((attribute.name(), attribute.value()), ("x", "1"));
/// # Ok::<(), syrp_var::VarError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XmlAttribute {
    name: String,
    value: String,
}

impl XmlAttribute {
    /// Makes an attribute whose name the caller has already checked.
    pub(crate) fn with_valid_name(name: String, value: String) -> Self {
        Self { name, value }
    }

    /// The attribute name.
    ///
    /// ```
    /// use syrp_var::xml::XmlElement;
    /// let mut e = XmlElement::new("a")?;
    /// e.set_attribute("gain", 1)?;
    /// assert_eq!(e.attributes().next().unwrap().name(), "gain");
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The attribute value as text.
    ///
    /// ```
    /// use syrp_var::xml::XmlElement;
    /// let mut e = XmlElement::new("a")?;
    /// e.set_attribute("gain", 1)?;
    /// assert_eq!(e.attributes().next().unwrap().value(), "1");
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }
}

/// A value [`XmlElement::set_attribute`] can store, converted to its text form.
///
/// Integers are written in decimal, `bool` as `"1"` or `"0"`, and `f64` in the shortest text
/// that reads back to the same value (Rust's `Display`, so `2.0` is `"2"` and `0.1` is
/// `"0.1"`). These match [`Var::to_text`](crate::Var::to_text). Infinities and NaN are written
/// as `inf`, `-inf` and `NaN` (Syrp's choice).
///
/// ```
/// use syrp_var::xml::IntoAttributeValue;
/// assert_eq!(0.1.into_attribute_value(), "0.1");
/// assert_eq!(true.into_attribute_value(), "1");
/// assert_eq!((-7_i64).into_attribute_value(), "-7");
/// ```
pub trait IntoAttributeValue {
    /// Converts to the attribute text.
    fn into_attribute_value(self) -> String;
}

impl IntoAttributeValue for &str {
    fn into_attribute_value(self) -> String {
        self.to_owned()
    }
}

impl IntoAttributeValue for String {
    fn into_attribute_value(self) -> String {
        self
    }
}

impl IntoAttributeValue for i32 {
    fn into_attribute_value(self) -> String {
        self.to_string()
    }
}

impl IntoAttributeValue for i64 {
    fn into_attribute_value(self) -> String {
        self.to_string()
    }
}

impl IntoAttributeValue for f64 {
    fn into_attribute_value(self) -> String {
        self.to_string()
    }
}

impl IntoAttributeValue for bool {
    fn into_attribute_value(self) -> String {
        if self { "1" } else { "0" }.to_owned()
    }
}

/// Attribute access. Names are case-sensitive and may be given as `&str`, `String` or
/// [`Identifier`](crate::Identifier).
impl XmlElement {
    /// Sets an attribute. Replacing one keeps its position; a new one is appended.
    ///
    /// # Errors
    /// `SYRP-VAR-0004` when `name` is not a valid XML name; nothing changes.
    ///
    /// ```
    /// use syrp_var::{Identifier, xml::XmlElement};
    /// let mut e = XmlElement::new("a")?;
    /// e.set_attribute("x", 1)?;
    /// e.set_attribute(Identifier::new("y"), 2.5)?;
    /// e.set_attribute("x", "one")?;
    /// let names: Vec<&str> = e.attributes().map(|a| a.name()).collect();
    /// assert_eq!(names, ["x", "y"]);
    /// assert_eq!(e.string_attribute("x"), Some("one"));
    /// assert!(e.set_attribute("bad name", 0).is_err());
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "setAttribute")]
    pub fn set_attribute(
        &mut self,
        name: impl AsRef<str>,
        value: impl IntoAttributeValue,
    ) -> Result<(), VarError> {
        let name = name.as_ref();
        check_name(name)?;
        let value = value.into_attribute_value();
        match self.attributes.iter_mut().find(|a| a.name == name) {
            Some(existing) => existing.value = value,
            None => self
                .attributes
                .push(XmlAttribute::with_valid_name(name.to_owned(), value)),
        }
        Ok(())
    }

    /// Returns whether the attribute exists.
    ///
    /// ```
    /// use syrp_var::xml::XmlElement;
    /// let mut e = XmlElement::new("a")?;
    /// e.set_attribute("x", "")?;
    /// assert!(e.has_attribute("x"));
    /// assert!(!e.has_attribute("X"));
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "hasAttribute")]
    #[must_use]
    pub fn has_attribute(&self, name: impl AsRef<str>) -> bool {
        self.string_attribute(name).is_some()
    }

    /// The attribute's text, or `None` when it is missing.
    ///
    /// ```
    /// use syrp_var::xml::XmlElement;
    /// let mut e = XmlElement::new("a")?;
    /// e.set_attribute("x", "v")?;
    /// assert_eq!(e.string_attribute("x"), Some("v"));
    /// assert_eq!(e.string_attribute("y"), None);
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "getStringAttribute")]
    #[must_use]
    pub fn string_attribute(&self, name: impl AsRef<str>) -> Option<&str> {
        let name = name.as_ref();
        self.attributes
            .iter()
            .find(|a| a.name == name)
            .map(|a| a.value.as_str())
    }

    /// Removes the attribute and returns whether it existed. The others keep their order.
    ///
    /// ```
    /// use syrp_var::xml::XmlElement;
    /// let mut e = XmlElement::new("a")?;
    /// e.set_attribute("x", 1)?;
    /// assert!(e.remove_attribute("x"));
    /// assert!(!e.remove_attribute("x"));
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "removeAttribute")]
    pub fn remove_attribute(&mut self, name: impl AsRef<str>) -> bool {
        let name = name.as_ref();
        let position = self.attributes.iter().position(|a| a.name == name);
        position.map(|i| self.attributes.remove(i)).is_some()
    }

    /// Removes every attribute.
    ///
    /// ```
    /// use syrp_var::xml::XmlElement;
    /// let mut e = XmlElement::new("a")?;
    /// e.set_attribute("x", 1)?;
    /// e.remove_all_attributes();
    /// assert_eq!(e.num_attributes(), 0);
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "removeAllAttributes")]
    pub fn remove_all_attributes(&mut self) {
        self.attributes.clear();
    }

    /// The number of attributes.
    ///
    /// ```
    /// use syrp_var::xml::XmlElement;
    /// let mut e = XmlElement::new("a")?;
    /// e.set_attribute("x", 1)?;
    /// assert_eq!(e.num_attributes(), 1);
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "getNumAttributes")]
    #[must_use]
    pub fn num_attributes(&self) -> usize {
        self.attributes.len()
    }

    /// The name of the attribute at `index`, or `None` past the end.
    ///
    /// ```
    /// use syrp_var::xml::XmlElement;
    /// let mut e = XmlElement::new("a")?;
    /// e.set_attribute("x", 1)?;
    /// assert_eq!(e.attribute_name(0), Some("x"));
    /// assert_eq!(e.attribute_name(1), None);
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "getAttributeName")]
    #[must_use]
    pub fn attribute_name(&self, index: usize) -> Option<&str> {
        self.attributes.get(index).map(XmlAttribute::name)
    }

    /// The value of the attribute at `index`, or `None` past the end.
    ///
    /// ```
    /// use syrp_var::xml::XmlElement;
    /// let mut e = XmlElement::new("a")?;
    /// e.set_attribute("x", 1)?;
    /// assert_eq!(e.attribute_value(0), Some("1"));
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "getAttributeValue")]
    #[must_use]
    pub fn attribute_value(&self, index: usize) -> Option<&str> {
        self.attributes.get(index).map(XmlAttribute::value)
    }

    /// The attributes in order.
    ///
    /// ```
    /// use syrp_var::xml::XmlElement;
    /// let mut e = XmlElement::new("a")?;
    /// e.set_attribute("x", 1)?;
    /// e.set_attribute("y", 2)?;
    /// let pairs: Vec<(&str, &str)> = e.attributes().map(|a| (a.name(), a.value())).collect();
    /// assert_eq!(pairs, [("x", "1"), ("y", "2")]);
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "getAttributeIterator")]
    pub fn attributes(&self) -> std::slice::Iter<'_, XmlAttribute> {
        self.attributes.iter()
    }

    /// Attribute comparison for [`XmlElement::is_equivalent_to`].
    pub(super) fn same_attributes(&self, other: &XmlElement, ignore_order: bool) -> bool {
        if self.attributes.len() != other.attributes.len() {
            return false;
        }
        if ignore_order {
            self.attributes
                .iter()
                .all(|a| other.string_attribute(&a.name) == Some(a.value.as_str()))
        } else {
            self.attributes == other.attributes
        }
    }
}

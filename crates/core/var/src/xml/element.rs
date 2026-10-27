//! [`XmlElement`]: the tree node, its tag and its children.

use std::fmt;

use super::attribute::XmlAttribute;
use super::check_name;
use super::node::{NodeRepr, XmlNode};
use crate::VarError;

/// An XML element: a tag name, an ordered list of attributes and an ordered list of children
/// ([`XmlNode`]s, each an element or text).
///
/// The tag name is always a valid XML name (see [`is_valid_xml_name`](super::is_valid_xml_name)).
/// Unlike JUCE's `XmlElement`, siblings are not linked: iterate a parent's
/// [`children`](XmlElement::children) or [`child_elements`](XmlElement::child_elements) instead
/// of following `getNextElement`.
///
/// `==` is [`is_equivalent_to`](XmlElement::is_equivalent_to) with attribute order significant.
///
/// ```
/// use syrp_var::xml::XmlElement;
///
/// let mut list = XmlElement::new("list")?;
/// for n in 0..3 {
///     list.create_new_child_element("item")?.set_attribute("n", n)?;
/// }
/// let ns: Vec<_> = list.child_elements().map(|e| e.string_attribute("n")).collect();
/// assert_eq!(ns, [Some("0"), Some("1"), Some("2")]);
/// # Ok::<(), syrp_var::VarError>(())
/// ```
pub struct XmlElement {
    pub(super) tag: String,
    pub(super) attributes: Vec<XmlAttribute>,
    pub(super) children: Vec<XmlNode>,
}

/// An iterator over the child elements of an [`XmlElement`], skipping text nodes. Made by
/// [`XmlElement::child_elements`].
///
/// ```
/// use syrp_var::xml::XmlElement;
/// let mut root = XmlElement::new("r")?;
/// root.add_text_element("t");
/// root.create_new_child_element("a")?;
/// assert_eq!(root.child_elements().count(), 1);
/// # Ok::<(), syrp_var::VarError>(())
/// ```
#[derive(Clone, Debug)]
pub struct ChildElements<'a> {
    inner: std::slice::Iter<'a, XmlNode>,
}

impl<'a> Iterator for ChildElements<'a> {
    type Item = &'a XmlElement;

    fn next(&mut self) -> Option<&'a XmlElement> {
        self.inner.by_ref().find_map(XmlNode::as_element)
    }
}

impl DoubleEndedIterator for ChildElements<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner.by_ref().rev().find_map(XmlNode::as_element)
    }
}

impl XmlElement {
    /// Makes an element with no attributes or children.
    ///
    /// # Errors
    /// `SYRP-VAR-0004` when `tag` is not a valid XML name.
    ///
    /// ```
    /// use syrp_var::{Diagnostic, xml::XmlElement};
    /// assert_eq!(XmlElement::new("preset")?.tag_name(), "preset");
    /// assert_eq!(XmlElement::new("1x").unwrap_err().code().as_str(), "SYRP-VAR-0004");
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    pub fn new(tag: &str) -> Result<Self, VarError> {
        check_name(tag)?;
        Ok(Self::with_valid_tag(tag.to_owned()))
    }

    /// Makes an element whose tag the caller has already checked.
    pub(super) fn with_valid_tag(tag: String) -> Self {
        Self {
            tag,
            attributes: Vec::new(),
            children: Vec::new(),
        }
    }

    /// The tag name, including any namespace prefix.
    ///
    /// ```
    /// use syrp_var::xml::XmlElement;
    /// assert_eq!(XmlElement::new("ns:a")?.tag_name(), "ns:a");
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "getTagName")]
    #[must_use]
    pub fn tag_name(&self) -> &str {
        &self.tag
    }

    /// Renames the element.
    ///
    /// # Errors
    /// `SYRP-VAR-0004` when `tag` is not a valid XML name; the old name is kept.
    ///
    /// ```
    /// use syrp_var::xml::XmlElement;
    /// let mut e = XmlElement::new("a")?;
    /// e.set_tag_name("b")?;
    /// assert!(e.set_tag_name("").is_err());
    /// assert_eq!(e.tag_name(), "b");
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "setTagName")]
    pub fn set_tag_name(&mut self, tag: &str) -> Result<(), VarError> {
        check_name(tag)?;
        tag.clone_into(&mut self.tag);
        Ok(())
    }

    /// All children (elements and text) in document order.
    ///
    /// ```
    /// use syrp_var::xml::XmlElement;
    /// let mut e = XmlElement::new("a")?;
    /// e.add_text_element("x");
    /// assert_eq!(e.children().len(), 1);
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[must_use]
    pub fn children(&self) -> &[XmlNode] {
        &self.children
    }

    /// The child elements in document order, skipping text nodes.
    ///
    /// ```
    /// use syrp_var::xml::XmlElement;
    /// let mut e = XmlElement::new("a")?;
    /// e.create_new_child_element("b")?;
    /// assert_eq!(e.child_elements().next().unwrap().tag_name(), "b");
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "getChildIterator")]
    #[must_use]
    pub fn child_elements(&self) -> ChildElements<'_> {
        ChildElements {
            inner: self.children.iter(),
        }
    }

    /// The number of children, text nodes included.
    ///
    /// ```
    /// use syrp_var::xml::XmlElement;
    /// let mut e = XmlElement::new("a")?;
    /// e.add_text_element("x");
    /// e.create_new_child_element("b")?;
    /// assert_eq!(e.num_children(), 2);
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "getNumChildElements")]
    #[must_use]
    pub fn num_children(&self) -> usize {
        self.children.len()
    }

    /// The child at `index`, or `None` past the end.
    ///
    /// ```
    /// use syrp_var::xml::XmlElement;
    /// let mut e = XmlElement::new("a")?;
    /// e.add_text_element("x");
    /// assert_eq!(e.child(0).and_then(|n| n.as_text()), Some("x"));
    /// assert!(e.child(1).is_none());
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "getChildElement")]
    #[must_use]
    pub fn child(&self, index: usize) -> Option<&XmlNode> {
        self.children.get(index)
    }

    /// The child at `index`, mutably.
    ///
    /// ```
    /// use syrp_var::xml::XmlElement;
    /// let mut e = XmlElement::new("a")?;
    /// e.add_text_element("x");
    /// e.child_mut(0).unwrap().set_text("y");
    /// assert_eq!(e.all_sub_text(), "y");
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[must_use]
    pub fn child_mut(&mut self, index: usize) -> Option<&mut XmlNode> {
        self.children.get_mut(index)
    }

    /// Appends a child (an element or a text node).
    ///
    /// ```
    /// use syrp_var::xml::{XmlElement, XmlNode};
    /// let mut e = XmlElement::new("a")?;
    /// e.add_child_element(XmlElement::new("b")?);
    /// e.add_child_element(XmlNode::text("t"));
    /// assert_eq!(e.num_children(), 2);
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "addChildElement")]
    pub fn add_child_element(&mut self, node: impl Into<XmlNode>) {
        self.children.push(node.into());
    }

    /// Appends a new, empty child element and returns it.
    ///
    /// # Errors
    /// `SYRP-VAR-0004` when `tag` is not a valid XML name; nothing is added.
    ///
    /// ```
    /// use syrp_var::xml::XmlElement;
    /// let mut e = XmlElement::new("a")?;
    /// e.create_new_child_element("b")?.set_attribute("x", 1)?;
    /// assert!(e.create_new_child_element("no spaces").is_err());
    /// assert_eq!(e.num_children(), 1);
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "createNewChildElement")]
    pub fn create_new_child_element(&mut self, tag: &str) -> Result<&mut XmlElement, VarError> {
        let child = XmlElement::new(tag)?;
        self.children.push(child.into());
        match self.children.last_mut().map(|n| &mut n.repr) {
            Some(NodeRepr::Element(element)) => Ok(element),
            _ => unreachable!("an element was just pushed"),
        }
    }

    /// Appends a text node.
    ///
    /// ```
    /// use syrp_var::xml::XmlElement;
    /// let mut e = XmlElement::new("a")?;
    /// e.add_text_element("hi");
    /// assert_eq!(e.all_sub_text(), "hi");
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "addTextElement")]
    pub fn add_text_element(&mut self, text: impl Into<String>) {
        self.children.push(XmlNode::text(text));
    }

    /// All text in this element's subtree, concatenated in document order.
    ///
    /// ```
    /// use syrp_var::xml::XmlElement;
    /// let mut e = XmlElement::new("p")?;
    /// e.add_text_element("a ");
    /// e.create_new_child_element("b")?.add_text_element("bold");
    /// e.add_text_element(" c");
    /// assert_eq!(e.all_sub_text(), "a bold c");
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "getAllSubText")]
    #[must_use]
    pub fn all_sub_text(&self) -> String {
        let mut text = String::new();
        let mut stack = vec![self.children.iter()];
        while let Some(iter) = stack.last_mut() {
            match iter.next().map(|n| &n.repr) {
                Some(NodeRepr::Text(t)) => text.push_str(t),
                Some(NodeRepr::Element(e)) => stack.push(e.children.iter()),
                None => {
                    stack.pop();
                }
            }
        }
        text
    }

    /// Returns whether both trees have the same tags, attributes and children in the same
    /// order, with text compared exactly. With `ignore_order_of_attributes`, attributes may be
    /// in any order (names and values must still match).
    ///
    /// ```
    /// use syrp_var::xml::XmlElement;
    /// let mut a = XmlElement::new("e")?;
    /// a.set_attribute("x", 1)?;
    /// a.set_attribute("y", 2)?;
    /// let mut b = XmlElement::new("e")?;
    /// b.set_attribute("y", 2)?;
    /// b.set_attribute("x", 1)?;
    /// assert!(!a.is_equivalent_to(&b, false));
    /// assert!(a.is_equivalent_to(&b, true));
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "isEquivalentTo")]
    #[must_use]
    pub fn is_equivalent_to(&self, other: &XmlElement, ignore_order_of_attributes: bool) -> bool {
        let mut stack = vec![(self, other)];
        while let Some((a, b)) = stack.pop() {
            if a.tag != b.tag
                || !a.same_attributes(b, ignore_order_of_attributes)
                || a.children.len() != b.children.len()
            {
                return false;
            }
            for (x, y) in a.children.iter().zip(&b.children) {
                match (&x.repr, &y.repr) {
                    (NodeRepr::Text(s), NodeRepr::Text(t)) if s == t => {}
                    (NodeRepr::Element(e), NodeRepr::Element(f)) => stack.push((e, f)),
                    _ => return false,
                }
            }
        }
        true
    }
}

impl Clone for XmlElement {
    /// A deep copy, made with an explicit stack (no recursion).
    fn clone(&self) -> Self {
        let shallow = |e: &XmlElement| XmlElement {
            tag: e.tag.clone(),
            attributes: e.attributes.clone(),
            children: Vec::with_capacity(e.children.len()),
        };
        // Each frame: the source element, the next child to copy, and the copy so far.
        let mut stack = vec![(self, 0_usize, shallow(self))];
        loop {
            let (source, next, copy) = stack.last_mut().expect("the root frame is popped last");
            if let Some(node) = source.children.get(*next) {
                *next += 1;
                match &node.repr {
                    NodeRepr::Text(text) => copy.children.push(XmlNode::text(text.clone())),
                    NodeRepr::Element(child) => stack.push((child, 0, shallow(child))),
                }
                continue;
            }
            let (_, _, done) = stack.pop().expect("checked above");
            match stack.last_mut() {
                Some((_, _, parent)) => parent.children.push(done.into()),
                None => return done,
            }
        }
    }
}

impl Drop for XmlElement {
    /// Drops the subtree with an explicit stack, so deep trees cannot overflow the stack.
    fn drop(&mut self) {
        let mut pending = std::mem::take(&mut self.children);
        while let Some(node) = pending.pop() {
            if let NodeRepr::Element(mut element) = node.repr {
                pending.append(&mut element.children);
            }
        }
    }
}

impl PartialEq for XmlElement {
    /// [`XmlElement::is_equivalent_to`] with attribute order significant.
    fn eq(&self, other: &Self) -> bool {
        self.is_equivalent_to(other, false)
    }
}

impl fmt::Debug for XmlElement {
    /// Shallow: the tag, the attributes and the number of children (never recursing).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("XmlElement")
            .field("tag", &self.tag)
            .field("attributes", &self.attributes)
            .field("children", &self.children.len())
            .finish()
    }
}

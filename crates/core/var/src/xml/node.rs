//! [`XmlNode`]: one child of an element, either an element or a text node.

use std::fmt;

use super::XmlElement;

/// A child of an [`XmlElement`]: a nested element or a run of text.
///
/// JUCE models text as a special "text element"; here it is a separate variant, made with
/// [`XmlNode::text`]. Match on [`XmlNode::as_view`] to tell the two apart.
///
/// ```
/// use syrp_var::xml::{XmlElement, XmlNode, XmlNodeView};
///
/// let text = XmlNode::text("hello");
/// let element = XmlNode::from(XmlElement::new("b")?);
/// assert!(text.is_text_element());
/// assert_eq!(text.as_text(), Some("hello"));
/// assert!(matches!(element.as_view(), XmlNodeView::Element(e) if e.tag_name() == "b"));
/// # Ok::<(), syrp_var::VarError>(())
/// ```
#[derive(Clone, PartialEq)]
pub struct XmlNode {
    pub(super) repr: NodeRepr,
}

/// The storage of an [`XmlNode`].
#[derive(Clone, PartialEq)]
pub(super) enum NodeRepr {
    Element(XmlElement),
    Text(String),
}

/// A borrowed, `Copy` view of an [`XmlNode`], for matching.
///
/// ```
/// use syrp_var::xml::{XmlNode, XmlNodeView};
/// let node = XmlNode::text("x");
/// match node.as_view() {
///     XmlNodeView::Text(text) => assert_eq!(text, "x"),
///     XmlNodeView::Element(_) => unreachable!(),
///     _ => unreachable!(),
/// }
/// ```
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum XmlNodeView<'a> {
    /// A nested element.
    Element(&'a XmlElement),
    /// A run of text, with entities already decoded.
    Text(&'a str),
}

impl XmlNode {
    /// Makes a text node holding `text` (any characters; the writer escapes them).
    ///
    /// ```
    /// use syrp_var::xml::XmlNode;
    /// assert_eq!(XmlNode::text("a < b").as_text(), Some("a < b"));
    /// ```
    #[doc(alias = "createTextElement")]
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            repr: NodeRepr::Text(text.into()),
        }
    }

    /// Lends a `Copy` view for matching.
    ///
    /// ```
    /// use syrp_var::xml::{XmlNode, XmlNodeView};
    /// assert!(matches!(XmlNode::text("t").as_view(), XmlNodeView::Text("t")));
    /// ```
    #[must_use]
    pub fn as_view(&self) -> XmlNodeView<'_> {
        match &self.repr {
            NodeRepr::Element(element) => XmlNodeView::Element(element),
            NodeRepr::Text(text) => XmlNodeView::Text(text),
        }
    }

    /// Returns whether this is a text node.
    ///
    /// ```
    /// use syrp_var::xml::{XmlElement, XmlNode};
    /// assert!(XmlNode::text("").is_text_element());
    /// assert!(!XmlNode::from(XmlElement::new("a")?).is_text_element());
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "isTextElement")]
    #[must_use]
    pub fn is_text_element(&self) -> bool {
        matches!(self.repr, NodeRepr::Text(_))
    }

    /// The text of a text node; `None` for an element.
    ///
    /// ```
    /// use syrp_var::xml::{XmlElement, XmlNode};
    /// assert_eq!(XmlNode::text("t").as_text(), Some("t"));
    /// assert_eq!(XmlNode::from(XmlElement::new("a")?).as_text(), None);
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "getText")]
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match &self.repr {
            NodeRepr::Text(text) => Some(text),
            NodeRepr::Element(_) => None,
        }
    }

    /// Replaces the text of a text node and returns `true`; an element is left unchanged and
    /// `false` is returned.
    ///
    /// ```
    /// use syrp_var::xml::XmlNode;
    /// let mut node = XmlNode::text("old");
    /// assert!(node.set_text("new"));
    /// assert_eq!(node.as_text(), Some("new"));
    /// ```
    #[doc(alias = "setText")]
    pub fn set_text(&mut self, text: impl Into<String>) -> bool {
        match &mut self.repr {
            NodeRepr::Text(old) => {
                *old = text.into();
                true
            }
            NodeRepr::Element(_) => false,
        }
    }

    /// The element, or `None` for a text node.
    ///
    /// ```
    /// use syrp_var::xml::{XmlElement, XmlNode};
    /// let node = XmlNode::from(XmlElement::new("a")?);
    /// assert_eq!(node.as_element().map(XmlElement::tag_name), Some("a"));
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[must_use]
    pub fn as_element(&self) -> Option<&XmlElement> {
        match &self.repr {
            NodeRepr::Element(element) => Some(element),
            NodeRepr::Text(_) => None,
        }
    }

    /// The element, mutably, or `None` for a text node.
    ///
    /// ```
    /// use syrp_var::xml::{XmlElement, XmlNode};
    /// let mut node = XmlNode::from(XmlElement::new("a")?);
    /// node.as_element_mut().unwrap().set_attribute("x", 1)?;
    /// assert_eq!(node.as_element().unwrap().string_attribute("x"), Some("1"));
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[must_use]
    pub fn as_element_mut(&mut self) -> Option<&mut XmlElement> {
        match &mut self.repr {
            NodeRepr::Element(element) => Some(element),
            NodeRepr::Text(_) => None,
        }
    }

    /// Takes the element out, or gives the node back when it is text.
    ///
    /// ```
    /// use syrp_var::xml::{XmlElement, XmlNode};
    /// let element = XmlNode::from(XmlElement::new("a")?).into_element().unwrap();
    /// assert_eq!(element.tag_name(), "a");
    /// assert!(XmlNode::text("t").into_element().is_err());
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    pub fn into_element(self) -> Result<XmlElement, XmlNode> {
        match self.repr {
            NodeRepr::Element(element) => Ok(element),
            repr @ NodeRepr::Text(_) => Err(Self { repr }),
        }
    }
}

impl From<XmlElement> for XmlNode {
    /// Wraps an element as a node.
    ///
    /// ```
    /// use syrp_var::xml::{XmlElement, XmlNode};
    /// assert!(!XmlNode::from(XmlElement::new("a")?).is_text_element());
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    fn from(element: XmlElement) -> Self {
        Self {
            repr: NodeRepr::Element(element),
        }
    }
}

impl fmt::Debug for XmlNode {
    /// Shallow: an element prints its tag, attributes and child count (never recursing).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.repr {
            NodeRepr::Element(element) => element.fmt(f),
            NodeRepr::Text(text) => f.debug_tuple("Text").field(text).finish(),
        }
    }
}

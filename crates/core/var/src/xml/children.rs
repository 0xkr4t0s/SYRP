//! The child-management, lookup and namespace methods of [`XmlElement`] (B3).

use std::cmp::Ordering;

use super::{XmlElement, XmlNode};

/// Children: insertion, removal, lookup, sorting and namespace prefixes.
impl XmlElement {
    /// Returns whether the tag name is exactly `name` (case-sensitive, prefix included).
    ///
    /// ```
    /// use syrp_var::xml::XmlElement;
    /// let e = XmlElement::new("ns:a")?;
    /// assert!(e.has_tag_name("ns:a"));
    /// assert!(!e.has_tag_name("a"));
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "hasTagName")]
    #[must_use]
    pub fn has_tag_name(&self, name: &str) -> bool {
        self.tag == name
    }

    /// Returns whether the tag name equals `name`, or its part after the namespace prefix
    /// ([`tag_name_without_namespace`](Self::tag_name_without_namespace)) does.
    ///
    /// ```
    /// use syrp_var::xml::XmlElement;
    /// let e = XmlElement::new("ns:a")?;
    /// assert!(e.has_tag_name_ignoring_namespace("a"));
    /// assert!(e.has_tag_name_ignoring_namespace("ns:a"));
    /// assert!(!e.has_tag_name_ignoring_namespace("ns"));
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "hasTagNameIgnoringNamespace")]
    #[must_use]
    pub fn has_tag_name_ignoring_namespace(&self, name: &str) -> bool {
        self.tag == name || self.tag_name_without_namespace() == name
    }

    /// The namespace prefix: the text before the first `:` of the tag, or `""` without one.
    ///
    /// ```
    /// use syrp_var::xml::XmlElement;
    /// assert_eq!(XmlElement::new("svg:rect")?.namespace(), "svg");
    /// assert_eq!(XmlElement::new("rect")?.namespace(), "");
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "getNamespace")]
    #[must_use]
    pub fn namespace(&self) -> &str {
        self.tag.split_once(':').map_or("", |(prefix, _)| prefix)
    }

    /// The tag after the first `:`, or the whole tag without one.
    ///
    /// ```
    /// use syrp_var::xml::XmlElement;
    /// assert_eq!(XmlElement::new("svg:rect")?.tag_name_without_namespace(), "rect");
    /// assert_eq!(XmlElement::new("rect")?.tag_name_without_namespace(), "rect");
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "getTagNameWithoutNamespace")]
    #[must_use]
    pub fn tag_name_without_namespace(&self) -> &str {
        self.tag
            .split_once(':')
            .map_or(&self.tag, |(_, local)| local)
    }

    /// The first child element whose tag is exactly `tag`.
    ///
    /// ```
    /// use syrp_var::xml::XmlElement;
    /// let mut e = XmlElement::new("a")?;
    /// e.create_new_child_element("b")?.set_attribute("n", 1)?;
    /// e.create_new_child_element("b")?.set_attribute("n", 2)?;
    /// assert_eq!(e.child_by_name("b").unwrap().int_attribute("n", 0), 1);
    /// assert!(e.child_by_name("c").is_none());
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "getChildByName")]
    #[must_use]
    pub fn child_by_name(&self, tag: &str) -> Option<&XmlElement> {
        self.child_elements().find(|e| e.tag == tag)
    }

    /// The first child element whose tag is exactly `tag`, mutably.
    ///
    /// ```
    /// use syrp_var::xml::XmlElement;
    /// let mut e = XmlElement::new("a")?;
    /// e.create_new_child_element("b")?;
    /// e.child_by_name_mut("b").unwrap().set_attribute("x", true)?;
    /// assert!(e.child_by_name("b").unwrap().bool_attribute("x", false));
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[must_use]
    pub fn child_by_name_mut(&mut self, tag: &str) -> Option<&mut XmlElement> {
        self.children
            .iter_mut()
            .filter_map(XmlNode::as_element_mut)
            .find(|e| e.tag == tag)
    }

    /// The first child element with an attribute `name` whose value is exactly `value`.
    ///
    /// ```
    /// use syrp_var::xml::XmlElement;
    /// let mut e = XmlElement::new("a")?;
    /// e.create_new_child_element("p")?.set_attribute("id", "gain")?;
    /// assert!(e.child_by_attribute("id", "gain").is_some());
    /// assert!(e.child_by_attribute("id", "Gain").is_none());
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "getChildByAttribute")]
    #[must_use]
    pub fn child_by_attribute(&self, name: impl AsRef<str>, value: &str) -> Option<&XmlElement> {
        let name = name.as_ref();
        self.child_elements()
            .find(|e| e.string_attribute(name) == Some(value))
    }

    /// Returns whether `child` is one of this element's direct children (the same object, not
    /// an equal copy).
    ///
    /// ```
    /// use syrp_var::xml::XmlElement;
    /// let mut e = XmlElement::new("a")?;
    /// e.create_new_child_element("b")?;
    /// let b = e.child_by_name("b").unwrap();
    /// assert!(e.contains_child(b));
    /// assert!(!e.contains_child(&XmlElement::new("b")?));
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "containsChildElement")]
    #[must_use]
    pub fn contains_child(&self, child: &XmlElement) -> bool {
        self.child_elements().any(|e| std::ptr::eq(e, child))
    }

    /// Inserts a child before all others.
    ///
    /// ```
    /// use syrp_var::xml::XmlElement;
    /// let mut e = XmlElement::new("a")?;
    /// e.add_child_element(XmlElement::new("b")?);
    /// e.prepend_child_element(XmlElement::new("c")?);
    /// assert_eq!(e.child_elements().next().unwrap().tag_name(), "c");
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "prependChildElement")]
    pub fn prepend_child_element(&mut self, node: impl Into<XmlNode>) {
        self.children.insert(0, node.into());
    }

    /// Inserts a child at `index`; an index past the end appends it.
    ///
    /// ```
    /// use syrp_var::xml::{XmlElement, XmlNode};
    /// let mut e = XmlElement::new("a")?;
    /// e.insert_child_element(XmlNode::text("end"), 99);
    /// e.insert_child_element(XmlNode::text("start"), 0);
    /// assert_eq!(e.all_sub_text(), "startend");
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "insertChildElement")]
    pub fn insert_child_element(&mut self, node: impl Into<XmlNode>, index: usize) {
        let index = index.min(self.children.len());
        self.children.insert(index, node.into());
    }

    /// Removes and returns the child at `index`, or `None` past the end.
    ///
    /// ```
    /// use syrp_var::xml::XmlElement;
    /// let mut e = XmlElement::new("a")?;
    /// e.add_text_element("x");
    /// assert_eq!(e.remove_child(0).unwrap().as_text(), Some("x"));
    /// assert!(e.remove_child(0).is_none());
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "removeChildElement")]
    pub fn remove_child(&mut self, index: usize) -> Option<XmlNode> {
        (index < self.children.len()).then(|| self.children.remove(index))
    }

    /// Replaces the child at `index` and returns the old one; past the end, `node` is given
    /// back as the error and nothing changes.
    ///
    /// ```
    /// use syrp_var::xml::{XmlElement, XmlNode};
    /// let mut e = XmlElement::new("a")?;
    /// e.add_text_element("old");
    /// let old = e.replace_child_element(0, XmlNode::text("new")).unwrap();
    /// assert_eq!(old.as_text(), Some("old"));
    /// assert!(e.replace_child_element(5, XmlNode::text("x")).is_err());
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "replaceChildElement")]
    pub fn replace_child_element(
        &mut self,
        index: usize,
        node: impl Into<XmlNode>,
    ) -> Result<XmlNode, XmlNode> {
        match self.children.get_mut(index) {
            Some(slot) => Ok(std::mem::replace(slot, node.into())),
            None => Err(node.into()),
        }
    }

    /// Removes every child, text nodes included.
    ///
    /// ```
    /// use syrp_var::xml::XmlElement;
    /// let mut e = XmlElement::new("a")?;
    /// e.add_text_element("x");
    /// e.create_new_child_element("b")?;
    /// e.delete_all_child_elements();
    /// assert_eq!(e.num_children(), 0);
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "deleteAllChildElements")]
    pub fn delete_all_child_elements(&mut self) {
        drop(std::mem::take(&mut self.children));
    }

    /// Removes every child element whose tag is exactly `tag`; text nodes stay.
    ///
    /// ```
    /// use syrp_var::xml::XmlElement;
    /// let mut e = XmlElement::new("a")?;
    /// e.create_new_child_element("b")?;
    /// e.create_new_child_element("c")?;
    /// e.create_new_child_element("b")?;
    /// e.delete_all_child_elements_with_tag_name("b");
    /// assert_eq!(e.num_children(), 1);
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "deleteAllChildElementsWithTagName")]
    pub fn delete_all_child_elements_with_tag_name(&mut self, tag: &str) {
        self.children
            .retain(|node| node.as_element().is_none_or(|e| e.tag != tag));
    }

    /// Sorts the children with `compare`, keeping equal children in their order (stable).
    ///
    /// ```
    /// use syrp_var::xml::XmlElement;
    /// let mut e = XmlElement::new("a")?;
    /// for n in [3, 1, 2] {
    ///     e.create_new_child_element("i")?.set_attribute("n", n)?;
    /// }
    /// let key = |node: &syrp_var::xml::XmlNode| node.as_element().map(|e| e.int_attribute("n", 0));
    /// e.sort_child_elements(|a, b| key(a).cmp(&key(b)));
    /// let ns: Vec<i32> = e.child_elements().map(|c| c.int_attribute("n", 0)).collect();
    /// assert_eq!(ns, [1, 2, 3]);
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "sortChildElements")]
    pub fn sort_child_elements(&mut self, compare: impl FnMut(&XmlNode, &XmlNode) -> Ordering) {
        self.children.sort_by(compare);
    }

    /// The [`all_sub_text`](Self::all_sub_text) of the first child element tagged `tag`, or
    /// `default` when there is none.
    ///
    /// ```
    /// use syrp_var::xml::XmlElement;
    /// let mut e = XmlElement::new("a")?;
    /// e.create_new_child_element("name")?.add_text_element("Pad");
    /// assert_eq!(e.child_element_all_sub_text("name", "?"), "Pad");
    /// assert_eq!(e.child_element_all_sub_text("author", "?"), "?");
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "getChildElementAllSubText")]
    #[must_use]
    pub fn child_element_all_sub_text(&self, tag: &str, default: &str) -> String {
        self.child_by_name(tag)
            .map_or_else(|| default.to_owned(), XmlElement::all_sub_text)
    }
}

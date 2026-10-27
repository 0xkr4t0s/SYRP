---
juce: juce::XmlElement, juce::XmlDocument, juce::parseXML, juce::parseXMLIfTagMatches, juce::XmlElement::TextFormat
syrp: syrp_var::xml
status: done
---

# XML: `XmlElement` and `XmlDocument`

Syrp's XML lives in `syrp_var::xml` (also re-exported from the `syrp_var` root). The tree owns
its nodes directly: there are no pointers to manage, and `XmlElement` is `Send + Sync`.

| JUCE | Syrp |
|---|---|
| `XmlElement (tag)` | `XmlElement::new(tag)?` (an invalid name is `SYRP-VAR-0004`) |
| `XmlElement::createTextElement (text)`, `isTextElement`, `getText`, `setText` | `XmlNode::text(text)`, `XmlNode::is_text_element`, `XmlNode::as_text`, `XmlNode::set_text` |
| `getTagName`, `setTagName`, `hasTagName`, `hasTagNameIgnoringNamespace` | `tag_name`, `set_tag_name`, `has_tag_name`, `has_tag_name_ignoring_namespace` |
| `getNamespace`, `getTagNameWithoutNamespace` | `namespace`, `tag_name_without_namespace` |
| `setAttribute (name, value)` | `set_attribute(name, value)?` for `&str`, `String`, `i32`, `i64`, `f64`, `bool` |
| `getStringAttribute (name)`, `(name, default)` | `string_attribute(name) -> Option<&str>`, `string_attribute_or(name, default)` |
| `getIntAttribute`, `getDoubleAttribute`, `getBoolAttribute` | `int_attribute`, `double_attribute`, `bool_attribute` (each with a default) |
| `hasAttribute`, `compareAttribute`, `removeAttribute`, `removeAllAttributes` | same names in snake case |
| `getNumAttributes`, `getAttributeName (i)`, `getAttributeValue (i)`, `getAttributeIterator` | `num_attributes`, `attribute_name(i)`, `attribute_value(i)`, `attributes()` (`XmlAttribute` items) |
| `addChildElement`, `prependChildElement`, `insertChildElement` | `add_child_element`, `prepend_child_element`, `insert_child_element(node, index)` |
| `createNewChildElement (tag)` | `create_new_child_element(tag)? -> &mut XmlElement` |
| `addTextElement` | `add_text_element` |
| `removeChildElement (child, delete)` | `remove_child(index) -> Option<XmlNode>` (you get the node back) |
| `replaceChildElement (current, new)` | `replace_child_element(index, node) -> Result<old, node>` |
| `deleteAllChildElements`, `deleteAllChildElementsWithTagName` | `delete_all_child_elements`, `delete_all_child_elements_with_tag_name` |
| `getNumChildElements`, `getChildElement (i)` | `num_children`, `child(i)` (text nodes count, as in JUCE) |
| `getChildByName`, `getChildByAttribute`, `containsChildElement` | `child_by_name`, `child_by_attribute`, `contains_child` |
| `getFirstChildElement` / `getNextElement`, `getChildIterator`, `getChildWithTagNameIterator` | iterate `children()` (all nodes) or `child_elements()` (elements only); filter by `has_tag_name` |
| `getAllSubText`, `getChildElementAllSubText` | `all_sub_text`, `child_element_all_sub_text` |
| `sortChildElements (comparator)` | `sort_child_elements(|a, b| ...)` (stable, takes an `Ordering` closure) |
| `isEquivalentTo (other, ignoreOrderOfAttributes)` | `is_equivalent_to(other, ignore_order_of_attributes)`; `==` keeps attribute order |
| `toString (format)`, `writeTo (stream, format)` | `to_string(&format)`, `write_to(&mut impl io::Write, &format)` |
| `XmlElement::TextFormat` fields | `TextFormat::new()` with `with_dtd`, `with_custom_header`, `with_default_header`, `with_line_wrap`, `with_new_line_chars`, `with_indentation`; getters by field name |
| `TextFormat::singleLine()`, `withoutHeader()` | `single_line()`, `without_header()` |
| `XmlDocument (text)`, `getDocumentElement` | `XmlDocument::new(text).document_element()?` |
| `setEmptyTextElementsIgnored`, `getDocumentElement (onlyReadOuterDocumentElement)` | `ignore_empty_text_elements(bool)`, `only_read_outer_element(bool)` |
| `getLastParseError` | the `Err(VarError)` itself, with `code()`, `line()` and `column()` |
| `parseXML (text)` | `XmlDocument::parse(text)?`; for bytes, `XmlDocument::parse_bytes(bytes)?` |
| `parseXMLIfTagMatches (text, tag)` | `XmlDocument::parse(text).ok().filter(|e| e.has_tag_name(tag))` |

## Differences to know

- Siblings are not linked. JUCE's `getNextElement` walk becomes iteration over the parent's
  `children()` or `child_elements()`.
- Text nodes are a separate `XmlNode` variant rather than elements with a special tag.
- Errors are values: parse errors carry `SYRP-VAR-0005` (malformed, with line and column),
  `-0006` (encoding other than UTF-8/UTF-16) or `-0007` (no root or more than one root).
- Files, URLs and streams are not read here (`parseXML (File)`, `readEntireXmlStream`,
  `writeTo (File)`): read the text with `syrp-io` first, then parse it.
- A `DOCTYPE` is skipped and its entities are never expanded; external entities are never
  fetched.
- Layout details JUCE's documentation does not fix (default header text, line ends, wrapping,
  `'` and `>` escaping, text-only elements on one line) are Syrp's choices, listed in the
  `syrp-var` `AGENTS.md`.

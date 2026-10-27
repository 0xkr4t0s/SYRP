//! An XML document model: [`XmlElement`] trees whose children are [`XmlNode`]s (elements or
//! text), with ordered [`XmlAttribute`]s.
//!
//! The behaviour follows JUCE's public documentation for `XmlElement`; where it leaves a case
//! open, the choice made here is stated in the item's documentation and listed in the crate's
//! `AGENTS.md` for the differential harness.
//!
//! ```
//! use syrp_var::xml::{XmlElement, XmlNode};
//!
//! let mut root = XmlElement::new("preset")?;
//! root.set_attribute("name", "Warm pad")?;
//! let gain = root.create_new_child_element("param")?;
//! gain.set_attribute("id", "gain")?;
//! gain.set_attribute("value", 0.5)?;
//! root.add_text_element("notes");
//!
//! assert_eq!(root.num_children(), 2);
//! let param = root.child_elements().find(|e| e.string_attribute("id") == Some("gain"));
//! assert_eq!(param.and_then(|p| p.string_attribute("value")), Some("0.5"));
//! assert!(root.child(1).is_some_and(XmlNode::is_text_element));
//! # Ok::<(), syrp_var::VarError>(())
//! ```
//!
//! # Depth
//! Cloning, dropping, comparing ([`XmlElement::is_equivalent_to`]) and collecting text
//! ([`XmlElement::all_sub_text`]) walk the tree with an explicit stack, never by recursion, so
//! a tree nested 100,000 levels deep works without a depth limit.
//!
//! # Threads
//! [`XmlElement`] and [`XmlNode`] own plain `String`s and `Vec`s (no `Rc`), so they are `Send`
//! and `Sync`: a document can be built or parsed on a worker thread and moved to another.

mod attribute;
mod element;
mod node;

pub use attribute::{IntoAttributeValue, XmlAttribute};
pub use element::{ChildElements, XmlElement};
pub use node::{XmlNode, XmlNodeView};

/// Returns whether `name` matches the XML 1.0 (fifth edition) `Name` production, the rule for
/// tag and attribute names.
///
/// The first character must be a name-start character: `:`, `_`, `A`-`Z`, `a`-`z`, or a
/// character in U+00C0-U+00D6, U+00D8-U+00F6, U+00F8-U+02FF, U+0370-U+037D, U+037F-U+1FFF,
/// U+200C-U+200D, U+2070-U+218F, U+2C00-U+2FEF, U+3001-U+D7FF, U+F900-U+FDCF, U+FDF0-U+FFFD or
/// U+10000-U+EFFFF. Each later character is a name-start character, `-`, `.`, `0`-`9`, U+00B7,
/// or one in U+0300-U+036F or U+203F-U+2040. The empty string is not a name.
///
/// ```
/// use syrp_var::xml::is_valid_xml_name;
/// assert!(is_valid_xml_name("param"));
/// assert!(is_valid_xml_name("ns:tag-2.b_c"));
/// assert!(is_valid_xml_name("größe"));
/// assert!(!is_valid_xml_name(""));
/// assert!(!is_valid_xml_name("2nd"));
/// assert!(!is_valid_xml_name("a b"));
/// ```
#[must_use]
pub fn is_valid_xml_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(is_name_start) && chars.all(is_name_char)
}

/// The XML `NameStartChar` production.
pub(crate) fn is_name_start(c: char) -> bool {
    matches!(c,
        ':' | '_' | 'A'..='Z' | 'a'..='z'
        | '\u{C0}'..='\u{D6}' | '\u{D8}'..='\u{F6}' | '\u{F8}'..='\u{2FF}'
        | '\u{370}'..='\u{37D}' | '\u{37F}'..='\u{1FFF}' | '\u{200C}'..='\u{200D}'
        | '\u{2070}'..='\u{218F}' | '\u{2C00}'..='\u{2FEF}' | '\u{3001}'..='\u{D7FF}'
        | '\u{F900}'..='\u{FDCF}' | '\u{FDF0}'..='\u{FFFD}' | '\u{10000}'..='\u{EFFFF}')
}

/// The XML `NameChar` production.
pub(crate) fn is_name_char(c: char) -> bool {
    is_name_start(c)
        || matches!(c,
            '-' | '.' | '0'..='9' | '\u{B7}' | '\u{300}'..='\u{36F}' | '\u{203F}'..='\u{2040}')
}

/// Returns `Ok(())` when `name` is a valid XML name, else the `SYRP-VAR-0004` error.
pub(crate) fn check_name(name: &str) -> Result<(), crate::VarError> {
    if is_valid_xml_name(name) {
        Ok(())
    } else {
        Err(crate::VarError::invalid_xml_name(name))
    }
}

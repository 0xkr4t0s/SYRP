//! [`XmlDocument`]: the XML parser. It is iterative (an explicit stack of open elements), so
//! nesting depth is limited only by memory, and every allocation is bounded by the input size.

use std::borrow::Cow;

use super::attribute::XmlAttribute;
use super::{XmlElement, XmlNode, is_name_char, is_name_start};
use crate::VarError;
use crate::error::{MALFORMED_XML, UNSUPPORTED_ENCODING, XML_ROOT_COUNT, XML_TOO_LARGE};

/// Parses XML text into an [`XmlElement`] tree.
///
/// Accepted: an XML declaration, comments and processing instructions (all dropped), CDATA
/// sections (kept as text), the five predefined entities (`&amp;` `&lt;` `&gt;` `&quot;`
/// `&apos;`) and decimal or hexadecimal character references. A `DOCTYPE` is skipped: entities
/// it declares are never expanded (using one is an undefined-entity error) and nothing outside
/// the text is ever read, so external-entity and entity-expansion attacks do not apply. Line
/// ends are normalised to `\n`, and tabs and line ends written literally inside attribute
/// values become spaces, as XML 1.0 requires. Adjacent text, CDATA and references merge into
/// one text node, also across a comment.
///
/// Errors carry a code and the 1-based line and column: `SYRP-VAR-0005` for text that is not
/// well-formed (mismatched or unclosed tags, an undefined entity, a duplicate attribute, an
/// invalid name...), `SYRP-VAR-0007` for no root element or more than one, `SYRP-VAR-0006` for
/// an encoding other than UTF-8 or UTF-16 ([`XmlDocument::parse_bytes`]).
///
/// Syrp's choices where JUCE's documentation is silent: character references may name any
/// Unicode scalar value, `&#0;` and control characters included, so any `String` round-trips;
/// literal control characters are accepted; the `encoding` of a declaration is ignored when the
/// input is already text ([`XmlDocument::parse`]).
///
/// ```
/// use syrp_var::xml::XmlDocument;
///
/// let root = XmlDocument::parse(r#"<?xml version="1.0"?>
/// <preset name="Pad &amp; Strings">
///   <param id="gain" value="0.5"/>
///   <!-- a comment -->
///   <notes><![CDATA[a < b]]></notes>
/// </preset>"#)?;
/// assert_eq!(root.string_attribute("name"), Some("Pad & Strings"));
/// assert_eq!(root.num_children(), 2); // whitespace-only text is dropped
/// assert_eq!(root.child_element_all_sub_text("notes", ""), "a < b");
///
/// let keep = XmlDocument::new("<a> <b/> </a>").ignore_empty_text_elements(false);
/// assert_eq!(keep.document_element()?.num_children(), 3);
/// # Ok::<(), syrp_var::VarError>(())
/// ```
#[derive(Clone, Debug)]
pub struct XmlDocument<'a> {
    input: Input<'a>,
    ignore_empty_text: bool,
    only_outer: bool,
}

/// What an [`XmlDocument`] reads.
#[derive(Clone, Copy, Debug)]
enum Input<'a> {
    Text(&'a str),
    Bytes(&'a [u8]),
}

impl<'a> XmlDocument<'a> {
    /// A parser for `text` with the default options (empty text dropped, whole document read).
    ///
    /// ```
    /// use syrp_var::xml::XmlDocument;
    /// let root = XmlDocument::new("<a/>").document_element()?;
    /// assert_eq!(root.tag_name(), "a");
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[must_use]
    pub fn new(text: &'a str) -> Self {
        Self::with_input(Input::Text(text))
    }

    /// A parser for encoded bytes; see [`XmlDocument::parse_bytes`] for the detection rules.
    ///
    /// ```
    /// use syrp_var::xml::XmlDocument;
    /// let root = XmlDocument::from_bytes(b"<a/>").document_element()?;
    /// assert_eq!(root.tag_name(), "a");
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[must_use]
    pub fn from_bytes(bytes: &'a [u8]) -> Self {
        Self::with_input(Input::Bytes(bytes))
    }

    fn with_input(input: Input<'a>) -> Self {
        Self {
            input,
            ignore_empty_text: true,
            only_outer: false,
        }
    }

    /// Whether text nodes holding only whitespace (space, tab, CR, LF) are dropped. On by
    /// default.
    ///
    /// ```
    /// use syrp_var::xml::XmlDocument;
    /// let root = XmlDocument::new("<a> </a>").ignore_empty_text_elements(false).document_element()?;
    /// assert_eq!(root.all_sub_text(), " ");
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "setEmptyTextElementsIgnored")]
    #[must_use]
    pub fn ignore_empty_text_elements(mut self, ignore: bool) -> Self {
        self.ignore_empty_text = ignore;
        self
    }

    /// Whether to read only the root element's tag and attributes, stopping after its start
    /// tag (nothing after it is read or checked). Off by default.
    ///
    /// ```
    /// use syrp_var::xml::XmlDocument;
    /// let root = XmlDocument::new("<a v='1'><b/>").only_read_outer_element(true).document_element()?;
    /// assert_eq!((root.int_attribute("v", 0), root.num_children()), (1, 0));
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[must_use]
    pub fn only_read_outer_element(mut self, only_outer: bool) -> Self {
        self.only_outer = only_outer;
        self
    }

    /// Parses the input and returns its root element.
    ///
    /// # Errors
    /// See the type documentation for the codes.
    ///
    /// ```
    /// use syrp_var::{Diagnostic, xml::XmlDocument};
    /// let error = XmlDocument::new("<a>\n  <b></a>").document_element().unwrap_err();
    /// assert_eq!((error.code().as_str(), error.line(), error.column()), ("SYRP-VAR-0005", 2, 6));
    /// ```
    #[doc(alias = "getDocumentElement")]
    pub fn document_element(&self) -> Result<XmlElement, VarError> {
        let text = match self.input {
            Input::Text(text) => Cow::Borrowed(text),
            Input::Bytes(bytes) => decode(bytes)?,
        };
        let text = text.strip_prefix('\u{FEFF}').unwrap_or(&text);
        Parser {
            text,
            pos: 0,
            ignore_empty_text: self.ignore_empty_text,
            only_outer: self.only_outer,
        }
        .document()
    }

    /// Parses `text` with the default options.
    ///
    /// # Errors
    /// See the type documentation for the codes.
    ///
    /// ```
    /// use syrp_var::{Diagnostic, xml::XmlDocument};
    /// assert_eq!(XmlDocument::parse("<a>&#x263A;</a>")?.all_sub_text(), "☺");
    /// let error = XmlDocument::parse("<a/><b/>").unwrap_err();
    /// assert_eq!(error.code().as_str(), "SYRP-VAR-0007");
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "parseXML")]
    pub fn parse(text: &str) -> Result<XmlElement, VarError> {
        XmlDocument::new(text).document_element()
    }

    /// Parses encoded bytes with the default options.
    ///
    /// A byte-order mark selects UTF-8 (`EF BB BF`), UTF-16BE (`FE FF`) or UTF-16LE (`FF FE`);
    /// without one, input starting `00 3C` or `3C 00` is read as UTF-16BE or LE, and anything
    /// else as UTF-8. An `encoding` in the XML declaration must then name UTF-8, UTF-16,
    /// UTF-16LE or UTF-16BE (any case), else the result is `SYRP-VAR-0006`; the bytes decide
    /// which of them is used (Syrp's choice). Invalid UTF-8 or UTF-16 is `SYRP-VAR-0005`.
    ///
    /// # Errors
    /// See the type documentation for the codes.
    ///
    /// ```
    /// use syrp_var::{Diagnostic, xml::XmlDocument};
    /// let utf16: Vec<u8> = [0xFF, 0xFE].into_iter()
    ///     .chain("<a>é</a>".encode_utf16().flat_map(u16::to_le_bytes)).collect();
    /// assert_eq!(XmlDocument::parse_bytes(&utf16)?.all_sub_text(), "é");
    /// let latin1 = b"<?xml version='1.0' encoding='ISO-8859-1'?><a/>";
    /// assert_eq!(XmlDocument::parse_bytes(latin1).unwrap_err().code().as_str(), "SYRP-VAR-0006");
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    pub fn parse_bytes(bytes: &[u8]) -> Result<XmlElement, VarError> {
        XmlDocument::from_bytes(bytes).document_element()
    }
}

/// The 1-based line and column (in characters) of byte offset `at` in `text`.
fn position(text: &str, at: usize) -> (usize, usize) {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    let before = &text[..at];
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    (
        before.matches('\n').count() + 1,
        before[line_start..].chars().count() + 1,
    )
}

/// Decodes bytes to text by byte-order mark or first bytes, then checks the declared encoding.
fn decode(bytes: &[u8]) -> Result<Cow<'_, str>, VarError> {
    let utf16 = match bytes {
        [0xEF, 0xBB, 0xBF, rest @ ..] => return check_encoding(utf8(rest)?),
        [0xFE, 0xFF, rest @ ..] => Some((rest, true)),
        [0xFF, 0xFE, rest @ ..] => Some((rest, false)),
        [0x00, 0x3C, ..] => Some((bytes, true)),
        [0x3C, 0x00, ..] => Some((bytes, false)),
        _ => None,
    };
    let Some((rest, big_endian)) = utf16 else {
        return check_encoding(utf8(bytes)?);
    };
    let too_large = |_| {
        VarError::xml(
            XML_TOO_LARGE,
            "cannot reserve memory to decode the input".into(),
            0,
            0,
        )
    };
    let pairs = rest.chunks_exact(2);
    let odd = !pairs.remainder().is_empty();
    let units = pairs.map(|pair| {
        let pair = [pair[0], pair[1]];
        if big_endian {
            u16::from_be_bytes(pair)
        } else {
            u16::from_le_bytes(pair)
        }
    });
    let mut text = String::new();
    // Most units decode to one UTF-8 byte; longer characters grow the string as needed.
    text.try_reserve_exact(rest.len() / 2).map_err(too_large)?;
    for c in char::decode_utf16(units) {
        match c {
            Ok(c) => text.push(c),
            Err(_) => return Err(bad_encoding(&text, "unpaired UTF-16 surrogate")),
        }
    }
    if odd {
        return Err(bad_encoding(
            &text,
            "UTF-16 input has an odd number of bytes",
        ));
    }
    check_encoding(Cow::Owned(text))
}

/// Borrows valid UTF-8, or reports where it stops being valid.
fn utf8(bytes: &[u8]) -> Result<Cow<'_, str>, VarError> {
    std::str::from_utf8(bytes).map(Cow::Borrowed).map_err(|e| {
        let valid = std::str::from_utf8(&bytes[..e.valid_up_to()]).unwrap_or_default();
        bad_encoding(valid, "invalid UTF-8")
    })
}

/// A `SYRP-VAR-0005` error at the end of the text decoded so far.
fn bad_encoding(decoded: &str, message: &str) -> VarError {
    let (line, column) = position(decoded, decoded.len());
    VarError::xml(MALFORMED_XML, message.to_owned(), line, column)
}

/// Rejects a declared encoding other than UTF-8 or UTF-16.
fn check_encoding(text: Cow<'_, str>) -> Result<Cow<'_, str>, VarError> {
    let body = text.strip_prefix('\u{FEFF}').unwrap_or(&text);
    let Some(declaration) = body
        .strip_prefix("<?xml")
        .filter(|rest| rest.starts_with(|c: char| c.is_ascii_whitespace()))
        .and_then(|rest| rest.split_once("?>"))
        .map(|(declaration, _)| declaration)
    else {
        return Ok(text);
    };
    let Some(after) = declaration.split_once("encoding").map(|(_, after)| after) else {
        return Ok(text);
    };
    let value = after
        .trim_start()
        .strip_prefix('=')
        .map(str::trim_start)
        .and_then(|v| {
            let quote = v.chars().next().filter(|q| matches!(q, '"' | '\''))?;
            v[1..].split_once(quote).map(|(value, _)| value)
        })
        .unwrap_or("");
    let known = ["UTF-8", "UTF-16", "UTF-16LE", "UTF-16BE"];
    if known.iter().any(|k| k.eq_ignore_ascii_case(value)) {
        Ok(text)
    } else {
        Err(VarError::xml(
            UNSUPPORTED_ENCODING,
            format!("unsupported encoding {value:?}: only UTF-8 and UTF-16 are read"),
            1,
            1,
        ))
    }
}

/// Appends `raw` with each CR LF pair and each lone CR turned into LF (XML 1.0 section 2.11).
fn push_line_ends_normalised(out: &mut String, raw: &str) {
    let mut lines = raw.split('\r');
    out.push_str(lines.next().unwrap_or(""));
    for line in lines {
        out.push('\n');
        out.push_str(line.strip_prefix('\n').unwrap_or(line));
    }
}

/// Returns whether `c` is XML white space.
fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r')
}

/// The parser state over decoded text.
struct Parser<'t> {
    text: &'t str,
    pos: usize,
    ignore_empty_text: bool,
    only_outer: bool,
}

impl<'t> Parser<'t> {
    fn rest(&self) -> &'t str {
        &self.text[self.pos..]
    }

    fn error_at(&self, at: usize, code: syrp_error::ErrorCode, message: String) -> VarError {
        let (line, column) = position(self.text, at);
        VarError::xml(code, message, line, column)
    }

    fn malformed(&self, at: usize, message: impl Into<String>) -> VarError {
        self.error_at(at, MALFORMED_XML, message.into())
    }

    fn skip_space(&mut self) -> bool {
        let skipped = self.rest().len() - self.rest().trim_start_matches(is_space).len();
        self.pos += skipped;
        skipped > 0
    }

    /// Skips past `end`, starting at `self.pos + from`; `what` names the construct.
    fn skip_past(&mut self, from: usize, end: &str, what: &str) -> Result<&'t str, VarError> {
        let start = self.pos;
        match self.text[start + from..].find(end) {
            Some(i) => {
                self.pos = start + from + i + end.len();
                Ok(&self.text[start + from..start + from + i])
            }
            None => Err(self.malformed(start, format!("unterminated {what}"))),
        }
    }

    fn name(&mut self) -> Result<&'t str, VarError> {
        let rest = self.rest();
        let mut chars = rest.char_indices();
        if !chars.next().is_some_and(|(_, c)| is_name_start(c)) {
            return Err(self.malformed(self.pos, "expected an XML name"));
        }
        let end = chars
            .find(|&(_, c)| !is_name_char(c))
            .map_or(rest.len(), |(i, _)| i);
        self.pos += end;
        Ok(&rest[..end])
    }

    /// Reads a reference after `&` and appends its character(s) to `out`.
    fn reference(&mut self, out: &mut String) -> Result<(), VarError> {
        let start = self.pos;
        let body = self.rest()[1..].split(';').next().unwrap_or("");
        let undefined = || self.malformed(start, "undefined entity or bad character reference");
        if self.rest()[1..].len() == body.len() {
            return Err(undefined());
        }
        let value = match body {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            _ => {
                let number = match body.strip_prefix('#') {
                    Some(hex) if hex.starts_with('x') => u32::from_str_radix(&hex[1..], 16).ok(),
                    Some(dec) => dec.parse::<u32>().ok(),
                    None => None,
                };
                let digits = body.trim_start_matches(['#', 'x']);
                if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err(undefined());
                }
                number.and_then(char::from_u32).ok_or_else(undefined)?
            }
        };
        out.push(value);
        self.pos += body.len() + 2;
        Ok(())
    }

    /// Reads a quoted attribute value after the opening quote position.
    fn attribute_value(&mut self) -> Result<String, VarError> {
        let start = self.pos;
        let quote = match self.rest().chars().next() {
            Some(q @ ('"' | '\'')) => q,
            _ => return Err(self.malformed(start, "expected a quoted attribute value")),
        };
        self.pos += 1;
        let mut value = String::new();
        loop {
            let rest = self.rest();
            let run = rest.find(['&', '<', '\r', '\n', '\t', quote]);
            let Some(run) = run else {
                return Err(self.malformed(start, "unterminated attribute value"));
            };
            value.push_str(&rest[..run]);
            self.pos += run;
            match self.rest().chars().next() {
                Some('&') => self.reference(&mut value)?,
                Some('<') => return Err(self.malformed(self.pos, "`<` in an attribute value")),
                Some(c) if c == quote => {
                    self.pos += 1;
                    return Ok(value);
                }
                _ => {
                    // Literal tab or line end: one space (CR LF counts as one line end).
                    self.pos += if self.rest().starts_with("\r\n") {
                        2
                    } else {
                        1
                    };
                    value.push(' ');
                }
            }
        }
    }

    /// Reads a start tag at `<`; returns the element and whether it was empty (`/>`).
    fn start_tag(&mut self) -> Result<(XmlElement, bool), VarError> {
        self.pos += 1;
        let mut element = XmlElement::with_valid_tag(self.name()?.to_owned());
        let mut names: Vec<&'t str> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        loop {
            let spaced = self.skip_space();
            if self.rest().starts_with("/>") {
                self.pos += 2;
                return Ok((element, true));
            }
            if self.rest().starts_with('>') {
                self.pos += 1;
                return Ok((element, false));
            }
            if self.rest().is_empty() {
                return Err(self.malformed(self.pos, "unterminated start tag"));
            }
            if !spaced {
                return Err(self.malformed(self.pos, "expected space, `>` or `/>`"));
            }
            let name_at = self.pos;
            let name = self.name()?;
            self.skip_space();
            if !self.rest().starts_with('=') {
                return Err(self.malformed(self.pos, "expected `=` after the attribute name"));
            }
            self.pos += 1;
            self.skip_space();
            let value = self.attribute_value()?;
            // Linear search for the usual few attributes, a set beyond that (no O(n^2)).
            let duplicate = if names.len() < 8 {
                names.contains(&name)
            } else {
                if seen.is_empty() {
                    seen.extend(names.iter().copied());
                }
                !seen.insert(name)
            };
            names.push(name);
            if duplicate {
                return Err(self.malformed(name_at, format!("duplicate attribute {name:?}")));
            }
            element
                .attributes
                .push(XmlAttribute::with_valid_name(name.to_owned(), value));
        }
    }

    /// Moves pending text into the innermost open element.
    fn flush_text(&self, text: &mut String, open: &mut [XmlElement]) {
        if text.is_empty() {
            return;
        }
        let text = std::mem::take(text);
        if self.ignore_empty_text && text.chars().all(is_space) {
            return;
        }
        if let Some(parent) = open.last_mut() {
            parent.children.push(XmlNode::text(text));
        }
    }

    /// Skips a comment, processing instruction or (before the root) DOCTYPE at `self.pos`;
    /// returns `false` when none starts here.
    fn misc(&mut self, doctype_allowed: bool) -> Result<bool, VarError> {
        let rest = self.rest();
        if rest.starts_with("<!--") {
            self.skip_past(4, "-->", "comment")?;
        } else if rest.starts_with("<?") {
            let at = self.pos;
            self.pos += 2;
            self.name()?;
            self.pos = at;
            self.skip_past(2, "?>", "processing instruction")?;
        } else if doctype_allowed && rest.starts_with("<!DOCTYPE") {
            self.skip_doctype()?;
        } else {
            return Ok(false);
        }
        Ok(true)
    }

    /// Skips a DOCTYPE, its quoted ids and its internal subset, without interpreting them.
    fn skip_doctype(&mut self) -> Result<(), VarError> {
        let start = self.pos;
        self.pos += "<!DOCTYPE".len();
        let mut in_subset = false;
        loop {
            let rest = self.rest();
            let Some(c) = rest.chars().next() else {
                return Err(self.malformed(start, "unterminated DOCTYPE"));
            };
            if in_subset && (rest.starts_with("<!--") || rest.starts_with("<?")) {
                let (from, end) = if rest.starts_with("<?") {
                    (2, "?>")
                } else {
                    (4, "-->")
                };
                self.skip_past(from, end, "DOCTYPE declaration")?;
                continue;
            }
            match c {
                '"' | '\'' => {
                    self.skip_past(1, if c == '"' { "\"" } else { "'" }, "DOCTYPE literal")?;
                    continue;
                }
                '[' if !in_subset => in_subset = true,
                ']' if in_subset => in_subset = false,
                '>' if !in_subset => {
                    self.pos += 1;
                    return Ok(());
                }
                _ => {}
            }
            self.pos += c.len_utf8();
        }
    }

    fn document(mut self) -> Result<XmlElement, VarError> {
        let mut root: Option<XmlElement> = None;
        let mut doctype_allowed = true;
        // Prolog and epilog: only white space, comments, PIs and (before the root) a DOCTYPE.
        loop {
            self.skip_space();
            if self.rest().is_empty() {
                return root.ok_or_else(|| {
                    self.error_at(self.pos, XML_ROOT_COUNT, "no root element".into())
                });
            }
            let was_doctype = self.rest().starts_with("<!DOCTYPE");
            if self.misc(doctype_allowed && root.is_none())? {
                doctype_allowed &= !was_doctype;
                continue;
            }
            if !self.rest().starts_with('<') || self.rest()[1..].starts_with(['!', '/']) {
                return Err(self.malformed(self.pos, "content outside the root element"));
            }
            if root.is_some() {
                let message = "more than one root element".into();
                return Err(self.error_at(self.pos, XML_ROOT_COUNT, message));
            }
            let at = self.pos;
            let (element, empty) = self.start_tag()?;
            if self.only_outer {
                return Ok(element);
            }
            root = Some(if empty {
                element
            } else {
                self.content(element, at)?
            });
        }
    }

    /// Reads the content of `top` (whose start tag was just read) through its end tag.
    /// `top_at` is where its start tag began, for the unclosed-element error.
    fn content(&mut self, top: XmlElement, top_at: usize) -> Result<XmlElement, VarError> {
        let mut open = vec![top];
        let mut open_at = vec![top_at];
        let mut text = String::new();
        loop {
            let rest = self.rest();
            if rest.is_empty() {
                let tag = open.last().map_or("", XmlElement::tag_name);
                let at = open_at.last().copied().unwrap_or(0);
                return Err(self.malformed(at, format!("unclosed element <{tag}>")));
            }
            if rest.starts_with("</") {
                let at = self.pos;
                self.pos += 2;
                let name = self.name()?;
                self.skip_space();
                if !self.rest().starts_with('>') {
                    return Err(self.malformed(self.pos, "expected `>` to close the end tag"));
                }
                self.pos += 1;
                self.flush_text(&mut text, &mut open);
                let element = open.pop().expect("an element is open");
                open_at.pop();
                if element.tag != name {
                    let message = format!("</{name}> does not close <{}>", element.tag);
                    return Err(self.malformed(at, message));
                }
                match open.last_mut() {
                    Some(parent) => parent.children.push(element.into()),
                    None => return Ok(element),
                }
            } else if rest.starts_with("<![CDATA[") {
                push_line_ends_normalised(&mut text, self.skip_past(9, "]]>", "CDATA section")?);
            } else if rest.starts_with('<') {
                if self.misc(false)? {
                    continue;
                }
                let at = self.pos;
                self.flush_text(&mut text, &mut open);
                let (element, empty) = self.start_tag()?;
                if empty {
                    let parent = open.last_mut().expect("an element is open");
                    parent.children.push(element.into());
                } else {
                    open.push(element);
                    open_at.push(at);
                }
            } else if rest.starts_with('&') {
                self.reference(&mut text)?;
            } else {
                let run = rest.find(['<', '&', '\r']).unwrap_or(rest.len());
                text.push_str(&rest[..run]);
                self.pos += run;
                if self.rest().starts_with('\r') {
                    text.push('\n');
                    self.pos += if self.rest().starts_with("\r\n") {
                        2
                    } else {
                        1
                    };
                }
            }
        }
    }
}

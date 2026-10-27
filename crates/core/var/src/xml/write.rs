//! The XML writer: [`TextFormat`] and [`XmlElement::to_string`]. It walks the tree with an
//! explicit stack, so any depth can be written.

use std::fmt::Write as _;

use super::node::NodeRepr;
use super::{XmlElement, XmlNode};

/// The header [`TextFormat`] writes when it has no custom header.
const DEFAULT_HEADER: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>";

/// How [`XmlElement::to_string`] lays out a document.
///
/// The output, in order: the header line (the custom header if set, else the default
/// `<?xml version="1.0" encoding="UTF-8"?>` when `add_default_header` is on), the DTD line if
/// set, then the element and a final line end. Each line ends with `new_line_chars`.
///
/// Elements are written one per line, indented by `indentation` spaces per level. An element
/// with no children uses the short form `<a/>`. An element with any text child is written on
/// one line with its whole subtree (no added line ends or indentation), so text is never
/// changed by layout. When a start tag would pass `line_wrap` bytes, later attributes move to a
/// new line aligned under the first one (0 turns wrapping off).
///
/// Escaping: `&`, `<`, `>` and, in attribute values, `"` become entities; control characters
/// (Unicode `Cc`) become decimal references, except tab and line feed in text; everything else
/// is written as UTF-8. `'` is not escaped.
///
/// Defaults (Syrp's choice where JUCE's documentation is silent; listed in the crate's
/// `AGENTS.md`): default header on, no DTD, `line_wrap` 60, `new_line_chars` `"\r\n"`,
/// `indentation` 2.
///
/// ```
/// use syrp_var::xml::{TextFormat, XmlElement};
///
/// let mut root = XmlElement::new("a")?;
/// root.create_new_child_element("b")?.set_attribute("x", "1 < 2")?;
/// root.create_new_child_element("c")?.add_text_element("hi");
/// let text = root.to_string(&TextFormat::new().with_new_line_chars("\n"));
/// assert_eq!(text, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<a>\n  <b x=\"1 &lt; 2\"/>\n  <c>hi</c>\n</a>\n");
/// assert_eq!(root.to_string(&TextFormat::new().single_line().without_header()),
///            "<a><b x=\"1 &lt; 2\"/><c>hi</c></a>");
/// # Ok::<(), syrp_var::VarError>(())
/// ```
#[doc(alias = "XmlElement::TextFormat")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextFormat {
    dtd: String,
    custom_header: String,
    add_default_header: bool,
    line_wrap: usize,
    new_line_chars: String,
    indentation: usize,
}

impl Default for TextFormat {
    /// Same as [`TextFormat::new`].
    fn default() -> Self {
        Self::new()
    }
}

impl TextFormat {
    /// The default format (see the type documentation).
    ///
    /// ```
    /// use syrp_var::xml::TextFormat;
    /// let f = TextFormat::new();
    /// assert!(f.add_default_header());
    /// assert_eq!((f.line_wrap(), f.new_line_chars(), f.indentation()), (60, "\r\n", 2));
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self {
            dtd: String::new(),
            custom_header: String::new(),
            add_default_header: true,
            line_wrap: 60,
            new_line_chars: "\r\n".to_owned(),
            indentation: 2,
        }
    }

    /// This format with no line ends, indentation or wrapping.
    ///
    /// ```
    /// use syrp_var::xml::TextFormat;
    /// let f = TextFormat::new().single_line();
    /// assert_eq!((f.new_line_chars(), f.indentation(), f.line_wrap()), ("", 0, 0));
    /// ```
    #[doc(alias = "singleLine")]
    #[must_use]
    pub fn single_line(self) -> Self {
        Self {
            line_wrap: 0,
            new_line_chars: String::new(),
            indentation: 0,
            ..self
        }
    }

    /// This format with no header line (default or custom); a DTD is kept.
    ///
    /// ```
    /// use syrp_var::xml::TextFormat;
    /// let f = TextFormat::new().with_custom_header("<?xml?>").without_header();
    /// assert!(!f.add_default_header() && f.custom_header().is_empty());
    /// ```
    #[doc(alias = "withoutHeader")]
    #[must_use]
    pub fn without_header(self) -> Self {
        Self {
            add_default_header: false,
            custom_header: String::new(),
            ..self
        }
    }

    /// Sets the DTD line written after the header (empty for none).
    ///
    /// ```
    /// use syrp_var::xml::TextFormat;
    /// assert_eq!(TextFormat::new().with_dtd("<!DOCTYPE a>").dtd(), "<!DOCTYPE a>");
    /// ```
    #[must_use]
    pub fn with_dtd(mut self, dtd: impl Into<String>) -> Self {
        self.dtd = dtd.into();
        self
    }

    /// Sets a header line written instead of the default one (empty for none).
    ///
    /// ```
    /// use syrp_var::xml::TextFormat;
    /// let f = TextFormat::new().with_custom_header("<?xml version=\"1.0\"?>");
    /// assert_eq!(f.custom_header(), "<?xml version=\"1.0\"?>");
    /// ```
    #[must_use]
    pub fn with_custom_header(mut self, header: impl Into<String>) -> Self {
        self.custom_header = header.into();
        self
    }

    /// Sets whether the default header is written when there is no custom header.
    ///
    /// ```
    /// use syrp_var::xml::TextFormat;
    /// assert!(!TextFormat::new().with_default_header(false).add_default_header());
    /// ```
    #[must_use]
    pub fn with_default_header(mut self, add: bool) -> Self {
        self.add_default_header = add;
        self
    }

    /// Sets the line length, in bytes, past which attributes wrap (0: never).
    ///
    /// ```
    /// use syrp_var::xml::TextFormat;
    /// assert_eq!(TextFormat::new().with_line_wrap(0).line_wrap(), 0);
    /// ```
    #[must_use]
    pub fn with_line_wrap(mut self, bytes: usize) -> Self {
        self.line_wrap = bytes;
        self
    }

    /// Sets the line end; empty writes everything on one line.
    ///
    /// ```
    /// use syrp_var::xml::TextFormat;
    /// assert_eq!(TextFormat::new().with_new_line_chars("\n").new_line_chars(), "\n");
    /// ```
    #[must_use]
    pub fn with_new_line_chars(mut self, new_line: impl Into<String>) -> Self {
        self.new_line_chars = new_line.into();
        self
    }

    /// Sets the indentation per nesting level, in spaces.
    ///
    /// ```
    /// use syrp_var::xml::TextFormat;
    /// assert_eq!(TextFormat::new().with_indentation(4).indentation(), 4);
    /// ```
    #[must_use]
    pub fn with_indentation(mut self, spaces: usize) -> Self {
        self.indentation = spaces;
        self
    }

    /// The DTD line (empty for none).
    ///
    /// ```
    /// assert_eq!(syrp_var::xml::TextFormat::new().dtd(), "");
    /// ```
    #[must_use]
    pub fn dtd(&self) -> &str {
        &self.dtd
    }

    /// The custom header line (empty for none).
    ///
    /// ```
    /// assert_eq!(syrp_var::xml::TextFormat::new().custom_header(), "");
    /// ```
    #[must_use]
    pub fn custom_header(&self) -> &str {
        &self.custom_header
    }

    /// Whether the default header is written when there is no custom one.
    ///
    /// ```
    /// assert!(syrp_var::xml::TextFormat::new().add_default_header());
    /// ```
    #[must_use]
    pub fn add_default_header(&self) -> bool {
        self.add_default_header
    }

    /// The wrap length in bytes (0: never).
    ///
    /// ```
    /// assert_eq!(syrp_var::xml::TextFormat::new().line_wrap(), 60);
    /// ```
    #[must_use]
    pub fn line_wrap(&self) -> usize {
        self.line_wrap
    }

    /// The line end.
    ///
    /// ```
    /// assert_eq!(syrp_var::xml::TextFormat::new().new_line_chars(), "\r\n");
    /// ```
    #[must_use]
    pub fn new_line_chars(&self) -> &str {
        &self.new_line_chars
    }

    /// The indentation per level, in spaces.
    ///
    /// ```
    /// assert_eq!(syrp_var::xml::TextFormat::new().indentation(), 2);
    /// ```
    #[must_use]
    pub fn indentation(&self) -> usize {
        self.indentation
    }
}

/// Appends `text` with XML escaping; `attribute` also escapes `"`, tab and line feed.
fn escape(out: &mut String, text: &str, attribute: bool) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if attribute => out.push_str("&quot;"),
            '\t' | '\n' if !attribute => out.push(c),
            c if c.is_control() => {
                let _ = write!(out, "&#{};", u32::from(c));
            }
            c => out.push(c),
        }
    }
}

/// The writer's state: the output and where the current line starts.
struct Writer<'f> {
    out: String,
    line_start: usize,
    format: &'f TextFormat,
}

impl Writer<'_> {
    fn new_line(&mut self, depth: usize) {
        if self.format.new_line_chars.is_empty() {
            return;
        }
        self.out.push_str(&self.format.new_line_chars);
        self.line_start = self.out.len();
        let spaces = depth.saturating_mul(self.format.indentation);
        self.out.extend(std::iter::repeat_n(' ', spaces));
    }

    /// Writes `<tag attributes`, wrapping attributes past the line length.
    fn start_tag(&mut self, element: &XmlElement) {
        self.out.push('<');
        self.out.push_str(&element.tag);
        let align = self.out.len() - self.line_start + 1;
        let wrap = self.format.line_wrap;
        let can_wrap = wrap > 0 && !self.format.new_line_chars.is_empty();
        for (i, attribute) in element.attributes.iter().enumerate() {
            let mut piece = String::new();
            piece.push_str(attribute.name());
            piece.push_str("=\"");
            escape(&mut piece, attribute.value(), true);
            piece.push('"');
            if can_wrap && i > 0 && self.out.len() - self.line_start + 1 + piece.len() > wrap {
                self.out.push_str(&self.format.new_line_chars);
                self.line_start = self.out.len();
                self.out.extend(std::iter::repeat_n(' ', align));
            } else {
                self.out.push(' ');
            }
            self.out.push_str(&piece);
        }
    }

    fn document(mut self, root: &XmlElement) -> String {
        let format = self.format;
        let header = if format.custom_header.is_empty() && format.add_default_header {
            DEFAULT_HEADER
        } else {
            &format.custom_header
        };
        for line in [header, &format.dtd] {
            if !line.is_empty() {
                self.out.push_str(line);
                self.new_line(0);
            }
        }
        self.element(root);
        self.new_line(0);
        self.out
    }

    /// Writes `root` and its subtree with an explicit stack.
    fn element(&mut self, root: &XmlElement) {
        // Each frame: the element, its next child, and whether its children are inline.
        let mut stack: Vec<(&XmlElement, usize, bool)> = Vec::new();
        let mut pending = Some((root, false));
        loop {
            if let Some((element, parent_inline)) = pending.take() {
                self.start_tag(element);
                if element.children.is_empty() {
                    self.out.push_str("/>");
                } else {
                    self.out.push('>');
                    let inline =
                        parent_inline || element.children.iter().any(XmlNode::is_text_element);
                    stack.push((element, 0, inline));
                }
            }
            let depth = stack.len();
            let Some((element, next, inline)) = stack.last_mut() else {
                return;
            };
            let (element, inline) = (*element, *inline);
            if let Some(child) = element.children.get(*next) {
                *next += 1;
                if !inline {
                    self.new_line(depth);
                }
                match &child.repr {
                    NodeRepr::Text(text) => escape(&mut self.out, text, false),
                    NodeRepr::Element(child) => pending = Some((child, inline)),
                }
            } else {
                stack.pop();
                if !inline {
                    self.new_line(depth - 1);
                }
                self.out.push_str("</");
                self.out.push_str(&element.tag);
                self.out.push('>');
            }
        }
    }
}

impl XmlElement {
    /// Writes this element as an XML document laid out by `format`.
    ///
    /// ```
    /// use syrp_var::xml::{TextFormat, XmlElement};
    /// let mut e = XmlElement::new("a")?;
    /// e.set_attribute("q", "\"it's\" & <more>")?;
    /// e.add_text_element("1 < 2 & \u{1}");
    /// let text = e.to_string(&TextFormat::new().without_header());
    /// assert_eq!(text, "<a q=\"&quot;it's&quot; &amp; &lt;more&gt;\">1 &lt; 2 &amp; &#1;</a>\r\n");
    /// # Ok::<(), syrp_var::VarError>(())
    /// ```
    #[doc(alias = "toString")]
    #[doc(alias = "createDocument")]
    #[must_use]
    pub fn to_string(&self, format: &TextFormat) -> String {
        Writer {
            out: String::new(),
            line_start: 0,
            format,
        }
        .document(self)
    }

    /// Writes the [`to_string`](Self::to_string) text to `sink` as UTF-8.
    ///
    /// # Errors
    /// Any error from `sink`.
    ///
    /// ```
    /// use syrp_var::xml::{TextFormat, XmlElement};
    /// let mut bytes = Vec::new();
    /// XmlElement::new("a")?.write_to(&mut bytes, &TextFormat::new().single_line())?;
    /// assert_eq!(bytes, b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><a/>");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[doc(alias = "writeTo")]
    pub fn write_to(
        &self,
        sink: &mut impl std::io::Write,
        format: &TextFormat,
    ) -> std::io::Result<()> {
        sink.write_all(self.to_string(format).as_bytes())
    }
}

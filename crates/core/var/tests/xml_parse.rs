//! B4-B5 (and the parse codes of B8): the XML parser.

use proptest::prelude::*;
use syrp_var::xml::{XmlDocument, XmlElement};
use syrp_var::{Diagnostic, VarError};

fn parse(text: &str) -> XmlElement {
    XmlDocument::parse(text).unwrap()
}

fn fails(text: &str) -> VarError {
    XmlDocument::parse(text).unwrap_err()
}

/// Asserts the error code and 1-based position of a malformed document.
fn fails_at(text: &str, code: &str, line: usize, column: usize) {
    let e = fails(text);
    assert_eq!(
        (e.code().as_str(), e.line(), e.column()),
        (code, line, column),
        "{text:?}: {e}"
    );
}

#[test]
fn b4_parses_elements_attributes_and_text() {
    let root = parse("<a x=\"1\" y='two'><b/>hello<c z=\"\">world</c></a>");
    assert_eq!(root.tag_name(), "a");
    let pairs: Vec<_> = root.attributes().map(|a| (a.name(), a.value())).collect();
    assert_eq!(pairs, [("x", "1"), ("y", "two")]);
    assert_eq!(root.num_children(), 3);
    assert_eq!(root.child(1).unwrap().as_text(), Some("hello"));
    assert_eq!(
        root.child_by_name("c").unwrap().string_attribute("z"),
        Some("")
    );
    assert_eq!(root.all_sub_text(), "helloworld");
}

#[test]
fn b4_entities_and_character_references() {
    let root = parse("<a v=\"&lt;&amp;&gt;&quot;&apos;\">&#65;&#x42;&#x1F600;&#0;&#x9;</a>");
    assert_eq!(root.string_attribute("v"), Some("<&>\"'"));
    assert_eq!(root.all_sub_text(), "AB😀\0\t");
    for bad in [
        "&nbsp;",
        "&#xD800;",
        "&#x110000;",
        "&#99999999999;",
        "&#;",
        "&#x;",
        "&#6a;",
        "&#+5;",
        "&#X43;",
        "&amp",
        "&",
    ] {
        let e = fails(&format!("<a>{bad}</a>"));
        assert_eq!(e.code().as_str(), "SYRP-VAR-0005", "{bad}");
    }
}

#[test]
fn b4_cdata_is_text_and_merges_with_neighbours() {
    let root = parse("<a>x<![CDATA[<b>&amp;]]>y<!-- c -->z&lt;</a>");
    assert_eq!(root.num_children(), 1);
    assert_eq!(root.child(0).unwrap().as_text(), Some("x<b>&amp;yz<"));
}

#[test]
fn b4_declaration_comments_and_processing_instructions_are_dropped() {
    let root = parse(
        "\u{FEFF}<?xml version=\"1.0\" encoding=\"latin1\"?>\n<!-- c -->\n<?pi data?>\n<a><?x?><!--y--></a>\n<!-- after -->\n",
    );
    assert_eq!((root.tag_name(), root.num_children()), ("a", 0));
}

#[test]
fn b4_line_ends_inside_cdata_are_normalised() {
    let root = parse("<a><![CDATA[1\r\n2\r3\n4\r\r\n5\r]]>\r\n<![CDATA[\r]]></a>");
    assert_eq!(root.all_sub_text(), "1\n2\n3\n4\n\n5\n\n\n");
    let kept = XmlDocument::new("<a><![CDATA[\r\n]]></a>").ignore_empty_text_elements(false);
    assert_eq!(kept.document_element().unwrap().all_sub_text(), "\n");
}

#[test]
fn b4_line_ends_and_attribute_whitespace_are_normalised() {
    let root = parse("<a v=\"1\r\n2\t3\n4&#10;5\">x\r\ny\rz</a>");
    assert_eq!(root.string_attribute("v"), Some("1 2 3 4\n5"));
    assert_eq!(root.all_sub_text(), "x\ny\nz");
}

#[test]
fn b4_empty_text_is_ignored_by_default_and_kept_on_request() {
    let text = "<a>\n  <b> </b>\n  <c/>  t  </a>";
    let root = parse(text);
    assert_eq!(root.num_children(), 3);
    assert_eq!(root.child_by_name("b").unwrap().num_children(), 0);
    assert_eq!(root.child(2).unwrap().as_text(), Some("  t  "));
    let kept = XmlDocument::new(text)
        .ignore_empty_text_elements(false)
        .document_element()
        .unwrap();
    assert_eq!(kept.num_children(), 5);
    assert_eq!(kept.all_sub_text(), "\n   \n    t  ");
}

#[test]
fn b4_only_read_outer_element_stops_after_the_start_tag() {
    let doc =
        XmlDocument::new("<?xml version='1.0'?><a v='1'><b>unclosed").only_read_outer_element(true);
    let root = doc.document_element().unwrap();
    assert_eq!((root.int_attribute("v", 0), root.num_children()), (1, 0));
    assert!(XmlDocument::new("<a v='1'><b>").document_element().is_err());
}

#[test]
fn b4_doctype_is_skipped_and_never_expanded() {
    let laughs = "<!DOCTYPE lolz [\n<!ENTITY lol \"lol\">\n<!ENTITY lol1 \"&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;\">\n<!ENTITY lol2 \"&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;\">\n<!ENTITY lol9 \"&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;\">\n]>\n<lolz>&lol9;</lolz>";
    fails_at(laughs, "SYRP-VAR-0005", 7, 7);
    let external = "<!DOCTYPE foo [ <!ENTITY xxe SYSTEM \"file:///etc/passwd\"> ]><foo>&xxe;</foo>";
    assert_eq!(fails(external).code().as_str(), "SYRP-VAR-0005");
    // Declared but unused entities, an external DTD id, and `]>` inside literals and comments
    // are all skipped.
    let ok = "<!DOCTYPE r SYSTEM \"http://example.invalid/r.dtd\" [ <!ENTITY e \"]>\"> <!-- ]> --> <?p ]>?> ]><r>&amp;</r>";
    assert_eq!(parse(ok).all_sub_text(), "&");
    fails_at("<!DOCTYPE a><!DOCTYPE a><a/>", "SYRP-VAR-0005", 1, 13);
    fails_at("<a/><!DOCTYPE a>", "SYRP-VAR-0005", 1, 5);
}

#[test]
fn b4_errors_carry_code_line_and_column() {
    let cases = [
        ("", "SYRP-VAR-0007", 1, 1),
        ("  <!-- only -->\n", "SYRP-VAR-0007", 2, 1),
        ("<a/>\n<b/>", "SYRP-VAR-0007", 2, 1),
        ("text<a/>", "SYRP-VAR-0005", 1, 1),
        ("<a/>text", "SYRP-VAR-0005", 1, 5),
        ("<a>\n <b>\n</a>", "SYRP-VAR-0005", 3, 1),
        ("<a>\n  <b>", "SYRP-VAR-0005", 2, 3),
        ("<a x='1' x='2'/>", "SYRP-VAR-0005", 1, 10),
        ("<a x='1'y='2'/>", "SYRP-VAR-0005", 1, 9),
        ("<a x=1/>", "SYRP-VAR-0005", 1, 6),
        ("<a x/>", "SYRP-VAR-0005", 1, 5),
        ("<a x='<'/>", "SYRP-VAR-0005", 1, 7),
        ("<a x='1/>", "SYRP-VAR-0005", 1, 6),
        ("<1a/>", "SYRP-VAR-0005", 1, 2),
        ("<a><1/></a>", "SYRP-VAR-0005", 1, 5),
        ("<é>\n\t<ü 2='x'/></é>", "SYRP-VAR-0005", 2, 5),
        ("<a><!-- x </a>", "SYRP-VAR-0005", 1, 4),
        ("<a><![CDATA[x</a>", "SYRP-VAR-0005", 1, 4),
        ("<a><? x?></a>", "SYRP-VAR-0005", 1, 6),
        ("<a></a  >x", "SYRP-VAR-0005", 1, 10),
        ("<a></a x>", "SYRP-VAR-0005", 1, 8),
        ("<a", "SYRP-VAR-0005", 1, 3),
        ("<!DOCTYPE a [", "SYRP-VAR-0005", 1, 1),
    ];
    for (text, code, line, column) in cases {
        fails_at(text, code, line, column);
    }
    let many: String = (0..20).map(|i| format!(" a{i}='{i}'")).collect();
    fails_at(
        &format!("<e{many} a19='x'/>"),
        "SYRP-VAR-0005",
        1,
        3 + many.len() + 1,
    );
    assert_eq!(parse(&format!("<e{many}/>")).num_attributes(), 20);
}

#[test]
fn b4_parse_bytes_detects_the_encoding() {
    let text = "<a>hé ☺ 😀</a>";
    let utf16 = |big: bool, bom: bool| -> Vec<u8> {
        let units = bom
            .then_some(0xFEFF_u16)
            .into_iter()
            .chain(text.encode_utf16());
        units
            .flat_map(|u| {
                if big {
                    u.to_be_bytes()
                } else {
                    u.to_le_bytes()
                }
            })
            .collect()
    };
    let mut utf8_bom = vec![0xEF, 0xBB, 0xBF];
    utf8_bom.extend_from_slice(text.as_bytes());
    let inputs = [
        text.as_bytes().to_vec(),
        utf8_bom,
        utf16(true, true),
        utf16(false, true),
        utf16(true, false),
        utf16(false, false),
    ];
    for bytes in inputs {
        assert_eq!(
            XmlDocument::parse_bytes(&bytes).unwrap().all_sub_text(),
            "hé ☺ 😀"
        );
    }
    for name in ["UTF-8", "utf-8", "UTF-16", "utf-16le", "UTF-16BE"] {
        let doc = format!("<?xml version='1.0' encoding=\"{name}\"?><a/>");
        assert!(XmlDocument::parse_bytes(doc.as_bytes()).is_ok(), "{name}");
    }
    for name in ["ISO-8859-1", "windows-1252", "US-ASCII", ""] {
        let doc = format!("<?xml version='1.0' encoding='{name}'?><a/>");
        let e = XmlDocument::parse_bytes(doc.as_bytes()).unwrap_err();
        assert_eq!(e.code().as_str(), "SYRP-VAR-0006", "{name}");
    }
}

#[test]
fn b4_parse_bytes_rejects_invalid_unicode_with_a_position() {
    let e = XmlDocument::parse_bytes(b"<a>\nok\xFF</a>").unwrap_err();
    assert_eq!(
        (e.code().as_str(), e.line(), e.column()),
        ("SYRP-VAR-0005", 2, 3)
    );
    let odd = [0xFF, 0xFE, b'<', 0, b'a', 0, b'/', 0, b'>', 0, b' '];
    assert_eq!(
        XmlDocument::parse_bytes(&odd).unwrap_err().code().as_str(),
        "SYRP-VAR-0005"
    );
    let lone: Vec<u8> = [
        0xFFFE_u16.swap_bytes(),
        u16::from(b'<'),
        0xD800,
        u16::from(b'a'),
    ]
    .into_iter()
    .flat_map(u16::to_le_bytes)
    .collect();
    let e = XmlDocument::parse_bytes(&lone).unwrap_err();
    assert_eq!((e.code().as_str(), e.column()), ("SYRP-VAR-0005", 2));
}

#[test]
fn b5_parses_100_000_levels_without_recursion() {
    let depth = 100_000;
    let text = format!("{}x{}", "<n>".repeat(depth), "</n>".repeat(depth));
    let root = parse(&text);
    assert_eq!(root.all_sub_text(), "x");
    let unclosed = format!("{}x", "<n>".repeat(depth));
    fails_at(&unclosed, "SYRP-VAR-0005", 1, 3 * depth - 2);
}

/// A small valid document for the mutation test.
const SAMPLE: &str = "<?xml version=\"1.0\"?>\n<!DOCTYPE s [<!ENTITY e \"x\">]>\n<s a=\"1&amp;2\" b='&#x41;'>\n  <c/><![CDATA[<raw>]]><!-- c --><?p i?>\n  t&lt;ext<d e=\"\">\u{e9}</d>\n</s>\n";

proptest! {
    #[test]
    fn b5_random_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..256)) {
        let _ = XmlDocument::parse_bytes(&bytes);
        let _ = XmlDocument::new(&String::from_utf8_lossy(&bytes)).only_read_outer_element(true).document_element();
    }

    #[test]
    fn b5_mutated_documents_never_panic(
        edits in proptest::collection::vec((any::<prop::sample::Index>(), any::<u8>(), 0..3_u8), 1..8),
    ) {
        let mut bytes = SAMPLE.as_bytes().to_vec();
        for (index, byte, kind) in edits {
            let i = index.index(bytes.len() + 1);
            match kind {
                0 => bytes.insert(i, byte),
                1 if i < bytes.len() => { bytes.remove(i); }
                _ if i < bytes.len() => bytes[i] = byte,
                _ => {}
            }
        }
        let _ = XmlDocument::parse_bytes(&bytes);
        let _ = XmlDocument::new(&String::from_utf8_lossy(&bytes)).ignore_empty_text_elements(false).document_element();
    }
}

#[test]
fn b5_sample_document_parses() {
    let root = parse(SAMPLE);
    assert_eq!(root.string_attribute("a"), Some("1&2"));
    assert_eq!(root.string_attribute("b"), Some("A"));
    assert_eq!(root.all_sub_text(), "<raw>\n  t<ext\u{e9}");
}

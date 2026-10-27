//! B6-B7 (and the B5 writer depth check): the XML writer and parse/write round trips.

use proptest::prelude::*;
use syrp_var::xml::{TextFormat, XmlDocument, XmlElement, XmlNode};

type R = Result<(), syrp_var::VarError>;

fn el(tag: &str) -> XmlElement {
    XmlElement::new(tag).unwrap()
}

fn lf() -> TextFormat {
    TextFormat::new().with_new_line_chars("\n")
}

fn sample() -> Result<XmlElement, syrp_var::VarError> {
    let mut root = el("preset");
    root.set_attribute("name", "Pad")?;
    let params = root.create_new_child_element("params")?;
    params
        .create_new_child_element("p")?
        .set_attribute("v", 0.5)?;
    params
        .create_new_child_element("p")?
        .set_attribute("v", 1)?;
    root.create_new_child_element("empty")?;
    root.create_new_child_element("notes")?
        .add_text_element("hi");
    Ok(root)
}

#[test]
fn b6_default_layout_indents_children_and_uses_short_empty_tags() -> R {
    let expected = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<preset name=\"Pad\">\r\n  <params>\r\n    <p v=\"0.5\"/>\r\n    <p v=\"1\"/>\r\n  </params>\r\n  <empty/>\r\n  <notes>hi</notes>\r\n</preset>\r\n";
    assert_eq!(sample()?.to_string(&TextFormat::default()), expected);
    let four = sample()?.to_string(&lf().with_indentation(4).without_header());
    assert!(
        four.starts_with("<preset name=\"Pad\">\n    <params>\n        <p v=\"0.5\"/>"),
        "{four}"
    );
    Ok(())
}

#[test]
fn b6_header_text_syrp_choice() -> R {
    let a = el("a");
    assert_eq!(
        a.to_string(&lf()),
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<a/>\n"
    );
    let custom = lf()
        .with_custom_header("<?xml version=\"1.0\"?>")
        .with_dtd("<!DOCTYPE a>");
    assert_eq!(
        a.to_string(&custom),
        "<?xml version=\"1.0\"?>\n<!DOCTYPE a>\n<a/>\n"
    );
    assert_eq!(
        a.to_string(&custom.clone().without_header()),
        "<!DOCTYPE a>\n<a/>\n"
    );
    assert_eq!(a.to_string(&lf().with_default_header(false)), "<a/>\n");
    Ok(())
}

#[test]
fn b6_single_line_and_without_header() -> R {
    let one = sample()?.to_string(&TextFormat::new().single_line().without_header());
    assert_eq!(
        one,
        "<preset name=\"Pad\"><params><p v=\"0.5\"/><p v=\"1\"/></params><empty/><notes>hi</notes></preset>"
    );
    let with_header = el("a").to_string(&TextFormat::new().single_line());
    assert_eq!(
        with_header,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><a/>"
    );
    Ok(())
}

#[test]
fn b6_escaping_in_text_and_attributes() -> R {
    let mut e = el("e");
    e.set_attribute("a", "&<>\"'\t\n\r\u{0}\u{7F}\u{85}é😀")?;
    e.add_text_element("&<>\"'\t\n\r\u{1}\u{9F}é😀");
    let text = e.to_string(&TextFormat::new().single_line().without_header());
    assert_eq!(
        text,
        "<e a=\"&amp;&lt;&gt;&quot;'&#9;&#10;&#13;&#0;&#127;&#133;é😀\">&amp;&lt;&gt;\"'\t\n&#13;&#1;&#159;é😀</e>"
    );
    assert!(XmlDocument::parse(&text)?.is_equivalent_to(&e, false));
    Ok(())
}

#[test]
fn b6_apostrophe_kept_and_greater_than_escaped_syrp_choice() -> R {
    let mut e = el("e");
    e.set_attribute("a", "'>")?;
    e.add_text_element("'>]]>");
    let text = e.to_string(&TextFormat::new().single_line().without_header());
    assert_eq!(text, "<e a=\"'&gt;\">'&gt;]]&gt;</e>");
    Ok(())
}

#[test]
fn b6_text_children_keep_the_element_on_one_line_syrp_choice() -> R {
    let mut p = el("p");
    p.add_text_element("a ");
    p.create_new_child_element("b")?
        .create_new_child_element("i")?
        .add_text_element("x");
    p.add_text_element(" c");
    let mut root = el("root");
    root.add_child_element(p);
    root.create_new_child_element("q")?
        .create_new_child_element("r")?;
    let text = root.to_string(&lf().without_header());
    assert_eq!(
        text,
        "<root>\n  <p>a <b><i>x</i></b> c</p>\n  <q>\n    <r/>\n  </q>\n</root>\n"
    );
    Ok(())
}

#[test]
fn b6_long_start_tags_wrap_attributes_syrp_choice() -> R {
    let mut e = el("element");
    for name in ["first", "second", "third", "fourth", "fifth"] {
        e.set_attribute(name, "0123456789")?;
    }
    let wrapped = e.to_string(&lf().without_header());
    let expected = "<element first=\"0123456789\" second=\"0123456789\"\n         third=\"0123456789\" fourth=\"0123456789\"\n         fifth=\"0123456789\"/>\n";
    assert_eq!(wrapped, expected);
    assert!(wrapped.lines().all(|l| l.len() <= 60));
    let unwrapped = e.to_string(&lf().without_header().with_line_wrap(0));
    assert_eq!(unwrapped.lines().count(), 1);
    assert!(XmlDocument::parse(&wrapped)?.is_equivalent_to(&e, false));
    Ok(())
}

#[test]
fn b6_write_to_matches_to_string() -> R {
    let format = TextFormat::new().with_indentation(3);
    let mut bytes = Vec::new();
    sample()?.write_to(&mut bytes, &format).unwrap();
    assert_eq!(bytes, sample()?.to_string(&format).into_bytes());
    Ok(())
}

#[test]
fn b5_writer_handles_100_000_levels() {
    let depth = 100_000;
    let mut inner = el("leaf");
    for _ in 0..depth {
        let mut outer = el("n");
        outer.add_child_element(inner);
        inner = outer;
    }
    let text = inner.to_string(&TextFormat::new().single_line().without_header());
    assert_eq!(text.len(), depth * "<n></n>".len() + "<leaf/>".len());
    assert!(
        XmlDocument::parse(&text)
            .unwrap()
            .is_equivalent_to(&inner, false)
    );
    let pretty = inner.to_string(&lf().with_indentation(0));
    assert!(
        XmlDocument::parse(&pretty)
            .unwrap()
            .is_equivalent_to(&inner, false)
    );
}

/// A generated tree: tag, attributes (unique names), children.
#[derive(Clone, Debug)]
enum Gen {
    Text(String),
    Element(String, Vec<(String, String)>, Vec<Gen>),
}

fn name() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-zA-Z_][a-zA-Z0-9_.-]{0,5}",
        "[éßΩ名_:][a-z0-9·\u{300}-]{0,3}",
    ]
}

fn text() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        "[ \t\r\n]{1,4}",
        any::<String>(),
        "([a-z <>&\"'\\]\\[!-]|&amp;|<!\\[CDATA\\[|\\]\\]>|\r\n|\u{0}|\u{1}|é|😀){0,12}",
    ]
}

fn tree() -> impl Strategy<Value = Gen> {
    let leaf = prop_oneof![
        text().prop_map(Gen::Text),
        (
            name(),
            proptest::collection::btree_map(name(), text(), 0..3)
        )
            .prop_map(|(t, a)| Gen::Element(t, a.into_iter().collect(), vec![])),
    ];
    leaf.prop_recursive(5, 48, 6, |inner| {
        (
            name(),
            proptest::collection::btree_map(name(), text(), 0..4),
            proptest::collection::vec(inner, 0..6),
        )
            .prop_map(|(t, a, c)| Gen::Element(t, a.into_iter().collect(), c))
    })
}

fn build(tag: &str, attributes: &[(String, String)], children: &[Gen]) -> XmlElement {
    let mut e = el(tag);
    for (n, v) in attributes {
        e.set_attribute(n, v.as_str()).unwrap();
    }
    for child in children {
        match child {
            Gen::Text(t) => e.add_text_element(t.as_str()),
            Gen::Element(t, a, c) => e.add_child_element(build(t, a, c)),
        }
    }
    e
}

/// The documented normalisation: adjacent text merges, then whitespace-only text is dropped.
fn normalise(e: &XmlElement) -> XmlElement {
    let mut out = el(e.tag_name());
    for a in e.attributes() {
        out.set_attribute(a.name(), a.value()).unwrap();
    }
    let mut pending = String::new();
    let flush = |out: &mut XmlElement, pending: &mut String| {
        if !pending
            .chars()
            .all(|c| matches!(c, ' ' | '\t' | '\r' | '\n'))
        {
            out.add_text_element(pending.as_str());
        }
        pending.clear();
    };
    for child in e.children() {
        match child.as_text() {
            Some(t) => pending.push_str(t),
            None => {
                flush(&mut out, &mut pending);
                out.add_child_element(normalise(child.as_element().unwrap()));
            }
        }
    }
    flush(&mut out, &mut pending);
    out
}

proptest! {
    #[test]
    fn b7_parse_of_written_tree_is_equivalent(generated in tree()) {
        let root = match &generated {
            Gen::Element(t, a, c) => build(t, a, c),
            Gen::Text(t) => {
                let mut e = el("root");
                e.add_child_element(XmlNode::text(t.as_str()));
                e
            }
        };
        let expected = normalise(&root);
        for format in [TextFormat::new(), TextFormat::new().single_line(), TextFormat::new().without_header()] {
            let text = root.to_string(&format);
            let parsed = XmlDocument::parse(&text).unwrap();
            prop_assert!(parsed.is_equivalent_to(&expected, false), "{}", text);
        }
    }
}
